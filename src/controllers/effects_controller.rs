//! Effets visuels : pluie d'emotes, cadre caméra, visualiseur — et le flux
//! d'événements qui les anime.

use actix_web::{web, HttpRequest, HttpResponse, Responder};
use actix_ws::Message;
use askama::Template;
use futures_util::StreamExt;
use serde::Deserialize;
use std::sync::Mutex;
use tokio::sync::broadcast;
use tokio::time::{sleep, Duration};

use crate::models::effects::{self, EffectsConfig};
use crate::models::events::{self, EffectKind, FeedMessage, EVENTS_PATH};
use crate::twitch::TwitchState;

#[derive(Template)]
#[template(path = "effects_config.html")]
struct EffectsConfigTemplate;

pub async fn page() -> impl Responder {
    let html = EffectsConfigTemplate.render().unwrap();
    HttpResponse::Ok().content_type("text/html").body(html)
}

pub async fn get() -> impl Responder {
    match effects::read() {
        Ok(config) => HttpResponse::Ok().json(config),
        Err(e) => {
            eprintln!("{e}");
            HttpResponse::Ok().json(EffectsConfig::default())
        }
    }
}

/// Enregistre les réglages et renvoie la version bornée : la page affiche ainsi ce
/// qui sera réellement appliqué.
pub async fn save(config: web::Json<EffectsConfig>) -> impl Responder {
    let config = config.into_inner().sanitized();
    match effects::write(&config) {
        Ok(()) => HttpResponse::Ok().json(config),
        Err(e) => {
            eprintln!("{e}");
            HttpResponse::InternalServerError()
                .json(serde_json::json!({ "error": "Failed to save effects config" }))
        }
    }
}

#[derive(Deserialize)]
pub struct TestRequest {
    effect: EffectKind,
}

/// Joue un effet dans les sources ouvertes, quels que soient ses déclencheurs.
///
/// Renvoie le nombre de sources connectées au flux — bannières comprises, qui
/// l'ignorent : c'est un ordre de grandeur, et surtout `0` explique un test resté
/// sans effet.
pub async fn test(
    request: web::Json<TestRequest>,
    state: web::Data<Mutex<TwitchState>>,
) -> impl Responder {
    let feed = state.lock().unwrap().feed.clone();
    let sources = feed
        .send(FeedMessage::EffectTest { effect: request.effect })
        .unwrap_or(0);
    HttpResponse::Ok().json(serde_json::json!({ "success": true, "sources": sources }))
}

fn config_message() -> Option<String> {
    let config = effects::read().ok()?;
    Some(serde_json::json!({ "type": "config", "effects": config }).to_string())
}

/// GET /api/events_ws — derniers événements, puis chaque nouveau, et les réglages
/// d'effets dès qu'ils changent.
///
/// Un seul flux pour la bannière et les trois overlays d'effets : chacun prend ce
/// qui le concerne et ignore le reste.
pub async fn events_ws(
    req: HttpRequest,
    body: web::Payload,
    state: web::Data<Mutex<TwitchState>>,
) -> actix_web::Result<HttpResponse> {
    let (response, mut session, mut stream) = actix_ws::handle(&req, body)?;
    // Abonnement pris avant le spawn : le verrou ne traverse jamais un `.await`.
    let mut feed = state.lock().unwrap().feed.subscribe();

    // `MessageStream` n'est pas `Send` : on spawne sur le runtime local d'actix.
    actix_web::rt::spawn(async move {
        let snapshot = serde_json::json!({
            "type": "snapshot",
            "events": events::read(EVENTS_PATH),
        })
        .to_string();
        if session.text(snapshot).await.is_err() {
            return;
        }

        let mut last_config = config_message();
        if let Some(message) = &last_config {
            if session.text(message.clone()).await.is_err() {
                return;
            }
        }

        loop {
            tokio::select! {
                // Sans consommer le flux entrant, la fermeture d'une source OBS passe
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
                message = feed.recv() => {
                    let message = match message {
                        Ok(message) => message,
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            println!("[WS] Overlay d'effets à la traîne, {n} événement(s) ignoré(s)");
                            continue;
                        }
                        Err(broadcast::error::RecvError::Closed) => return,
                    };
                    let Ok(text) = serde_json::to_string(&message) else { continue };
                    if session.text(text).await.is_err() {
                        return;
                    }
                }
                // Un « Enregistrer » dans /effects-config s'applique sans rafraîchir
                // les sources OBS. Une lecture ratée garde les réglages en place.
                _ = sleep(Duration::from_millis(1000)) => {
                    let next = config_message();
                    if next.is_some() && next != last_config {
                        if session.text(next.clone().unwrap_or_default()).await.is_err() {
                            return;
                        }
                        last_config = next;
                    }
                }
            }
        }
    });

    Ok(response)
}
