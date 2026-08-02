//! Page `/settings` : édition d'`env.json` depuis le navigateur.
//!
//! Découpage habituel — une page, un GET, un POST — mais avec deux particularités
//! imposées par le fichier édité (cf. `models::env_file` et `models::settings`) :
//! les secrets ne descendent jamais dans la page, et l'enregistrement ne réécrit
//! que les clés effectivement modifiées.

use std::sync::Arc;

use actix_web::{web, HttpResponse, Responder};
use askama::Template;
use serde_json::{Map, Value};
use tokio::sync::Notify;

use crate::models::config::reload_config;
use crate::models::{env_file, settings};
use crate::twitch_auth;

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsTemplate {
    redirect_uri: String,
}

pub async fn page() -> impl Responder {
    // Affichée telle quelle pour être recopiée dans la console développeur Twitch,
    // qui exige une correspondance exacte — d'où la construction depuis `PORT`.
    let port = crate::models::config::load_config().port;
    HttpResponse::Ok().content_type("text/html").body(
        SettingsTemplate { redirect_uri: twitch_auth::redirect_uri(port) }
            .render()
            .unwrap(),
    )
}

/// GET /api/settings — valeurs courantes, secrets masqués.
pub async fn get() -> impl Responder {
    match env_file::read_raw() {
        Ok(raw) => HttpResponse::Ok().json(settings::view(&raw)),
        Err(e) => {
            eprintln!("{e}");
            // Un env.json illisible ne doit pas rendre la page inutilisable : elle
            // s'affiche vide, et l'enregistrement refusera d'écraser le fichier.
            HttpResponse::Ok().json(settings::view(&Map::new()))
        }
    }
}

/// POST /api/settings — valide puis fusionne les champs modifiés.
pub async fn save(
    submitted: web::Json<Map<String, Value>>,
    reload: web::Data<Arc<Notify>>,
) -> impl Responder {
    let current = match env_file::read_raw() {
        Ok(raw) => raw,
        Err(e) => {
            eprintln!("{e}");
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "env.json est illisible — enregistrement annulé pour ne pas écraser la configuration"
            }));
        }
    };

    let changes = match settings::sanitize(&submitted, &current) {
        Ok(changes) => changes,
        // Erreur de saisie : le message est écrit pour être affiché tel quel.
        Err(e) => return HttpResponse::BadRequest().json(serde_json::json!({ "error": e })),
    };

    if changes.is_empty() {
        return HttpResponse::Ok().json(serde_json::json!({
            "success": true,
            "changed": 0,
            "restart": Vec::<String>::new(),
        }));
    }

    let restart = settings::impacted_services(&changes);
    let changed = changes.len();

    if let Err(e) = env_file::merge_keys(&changes) {
        eprintln!("{e}");
        return HttpResponse::InternalServerError()
            .json(serde_json::json!({ "error": "Failed to save settings" }));
    }

    // Sans ce rechargement, les handlers continueraient de servir l'ancienne
    // configuration jusqu'au prochain démarrage.
    if let Err(e) = reload_config() {
        eprintln!("{e}");
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Réglages écrits, mais le rechargement a échoué — redémarre le serveur"
        }));
    }

    // Le nom de chaîne ou le jeton ont pu changer : on force la session EventSub à
    // repartir sur la nouvelle configuration.
    reload.notify_waiters();

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "changed": changed,
        "restart": restart,
    }))
}
