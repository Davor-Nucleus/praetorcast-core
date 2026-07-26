use actix_multipart::Multipart;
use actix_web::{web, HttpRequest, HttpResponse, Responder};
use actix_ws::Message;
use askama::Template;
use futures_util::StreamExt;
use std::sync::Mutex;
use tokio::sync::broadcast;
use tokio::time::{sleep, Duration};
use crate::controllers::upload::{save_upload, AUDIO_EXTENSIONS, IMAGE_EXTENSIONS};
use crate::models::channel_point::{self, ChannelPointReward};
use crate::twitch::TwitchState;

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
            HttpResponse::Ok().json(vec![] as Vec<ChannelPointReward>)
        }
    }
}

pub async fn save(rewards: web::Json<Vec<ChannelPointReward>>) -> impl Responder {
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
    let mut redemptions = state.lock().unwrap().redemptions.subscribe();

    // `MessageStream` n'est pas `Send` : on spawne sur le runtime local d'actix
    // plutôt que sur le pool tokio.
    actix_web::rt::spawn(async move {
        // Envoyer la configuration des récompenses au client dès la connexion
        let mut last_rewards = rewards_message();
        if !last_rewards.is_empty() && session.text(last_rewards.clone()).await.is_err() {
            return;
        }

        // Chaque overlay connecté reçoit une copie de chaque échange.
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
                event = redemptions.recv() => {
                    let redemption = match event {
                        Ok(redemption) => redemption,
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            println!("[WS] Overlay à la traîne, {n} redemption(s) ignorée(s)");
                            continue;
                        }
                        Err(broadcast::error::RecvError::Closed) => return,
                    };

                    let msg = serde_json::json!({
                        "type": "redemption",
                        "redemption": redemption
                    }).to_string();
                    println!("[WS] Envoi redemption: {} par {}", redemption.reward_title, redemption.user_name);
                    if session.text(msg).await.is_err() {
                        println!("[WS] Connexion perdue, arrêt de la diffusion");
                        return;
                    }
                }
                // Relecture périodique du fichier : un « Save » dans le configurateur
                // se reflète sans recharger la source OBS, comme pour la bannière.
                _ = sleep(Duration::from_millis(1000)) => {
                    let current = rewards_message();
                    if !current.is_empty() && current != last_rewards {
                        if session.text(current.clone()).await.is_err() {
                            return;
                        }
                        last_rewards = current;
                    }
                }
            }
        }
    });

    Ok(response)
}

/// Sérialise `channel_points.json` en message `rewards`, indexé par titre exact.
/// Renvoie une chaîne vide si le fichier est illisible : mieux vaut conserver la
/// configuration déjà envoyée que de vider l'overlay.
fn rewards_message() -> String {
    match channel_point::read() {
        Ok(rewards) => {
            let mut map = serde_json::Map::new();
            for r in rewards {
                map.insert(r.reward_title.clone(), serde_json::to_value(&r).unwrap_or_default());
            }
            serde_json::json!({"type": "rewards", "rewards": map}).to_string()
        }
        Err(_) => String::new(),
    }
}
