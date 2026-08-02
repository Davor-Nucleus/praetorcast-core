use std::sync::Mutex;
use actix_web::{web, HttpRequest, HttpResponse, Responder};
use actix_ws::Message;
use futures_util::StreamExt;
use serde_json::{Map, Value};
use tokio::time::{sleep, Duration};
use crate::models::config::load_config;
use crate::twitch::{broadcaster_id, BoxError, TwitchConfig, TwitchState};

/// GET /api/twitch/badges — carte des badges de chat.
///
/// Les appels Helix sont faits **ici**, côté serveur : c'est ce qui permet aux
/// overlays de chat de ne plus embarquer `TWITCH_OAUTH_TOKEN` dans leur HTML.
/// La réponse ne contient que des URL d'images publiques.
pub async fn badges() -> impl Responder {
    match fetch_badges().await {
        Ok(payload) => HttpResponse::Ok().json(payload),
        Err(e) => {
            eprintln!("[Twitch] Récupération des badges impossible: {e}");
            // L'overlay doit continuer à afficher le chat sans badges.
            HttpResponse::Ok().json(serde_json::json!({"global": {}, "channel": {}}))
        }
    }
}

async fn fetch_badges() -> Result<Value, BoxError> {
    let twitch = TwitchConfig::from_app(&load_config());

    let client = reqwest::Client::new();
    let bid = broadcaster_id(&client, &twitch).await?;

    let global = badge_set(&client, &twitch, "https://api.twitch.tv/helix/chat/badges/global", None).await?;
    let channel = badge_set(
        &client,
        &twitch,
        "https://api.twitch.tv/helix/chat/badges",
        Some(("broadcaster_id", bid.as_str())),
    )
    .await?;

    Ok(serde_json::json!({ "global": global, "channel": channel }))
}

/// Aplatit la réponse Helix en `{"set_id/version": "url"}`, la forme attendue par
/// `chat-common.js::parseBadges`.
async fn badge_set(
    client: &reqwest::Client,
    config: &TwitchConfig,
    url: &str,
    query: Option<(&str, &str)>,
) -> Result<Map<String, Value>, BoxError> {
    let mut request = client
        .get(url)
        .header("Client-Id", &config.client_id)
        .header("Authorization", config.bearer());
    if let Some(q) = query {
        request = request.query(&[q]);
    }

    let response = request.send().await?;
    if !response.status().is_success() {
        return Err(format!("{url} a renvoyé HTTP {}", response.status()).into());
    }

    let body: Value = response.json().await?;
    let mut map = Map::new();
    if let Some(sets) = body["data"].as_array() {
        for set in sets {
            let Some(set_id) = set["set_id"].as_str() else { continue };
            let Some(versions) = set["versions"].as_array() else { continue };
            for version in versions {
                let (Some(id), Some(image)) =
                    (version["id"].as_str(), version["image_url_1x"].as_str())
                else {
                    continue;
                };
                map.insert(format!("{set_id}/{id}"), Value::String(image.to_string()));
            }
        }
    }
    Ok(map)
}

pub async fn ws_handler(
    req: HttpRequest,
    body: web::Payload,
    state: web::Data<Mutex<TwitchState>>,
) -> actix_web::Result<HttpResponse> {
    let (response, mut session, mut stream) = actix_ws::handle(&req, body)?;
    let state = state.into_inner();

    // `MessageStream` n'est pas `Send` : on spawne sur le runtime local d'actix.
    actix_web::rt::spawn(async move {
        let snapshot = |state: &Mutex<TwitchState>| {
            let g = state.lock().unwrap();
            serde_json::json!({
                "total_followers": g.total_followers,
                "last_follower": g.last_follower,
                "connected": g.connected,
            })
            .to_string()
        };

        // Envoi immédiat : la boucle n'émettait rien pendant la première demi-seconde.
        let mut last = snapshot(&state);
        if session.text(last.clone()).await.is_err() {
            return;
        }

        loop {
            tokio::select! {
                // Sans consommer le flux entrant, une déconnexion client passe
                // inaperçue et les pings restent sans réponse.
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
                _ = sleep(Duration::from_millis(500)) => {
                    let current = snapshot(&state);
                    if current != last {
                        if session.text(current.clone()).await.is_err() {
                            return;
                        }
                        last = current;
                    }
                }
            }
        }
    });

    Ok(response)
}
