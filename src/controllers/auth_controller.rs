//! Connexion du compte Twitch depuis `/settings`.
//!
//! Quatre routes : la redirection vers le consentement, la page de retour qui
//! récupère le jeton dans le fragment d'URL, l'enregistrement de ce jeton, et
//! l'état affiché par la page. Le détail du flux est dans [`crate::twitch_auth`].

use std::sync::{Arc, Mutex};

use actix_web::{web, HttpResponse, Responder};
use serde::Deserialize;
use tokio::sync::Notify;

use crate::models::config::load_config;
use crate::twitch_auth;

/// `state` du dernier `/auth/twitch` émis.
///
/// Protège du CSRF : un retour dont le `state` ne correspond pas au dernier que
/// **nous** avons émis vient d'ailleurs et est rejeté. Consommé à la première
/// utilisation pour qu'un jeton ne puisse pas être rejoué.
static PENDING_STATE: Mutex<Option<String>> = Mutex::new(None);

/// GET /auth/twitch — redirige vers l'écran de consentement Twitch.
pub async fn start() -> impl Responder {
    let config = load_config();

    if config.twitch_client_id.is_empty() {
        return feedback(
            false,
            "Renseigne d'abord le Client ID de ton application Twitch, puis enregistre.",
        );
    }

    let state = uuid::Uuid::new_v4().to_string();
    *PENDING_STATE.lock().unwrap() = Some(state.clone());

    HttpResponse::Found()
        .append_header((
            "Location",
            twitch_auth::authorize_url(
                &config.twitch_client_id,
                &twitch_auth::redirect_uri(config.port),
                &state,
            ),
        ))
        .finish()
}

/// GET /auth/callback — page de retour de Twitch.
///
/// En *implicit grant*, le jeton arrive dans le fragment (`#access_token=…`), que
/// le navigateur ne transmet jamais au serveur : ce handler ne peut donc rien lire
/// côté Rust. Il sert une page qui lit `location.hash` en JavaScript et repost le
/// jeton sur `/auth/twitch/token`.
pub async fn callback() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(CALLBACK_PAGE)
}

#[derive(Deserialize)]
pub struct SubmittedToken {
    #[serde(rename = "accessToken")]
    pub access_token: String,
    pub state: String,
}

/// POST /auth/twitch/token — enregistre le jeton lu dans le fragment.
pub async fn submit_token(
    body: web::Json<SubmittedToken>,
    reload: web::Data<Arc<Notify>>,
) -> impl Responder {
    // Le state est consommé quoi qu'il arrive : un jeton déjà présenté ne doit pas
    // pouvoir l'être une seconde fois.
    let expected = PENDING_STATE.lock().unwrap().take();
    if expected.is_none() || expected.as_deref() != Some(body.state.as_str()) {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Retour inattendu (state invalide) — relance la connexion depuis /settings."
        }));
    }

    if body.access_token.is_empty() {
        return HttpResponse::BadRequest()
            .json(serde_json::json!({ "error": "Twitch n'a renvoyé aucun jeton." }));
    }

    // On valide avant d'écrire : ça confirme que le jeton fonctionne, et surtout ça
    // donne les scopes réellement accordés et l'échéance, que le fragment n'annonce
    // pas de façon fiable.
    let info = match twitch_auth::validate(&body.access_token).await {
        Ok(info) => info,
        Err(e) => {
            eprintln!("[Twitch] Jeton refusé à la validation : {e}");
            return HttpResponse::BadRequest().json(serde_json::json!({ "error": e }));
        }
    };

    if let Err(e) = twitch_auth::store_token(&body.access_token, info.expires_in) {
        eprintln!("[Twitch] Enregistrement du jeton impossible : {e}");
        return HttpResponse::InternalServerError().json(serde_json::json!({ "error": e }));
    }

    // La boucle EventSub tourne avec l'ancien jeton : on la coupe pour qu'elle
    // reparte immédiatement avec le nouveau.
    reload.notify_waiters();
    println!("[Twitch] Compte connecté : {}", info.login);

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "login": info.login,
        "scopesMissing": twitch_auth::missing_scopes(&info.scopes),
    }))
}

/// GET /api/twitch/auth-status — état du jeton pour la page de réglages.
///
/// Ne renvoie jamais le jeton lui-même, seulement ce qu'on peut en dire.
pub async fn status() -> impl Responder {
    let config = load_config();

    let mut payload = serde_json::json!({
        "redirectUri": twitch_auth::redirect_uri(config.port),
        "scopesRequired": twitch_auth::SCOPES,
        "hasClientId": !config.twitch_client_id.is_empty(),
        "hasToken": !config.twitch_oauth_token.is_empty(),
        "expiresAt": config.twitch_token_expires_at,
    });

    if config.twitch_oauth_token.is_empty() {
        payload["valid"] = serde_json::Value::Bool(false);
        return HttpResponse::Ok().json(payload);
    }

    match twitch_auth::validate(&config.twitch_oauth_token).await {
        Ok(info) => {
            payload["valid"] = serde_json::Value::Bool(true);
            payload["login"] = serde_json::Value::String(info.login);
            payload["expiresIn"] = serde_json::Value::from(info.expires_in);
            payload["scopesMissing"] =
                serde_json::json!(twitch_auth::missing_scopes(&info.scopes));
            payload["scopes"] = serde_json::json!(info.scopes);
        }
        Err(e) => {
            payload["valid"] = serde_json::Value::Bool(false);
            payload["error"] = serde_json::Value::String(e);
        }
    }

    HttpResponse::Ok().json(payload)
}

/// Page affichée quand la connexion ne peut même pas démarrer.
fn feedback(success: bool, message: &str) -> HttpResponse {
    let color = if success { "#22c55e" } else { "#ef4444" };
    let title = if success { "Connexion réussie" } else { "Connexion échouée" };
    let message = message
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");

    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(format!(
            "<!DOCTYPE html><html lang=\"fr\"><head><meta charset=\"utf-8\"><title>{title}</title>\
             <style>{STYLE}h1{{color:{color}}}</style></head><body>\
             <h1>{title}</h1><p>{message}</p>\
             <p>Retour aux <a href=\"/settings\">paramètres</a>…</p>\
             <script>setTimeout(function(){{location.href=\"/settings\";}}, 5000);</script>\
             </body></html>"
        ))
}

const STYLE: &str = "body{margin:0;min-height:100vh;display:flex;flex-direction:column;\
align-items:center;justify-content:center;gap:1rem;background:#121212;color:#fff;\
font-family:sans-serif;text-align:center;padding:2rem}p{color:#a0a0a0;max-width:32rem}\
a{color:#60a5fa}";

/// Page de retour : lit le fragment, l'envoie au serveur, puis efface le jeton de
/// la barre d'adresse et de l'historique avant de revenir sur `/settings`.
const CALLBACK_PAGE: &str = r#"<!DOCTYPE html>
<html lang="fr">
<head>
    <meta charset="utf-8">
    <title>Connexion Twitch</title>
    <style>
        body {
            margin: 0;
            min-height: 100vh;
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            gap: 1rem;
            background: #121212;
            color: #fff;
            font-family: sans-serif;
            text-align: center;
            padding: 2rem;
        }
        h1 { margin: 0; }
        h1.ok { color: #22c55e; }
        h1.ko { color: #ef4444; }
        p { color: #a0a0a0; max-width: 32rem; }
        a { color: #60a5fa; }
    </style>
</head>
<body>
    <h1 id="title">Connexion en cours…</h1>
    <p id="message">Récupération du jeton renvoyé par Twitch.</p>
    <p><a href="/settings">Retour aux paramètres</a></p>

    <script>
        function show(ok, message) {
            const title = document.getElementById("title");
            title.textContent = ok ? "Connexion réussie" : "Connexion échouée";
            title.className = ok ? "ok" : "ko";
            document.getElementById("message").textContent = message;
            setTimeout(function () { location.href = "/settings"; }, ok ? 1500 : 6000);
        }

        // Twitch renvoie le jeton dans le fragment, jamais dans la query : c'est
        // pour ça que cette page existe, le serveur ne peut pas le lire lui-même.
        const params = new URLSearchParams(location.hash.slice(1));
        const accessToken = params.get("access_token") || "";
        const state = params.get("state") || "";
        const error = params.get("error");

        // Le jeton ne doit pas rester dans la barre d'adresse ni dans l'historique.
        history.replaceState(null, "", location.pathname);

        if (error) {
            show(false, params.get("error_description") || error);
        } else if (!accessToken) {
            show(false, "Aucun jeton dans l'URL de retour. Vérifie l'URL de redirection déclarée dans la console Twitch.");
        } else {
            fetch("/auth/twitch/token", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ accessToken: accessToken, state: state }),
            })
                .then(function (response) {
                    return response.json().then(function (data) {
                        return { ok: response.ok, data: data };
                    });
                })
                .then(function (result) {
                    if (!result.ok) {
                        show(false, result.data.error || "Enregistrement refusé.");
                        return;
                    }
                    const missing = result.data.scopesMissing || [];
                    show(
                        true,
                        "Compte connecté : " + (result.data.login || "?") +
                            (missing.length ? " — droits manquants : " + missing.join(", ") : "")
                    );
                })
                .catch(function (e) {
                    show(false, "Enregistrement impossible : " + e.message);
                });
        }
    </script>
</body>
</html>"#;
