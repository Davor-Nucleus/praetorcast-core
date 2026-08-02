//! Thème global : feuille de style générée, API de réglage, et flux temps réel.
//!
//! Le WebSocket existe pour une raison précise : une Browser Source OBS ne se
//! recharge pas toute seule. Sans lui, chaque essai de couleur imposerait d'aller
//! cliquer « Actualiser » sur chaque source dans OBS. Avec lui, `theme-live.js`
//! réinjecte la feuille et l'overlay change d'apparence à l'écran, en direct.

use actix_web::{web, HttpRequest, HttpResponse, Responder};
use actix_ws::Message;
use futures_util::StreamExt;
use tokio::time::{sleep, Duration};

use crate::models::config::{font_path, load_config};
use crate::models::theme::{self, Theme};

/// Cadence du flux, alignée sur `banner_ws` et `goal_ws`.
const TICK: Duration = Duration::from_millis(1000);

/// CSS courant, police comprise.
fn current_css() -> String {
    let config = load_config();
    let theme = theme::read().unwrap_or_else(|e| {
        eprintln!("{e}");
        Theme::default()
    });
    theme.to_css(&font_path(&config))
}

/// GET /theme.css — feuille de style commune à tous les overlays.
///
/// `no-store` est indispensable : sans ça le navigateur d'OBS resservirait la
/// version en cache et le changement de thème n'apparaîtrait jamais.
pub async fn css() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/css; charset=utf-8")
        .append_header(("Cache-Control", "no-store"))
        .body(current_css())
}

/// GET /api/theme — réglages bruts, pour le formulaire.
pub async fn get() -> impl Responder {
    match theme::read() {
        Ok(theme) => HttpResponse::Ok().json(theme),
        Err(e) => {
            eprintln!("{e}");
            // Comme pour les objectifs : un fichier illisible renvoie le thème par
            // défaut plutôt qu'une erreur qui bloquerait la page.
            HttpResponse::Ok().json(Theme::default())
        }
    }
}

/// POST /api/theme
pub async fn save(theme: web::Json<Theme>) -> impl Responder {
    match theme::write(&theme.into_inner()) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"success": true})),
        Err(e) => {
            eprintln!("{e}");
            HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": "Failed to save theme config"}))
        }
    }
}

/// GET /api/theme_ws — signale aux overlays qu'il faut recharger `/theme.css`.
///
/// On compare le CSS produit plutôt que la date du fichier : un enregistrement qui
/// ne change rien (réouvrir la page et resauvegarder) ne doit pas faire clignoter
/// les overlays.
pub async fn theme_ws(req: HttpRequest, body: web::Payload) -> actix_web::Result<HttpResponse> {
    let (response, mut session, mut stream) = actix_ws::handle(&req, body)?;

    // `MessageStream` n'est pas `Send` : on spawne sur le runtime local d'actix.
    actix_web::rt::spawn(async move {
        let mut last = current_css();

        loop {
            tokio::select! {
                // Sans consommer le flux entrant, la fermeture d'une source OBS
                // passe inaperçue et les pings restent sans réponse.
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
                _ = sleep(TICK) => {
                    let current = current_css();
                    if current != last {
                        // Le contenu n'est pas transmis : l'overlay recharge la
                        // feuille, ce qui laisse le navigateur gérer le remplacement.
                        if session.text("reload").await.is_err() {
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
