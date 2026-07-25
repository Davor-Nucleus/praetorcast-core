use actix_multipart::Multipart;
use actix_web::{web, HttpRequest, HttpResponse, Responder};
use actix_ws::Message;
use askama::Template;
use bytes::Bytes;
use futures_util::{StreamExt, TryStreamExt};
use std::fs;
use std::io::Write;
use std::sync::Mutex;
use tokio::sync::broadcast;
use uuid::Uuid;
use crate::models::channel_point::{self, ChannelPointReward};
use crate::twitch::TwitchState;

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

pub async fn upload_image(mut payload: Multipart) -> impl Responder {
    if let Err(e) = fs::create_dir_all("public/channelpoint") {
        eprintln!("Error creating channelpoint directory: {}", e);
        return HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": "Failed to create directory"}));
    }

    while let Ok(Some(mut field)) = payload.try_next().await {
        if let Some(filename) = field.content_disposition().get_filename() {
            let ext = filename.rfind('.').map(|i| &filename[i..]).unwrap_or("").to_owned();
            let new_filename = format!("{}{}", Uuid::new_v4(), ext);
            let filepath = format!("public/channelpoint/{}", new_filename);

            match fs::File::create(&filepath) {
                Ok(mut file) => {
                    while let Ok(Some(chunk)) = field.try_next().await {
                        let bytes: Bytes = chunk;
                        if let Err(e) = file.write_all(&bytes) {
                            eprintln!("Error writing chunk: {}", e);
                            return HttpResponse::InternalServerError()
                                .json(serde_json::json!({"error": "Failed to write file"}));
                        }
                    }
                    return HttpResponse::Ok().json(serde_json::json!({
                        "success": true,
                        "path": format!("/public/channelpoint/{}", new_filename)
                    }));
                }
                Err(e) => {
                    eprintln!("Error creating file: {}", e);
                    return HttpResponse::InternalServerError()
                        .json(serde_json::json!({"error": "Failed to create file"}));
                }
            }
        }
    }

    HttpResponse::BadRequest().json(serde_json::json!({"error": "No file provided"}))
}

pub async fn upload_sound(mut payload: Multipart) -> impl Responder {
    if let Err(e) = fs::create_dir_all("public/channelpoint") {
        eprintln!("Error creating channelpoint directory: {}", e);
        return HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": "Failed to create directory"}));
    }

    while let Ok(Some(mut field)) = payload.try_next().await {
        if let Some(filename) = field.content_disposition().get_filename() {
            let ext = filename.rfind('.').map(|i| &filename[i..]).unwrap_or("").to_owned();
            let new_filename = format!("{}{}", Uuid::new_v4(), ext);
            let filepath = format!("public/channelpoint/{}", new_filename);

            match fs::File::create(&filepath) {
                Ok(mut file) => {
                    while let Ok(Some(chunk)) = field.try_next().await {
                        let bytes: Bytes = chunk;
                        if let Err(e) = file.write_all(&bytes) {
                            eprintln!("Error writing chunk: {}", e);
                            return HttpResponse::InternalServerError()
                                .json(serde_json::json!({"error": "Failed to write file"}));
                        }
                    }
                    return HttpResponse::Ok().json(serde_json::json!({
                        "success": true,
                        "path": format!("/public/channelpoint/{}", new_filename)
                    }));
                }
                Err(e) => {
                    eprintln!("Error creating file: {}", e);
                    return HttpResponse::InternalServerError()
                        .json(serde_json::json!({"error": "Failed to create file"}));
                }
            }
        }
    }

    HttpResponse::BadRequest().json(serde_json::json!({"error": "No file provided"}))
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
        let rewards_config = match crate::models::channel_point::read() {
            Ok(rewards) => {
                let mut map = serde_json::Map::new();
                for r in rewards {
                    map.insert(r.reward_title.clone(), serde_json::to_value(&r).unwrap_or_default());
                }
                serde_json::json!({"type": "rewards", "rewards": map}).to_string()
            }
            Err(_) => String::new(),
        };
        if !rewards_config.is_empty() {
            let _ = session.text(rewards_config).await;
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
            }
        }
    });

    Ok(response)
}
