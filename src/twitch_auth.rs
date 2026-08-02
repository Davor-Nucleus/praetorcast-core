//! Obtention du jeton Twitch (*implicit grant*) depuis la page de réglages.
//!
//! C'est le flux de l'URL qu'on construisait à la main jusqu'ici
//! (`response_type=token`) : il ne demande **que le Client ID**, aucun Client
//! Secret. Le bouton « Connecter Twitch » de `/settings` ouvre l'écran de
//! consentement, et PraetorCast récupère le jeton tout seul.
//!
//! Particularité du flux : Twitch renvoie le jeton dans le **fragment** de l'URL
//! (`#access_token=…`), que le navigateur n'envoie jamais au serveur. La page de
//! retour est donc un bout de HTML qui lit `location.hash` en JavaScript et
//! repost le jeton sur [`crate::controllers::auth_controller::submit_token`].
//! C'est aussi pour ça que l'URL de redirection doit pointer sur PraetorCast et
//! non sur `http://localhost` : sans page servie à l'arrivée, il n'y a personne
//! pour lire le fragment, et il faut recopier le jeton à la main.
//!
//! Contrepartie assumée : l'*implicit grant* ne délivre pas de *refresh token*.
//! Le jeton Twitch vit une soixantaine de jours ; `/settings` affiche l'échéance
//! et il suffit de recliquer sur le bouton pour en refaire un.

use serde::Deserialize;
use serde_json::{json, Map};

use crate::models::config::reload_config;
use crate::models::env_file;

pub const AUTHORIZE_URL: &str = "https://id.twitch.tv/oauth2/authorize";
pub const VALIDATE_URL: &str = "https://id.twitch.tv/oauth2/validate";

/// Droits demandés, repris de ce que le projet exploite réellement :
/// followers et abonnés pour les barres d'objectif, redemptions pour les points de
/// chaîne, `chat:read` pour le chat, `user:read:email` pour identifier le compte.
pub const SCOPES: &[&str] = &[
    "user:read:email",
    "user:read:follows",
    "moderator:read:followers",
    "chat:read",
    "channel:read:redemptions",
    "channel:read:subscriptions",
];

/// URL de redirection à déclarer dans la console développeur Twitch.
///
/// Twitch exige une correspondance exacte et n'accepte `http://` que sur
/// `localhost` — d'où cette forme. Elle suit `PORT` : changer le port du serveur
/// impose de mettre à jour la console Twitch, et la page de réglages affiche la
/// valeur à coller pour éviter toute erreur de recopie.
pub fn redirect_uri(port: u16) -> String {
    format!("http://localhost:{port}/auth/callback")
}

/// Réponse de `/oauth2/validate`.
///
/// C'est elle qui fait autorité sur ce que vaut le jeton : le fragment renvoyé par
/// Twitch annonce les scopes demandés, pas ceux réellement accordés.
#[derive(Deserialize)]
pub struct TokenInfo {
    #[serde(default)]
    pub login: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub expires_in: i64,
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// URL de consentement — l'équivalent exact de l'URL qu'on collait à la main.
pub fn authorize_url(client_id: &str, redirect_uri: &str, state: &str) -> String {
    format!(
        "{AUTHORIZE_URL}?client_id={client_id}&redirect_uri={redirect}&response_type=token&scope={scopes}&state={state}&force_verify=true",
        client_id = urlencode(client_id),
        redirect = urlencode(redirect_uri),
        scopes = urlencode(&SCOPES.join(" ")),
        state = urlencode(state),
    )
}

/// Encodage `application/x-www-form-urlencoded` minimal, pour l'URL d'autorisation
/// qu'on construit à la main avant de rediriger dessus.
pub fn urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Enregistre le jeton et son échéance, puis recharge la configuration.
///
/// Les deux clés partent d'un bloc : une écriture séparée laisserait une fenêtre
/// où le jeton est neuf mais l'échéance encore ancienne, donc un avertissement
/// « expire bientôt » sur un jeton qui vient d'être créé.
pub fn store_token(access_token: &str, expires_in: i64) -> Result<(), String> {
    let mut changes = Map::new();
    changes.insert("TWITCH_OAUTH_TOKEN".into(), json!(access_token));
    changes.insert("TWITCH_TOKEN_EXPIRES_AT".into(), json!(expires_at(expires_in)));

    env_file::merge_keys(&changes)?;
    reload_config().map(|_| ())
}

/// `expires_in` absent ou nul → 0, c'est-à-dire « échéance inconnue » : la page de
/// réglages n'affiche alors aucun compte à rebours plutôt qu'une date fausse.
fn expires_at(expires_in: i64) -> i64 {
    if expires_in <= 0 {
        0
    } else {
        now_unix() + expires_in
    }
}

/// Interroge `/oauth2/validate` : seule façon de connaître les scopes réellement
/// attachés au jeton, et donc d'expliquer pourquoi une barre « abonnés » reste vide.
pub async fn validate(token: &str) -> Result<TokenInfo, String> {
    let bare = token.strip_prefix("oauth:").unwrap_or(token);
    let response = reqwest::Client::new()
        .get(VALIDATE_URL)
        .header("Authorization", format!("OAuth {bare}"))
        .send()
        .await
        .map_err(|e| format!("Contact avec id.twitch.tv impossible : {e}"))?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err("Jeton invalide ou expiré".to_string());
    }
    if !response.status().is_success() {
        return Err(format!("id.twitch.tv a répondu HTTP {}", response.status()));
    }

    response
        .json()
        .await
        .map_err(|e| format!("Réponse de validation illisible : {e}"))
}

/// Scopes attendus mais absents du jeton.
pub fn missing_scopes(granted: &[String]) -> Vec<&'static str> {
    SCOPES
        .iter()
        .copied()
        .filter(|needed| !granted.iter().any(|g| g == needed))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_url_d_autorisation_reprend_le_flux_implicite() {
        let url = authorize_url("abc", &redirect_uri(3000), "xyz");
        assert!(url.starts_with(AUTHORIZE_URL));
        assert!(url.contains("client_id=abc"));
        // C'est ce paramètre qui distingue l'implicit grant du code grant : il
        // évite d'avoir à stocker un Client Secret.
        assert!(url.contains("response_type=token"));
        assert!(url.contains("state=xyz"));
        assert!(url.contains("force_verify=true"));
        // Espaces entre scopes et `:` encodés, sinon Twitch renvoie une erreur.
        assert!(url.contains("chat%3Aread"));
        assert!(url.contains("%20"));
        assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A3000%2Fauth%2Fcallback"));
    }

    #[test]
    fn l_url_de_redirection_suit_le_port_du_serveur() {
        assert_eq!(redirect_uri(3000), "http://localhost:3000/auth/callback");
        assert_eq!(redirect_uri(8080), "http://localhost:8080/auth/callback");
    }

    #[test]
    fn l_encodage_laisse_passer_les_caracteres_non_reserves() {
        assert_eq!(urlencode("aZ0-_.~"), "aZ0-_.~");
        assert_eq!(urlencode("a b"), "a%20b");
    }

    #[test]
    fn une_echeance_inconnue_reste_a_zero() {
        assert_eq!(expires_at(0), 0);
        assert_eq!(expires_at(-1), 0);
        assert!(expires_at(3600) >= now_unix() + 3600);
    }

    #[test]
    fn les_scopes_manquants_sont_listes() {
        let granted = vec!["chat:read".to_string(), "user:read:email".to_string()];
        let missing = missing_scopes(&granted);
        assert!(missing.contains(&"channel:read:subscriptions"));
        assert!(!missing.contains(&"chat:read"));
    }

    #[test]
    fn tous_les_scopes_accordes_ne_laissent_rien_manquant() {
        let granted: Vec<String> = SCOPES.iter().map(|s| s.to_string()).collect();
        assert!(missing_scopes(&granted).is_empty());
    }
}
