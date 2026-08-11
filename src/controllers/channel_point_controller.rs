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
) -> actix_web::Result<HttpResponse> {
    let (response, mut session, mut stream) = actix_ws::handle(&req, body)?;
    // Abonnement pris avant le spawn : le verrou ne traverse jamais un `.await`.
    let mut alerts = state.lock().unwrap().alerts.subscribe();

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
