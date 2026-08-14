use actix_multipart::Multipart;
use actix_web::{web, HttpRequest, HttpResponse, Responder};
use actix_ws::Message;
use askama::Template;
use futures_util::StreamExt;
use std::sync::Mutex;
use tokio::sync::broadcast;
use tokio::time::{sleep, Duration};
use crate::controllers::upload::{save_upload, AUDIO_EXTENSIONS, IMAGE_EXTENSIONS};
use crate::models::channel_point::{self, Alert};
use crate::twitch::{AlertEvent, TwitchState};

const CHANNELPOINT_DIR: &str = "public/channelpoint";
const CHANNELPOINT_URL: &str = "/public/channelpoint";

/// Alertes de test gardées pour un overlay momentanément à la traîne. Plus petit que
/// `ALERT_BUFFER` : les tests viennent d'un clic humain, pas d'une rafale de raid.
const TEST_BUFFER: usize = 8;

/// Diffusion des alertes de **test**, déclenchées par le configurateur.
///
/// Canal distinct de `TwitchState::alerts` pour une raison de fond : ce dernier est
/// aussi consommé par la tâche subathon, où un cheer de test rallongerait pour de vrai
/// le compte à rebours du direct. Ici, aucun consommateur en dehors des overlays.
///
/// La charge utile est le message WebSocket **déjà formé**, et non un `AlertEvent` :
/// la ligne à jouer est celle affichée dans le configurateur, il n'y a donc rien à
/// résoudre contre `channel_points.json`.
pub struct AlertTest(pub broadcast::Sender<String>);

impl Default for AlertTest {
    fn default() -> Self {
        Self(broadcast::channel(TEST_BUFFER).0)
    }
}

#[derive(Template)]
#[template(path = "channel_point_config.html")]
struct ChannelPointConfigTemplate;

pub async fn page() -> impl Responder {
    let html = ChannelPointConfigTemplate.render().unwrap();
    HttpResponse::Ok().content_type("text/html").body(html)
}

pub async fn get() -> impl Responder {
    match channel_point::read() {
        Ok(rewards) => HttpResponse::Ok().json(rewards),
        Err(e) => {
            eprintln!("{}", e);
            HttpResponse::Ok().json(vec![] as Vec<Alert>)
        }
    }
}

pub async fn save(rewards: web::Json<Vec<Alert>>) -> impl Responder {
    match channel_point::write(rewards.into_inner()) {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({"success": true})),
        Err(e) => {
            eprintln!("{}", e);
            HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": "Failed to save channel points config"}))
        }
    }
}

/// Joue une alerte dans les overlays ouverts, sans attendre un vrai événement Twitch.
///
/// La ligne testée arrive dans le corps de la requête plutôt que d'être relue dans
/// `channel_points.json` : c'est ce qui permet d'essayer une phrase ou un son **avant**
/// d'enregistrer. La contrepartie est que `channel_point::select` n'est pas exercé — le
/// choix du palier reste couvert par ses tests, et un test ne peut pas se tromper de
/// ligne puisque le bouton en désigne une.
///
/// La réponse renvoie le nombre d'overlays touchés. C'est *la* explication d'un test
/// resté sans effet : aucune source `/channel-points` ouverte dans OBS ni dans un
/// onglet. Le dire vaut mieux que laisser chercher.
pub async fn test(alert: web::Json<Alert>, tests: web::Data<AlertTest>) -> impl Responder {
    let alert = channel_point::normalized(alert.into_inner());
    let kind = alert.kind;
    let event = AlertEvent::sample(kind, &alert.reward_title, alert.min_amount);

    let message = serde_json::json!({
        "type": "alert",
        "event": event,
        "config": alert,
    })
    .to_string();

    // `send` échoue quand le canal n'a aucun abonné : ce n'est pas une erreur de
    // serveur, juste un overlay fermé.
    let overlays = tests.0.send(message).unwrap_or(0);
    println!("[Test] Alerte {kind:?} envoyée à {overlays} overlay(s)");

    HttpResponse::Ok().json(serde_json::json!({ "success": true, "overlays": overlays }))
}

pub async fn upload_image(payload: Multipart) -> impl Responder {
    save_upload(payload, CHANNELPOINT_DIR, CHANNELPOINT_URL, IMAGE_EXTENSIONS).await
}

pub async fn upload_sound(payload: Multipart) -> impl Responder {
    save_upload(payload, CHANNELPOINT_DIR, CHANNELPOINT_URL, AUDIO_EXTENSIONS).await
}

pub async fn redemption_ws(
    req: HttpRequest,
    body: web::Payload,
    state: web::Data<Mutex<TwitchState>>,
    tests: web::Data<AlertTest>,
) -> actix_web::Result<HttpResponse> {
    let (response, mut session, mut stream) = actix_ws::handle(&req, body)?;
    // Abonnement pris avant le spawn : le verrou ne traverse jamais un `.await`.
    let mut alerts = state.lock().unwrap().alerts.subscribe();
    let mut tests = tests.0.subscribe();

    // `MessageStream` n'est pas `Send` : on spawne sur le runtime local d'actix
    // plutôt que sur le pool tokio.
    actix_web::rt::spawn(async move {
        // Chaque overlay connecté reçoit une copie de chaque événement.
        loop {
            tokio::select! {
                // Le flux entrant doit être consommé : sans ça la fermeture d'une
                // source OBS passe inaperçue et l'abonnement survit à la connexion.
                incoming = stream.next() => match incoming {
                    Some(Ok(Message::Ping(bytes))) => {
                        if session.pong(&bytes).await.is_err() {
                            return;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        let _ = session.close(None).await;
                        return;
                    }
                    Some(Err(_)) => return,
                    _ => {}
                },
                event = alerts.recv() => {
                    let alert = match event {
                        Ok(alert) => alert,
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            println!("[WS] Overlay à la traîne, {n} alerte(s) ignorée(s)");
                            continue;
                        }
                        Err(broadcast::error::RecvError::Closed) => return,
                    };

                    let Some(msg) = alert_message(&alert) else { continue };
                    println!("[WS] Envoi alerte {:?} pour {}", alert.kind, alert.user_name);
                    if session.text(msg).await.is_err() {
                        println!("[WS] Connexion perdue, arrêt de la diffusion");
                        return;
                    }
                }
                // Les tests du configurateur arrivent déjà mis en forme : le message
                // est identique à celui d'un vrai événement, l'overlay ne distingue
                // donc pas les deux chemins — c'est tout l'intérêt du bouton.
                test = tests.recv() => {
                    let message = match test {
                        Ok(message) => message,
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            println!("[Test] Overlay à la traîne, {n} test(s) ignoré(s)");
                            continue;
                        }
                        Err(broadcast::error::RecvError::Closed) => return,
                    };

                    if session.text(message).await.is_err() {
                        println!("[WS] Connexion perdue, arrêt de la diffusion");
                        return;
                    }
                }
                // Le flux entrant doit rester consommé même sans événement : sans ce
                // réveil régulier, un `select!` bloqué sur `recv()` ne remarquerait
                // la fermeture d'une source OBS qu'au prochain événement.
                _ = sleep(Duration::from_millis(1000)) => {}
            }
        }
    });

    Ok(response)
}

/// Message WebSocket pour une alerte, ou `None` si rien n'est configuré pour elle.
///
/// La résolution se fait ici et pas dans l'overlay : `channel_point::select` est
/// testable sans navigateur, et la version JS retéléchargeait la configuration à
/// chaque titre inconnu — avec des `kind` sans ligne configurée, ç'aurait été une
/// requête par événement.
///
/// Un fichier illisible ne fait rien passer plutôt que de jouer une alerte au
/// hasard : l'événement est perdu, ce qui reste préférable à un faux positif.
fn alert_message(alert: &AlertEvent) -> Option<String> {
    let alerts = channel_point::read().ok()?;
    let config = channel_point::select(
        &alerts,
        alert.kind,
        &alert.reward_title,
        alert.amount,
        alert.months,
    )?;

    Some(
        serde_json::json!({
            "type": "alert",
            "event": alert,
            "config": config,
        })
        .to_string(),
    )
}
