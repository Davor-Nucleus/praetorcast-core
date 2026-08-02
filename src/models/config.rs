use serde::Deserialize;
use std::sync::{Arc, RwLock};

/// Vue en lecture d'`env.json`.
///
/// **Ne jamais dériver `Serialize` ici.** La structure ne connaît qu'une partie des
/// clés du fichier : la resérialiser puis l'écrire effacerait `VOLUME`,
/// `normalizationEnabled`, `janusCoreGui`, `phonosCoreGui` et les clés Discord /
/// YouTube, c'est-à-dire la configuration de JanusCore, PhonosCore et des deux
/// scripts node. Toute écriture passe par `models::env_file`, qui fusionne clé par
/// clé sur une relecture fraîche du fichier.
#[derive(Deserialize, Clone)]
pub struct AppConfig {
    #[serde(rename = "FRONT_FONT_TITLE")]
    pub front_font_title: String,
    #[serde(rename = "PORT")]
    pub port: u16,
    #[serde(rename = "PORT_MUSIC")]
    pub port_music: u16,
    #[serde(rename = "PORT_SOUNDBOARD")]
    pub port_soundboard: u16,
    #[serde(rename = "SOUNDBOARD_SHORTCUTS")]
    pub soundboard_shortcuts: std::collections::HashMap<String, String>,
    #[serde(rename = "TWITCH_CHANNEL_NAME")]
    pub twitch_channel_name: String,
    #[serde(rename = "TWITCH_CLIENT_ID")]
    pub twitch_client_id: String,
    #[serde(rename = "TWITCH_OAUTH_TOKEN")]
    pub twitch_oauth_token: String,
    #[serde(rename = "PORT_WS_YOUTUBE_CHAT")]
    pub port_ws_youtube_chat: u16,
    #[serde(rename = "PORT_WS_DISCORD_PRESENCE")]
    pub port_discord: u16,
    #[serde(rename = "OBS_WS_HOST", default = "default_obs_host")]
    pub obs_ws_host: String,
    #[serde(rename = "OBS_WS_PORT", default = "default_obs_port")]
    pub obs_ws_port: u16,
    #[serde(rename = "OBS_WS_PASSWORD", default)]
    pub obs_ws_password: String,
    #[serde(rename = "OBS_AUDIO_SOURCE", default = "default_obs_source")]
    pub obs_audio_source: String,
    #[serde(rename = "OBS_LIMITER_FILTER", default = "default_obs_filter")]
    pub obs_limiter_filter: String,

    /// Instant d'expiration du jeton, en secondes Unix. 0 = inconnu (jeton collé à
    /// la main), auquel cas la page de réglages n'affiche aucun compte à rebours.
    ///
    /// Écrite par le bouton « Connecter Twitch » ; `default` pour qu'un `env.json`
    /// antérieur reste chargeable.
    #[serde(rename = "TWITCH_TOKEN_EXPIRES_AT", default)]
    pub twitch_token_expires_at: i64,
}

fn default_obs_host() -> String {
    "localhost".to_string()
}

fn default_obs_port() -> u16 {
    4455
}

fn default_obs_source() -> String {
    "music".to_string()
}

fn default_obs_filter() -> String {
    "Limiter".to_string()
}

static CONFIG: RwLock<Option<Arc<AppConfig>>> = RwLock::new(None);

/// Lit et parse `env.json` sans toucher au cache.
fn read_from_disk() -> Result<AppConfig, String> {
    let content = std::fs::read_to_string(super::env_file::ENV_PATH).map_err(|e| {
        format!("Impossible de lire env.json ({e}) — le serveur doit être lancé depuis le dossier qui le contient")
    })?;
    serde_json::from_str(&content).map_err(|e| format!("env.json est invalide : {e}"))
}

/// Configuration courante.
///
/// Le fichier n'est lu qu'une fois puis gardé en mémoire : le relire à chaque
/// requête coûtait cher et faisait paniquer le serveur en pleine requête si le
/// fichier devenait corrompu. Le cache est en revanche **remplaçable** par
/// [`reload_config`], sans quoi un réglage enregistré depuis `/settings` — un jeton
/// OAuth en particulier — n'aurait aucun effet avant un redémarrage manuel.
///
/// `main` appelle cette fonction au démarrage : une configuration invalide échoue
/// donc immédiatement, avec un message explicite.
pub fn load_config() -> Arc<AppConfig> {
    if let Some(config) = CONFIG.read().unwrap().as_ref() {
        return config.clone();
    }

    let mut guard = CONFIG.write().unwrap();
    // Un autre thread a pu charger la configuration entre les deux verrous.
    if let Some(config) = guard.as_ref() {
        return config.clone();
    }
    let config = Arc::new(read_from_disk().unwrap_or_else(|e| panic!("{e}")));
    *guard = Some(config.clone());
    config
}

/// Relit `env.json` et remplace la configuration en mémoire.
///
/// Contrairement à [`load_config`], **ne panique jamais** : un fichier devenu
/// illisible pendant la session laisse l'ancienne configuration en place plutôt
/// que d'abattre le serveur. Les handlers appelés ensuite continuent de servir la
/// dernière configuration valide.
///
/// Deux réglages échappent malgré tout au rechargement à chaud : `PORT`, parce que
/// la socket est déjà liée, et les ports des autres services, qui ne sont lus qu'au
/// démarrage de JanusCore / PhonosCore / des scripts node.
pub fn reload_config() -> Result<Arc<AppConfig>, String> {
    let config = Arc::new(read_from_disk()?);
    *CONFIG.write().unwrap() = Some(config.clone());
    Ok(config)
}

pub fn font_path(config: &AppConfig) -> String {
    if config.front_font_title.starts_with('/') {
        config.front_font_title.clone()
    } else {
        format!("/public/font/{}", config.front_font_title)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_config_deserialization() {
        let json = r#"{
            "FRONT_FONT_TITLE": "myfont.ttf",
            "PORT": 8080,
            "PORT_MUSIC": 8081,
            "PORT_SOUNDBOARD": 8082,
            "SOUNDBOARD_SHORTCUTS": {},
            "TWITCH_CHANNEL_NAME": "mychannel",
            "TWITCH_CLIENT_ID": "client123",
            "TWITCH_OAUTH_TOKEN": "token123",
            "PORT_WS_YOUTUBE_CHAT": 8083,
            "PORT_WS_DISCORD_PRESENCE": 8084
        }"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.front_font_title, "myfont.ttf");
        assert_eq!(config.port, 8080);
        assert_eq!(config.port_music, 8081);
        assert_eq!(config.port_soundboard, 8082);
        assert_eq!(config.twitch_channel_name, "mychannel");
        assert_eq!(config.twitch_client_id, "client123");
        assert_eq!(config.twitch_oauth_token, "token123");
        assert_eq!(config.port_ws_youtube_chat, 8083);
        assert_eq!(config.port_discord, 8084);
    }

    #[test]
    fn test_app_config_defaults() {
        let json = r#"{
            "FRONT_FONT_TITLE": "myfont.ttf",
            "PORT": 8080,
            "PORT_MUSIC": 8081,
            "PORT_SOUNDBOARD": 8082,
            "SOUNDBOARD_SHORTCUTS": {},
            "TWITCH_CHANNEL_NAME": "mychannel",
            "TWITCH_CLIENT_ID": "client123",
            "TWITCH_OAUTH_TOKEN": "token123",
            "PORT_WS_YOUTUBE_CHAT": 8083,
            "PORT_WS_DISCORD_PRESENCE": 8084
        }"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.obs_ws_host, "localhost");
        assert_eq!(config.obs_ws_port, 4455);
        assert_eq!(config.obs_ws_password, "");
        assert_eq!(config.obs_audio_source, "music");
        assert_eq!(config.obs_limiter_filter, "Limiter");
    }

    #[test]
    fn test_font_path_with_leading_slash() {
        let json = r#"{
            "FRONT_FONT_TITLE": "/custom/fonts/myfont.ttf",
            "PORT": 8080,
            "PORT_MUSIC": 8081,
            "PORT_SOUNDBOARD": 8082,
            "SOUNDBOARD_SHORTCUTS": {},
            "TWITCH_CHANNEL_NAME": "mychannel",
            "TWITCH_CLIENT_ID": "client123",
            "TWITCH_OAUTH_TOKEN": "token123",
            "PORT_WS_YOUTUBE_CHAT": 8083,
            "PORT_WS_DISCORD_PRESENCE": 8084
        }"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert_eq!(font_path(&config), "/custom/fonts/myfont.ttf");
    }

    #[test]
    fn test_font_path_without_slash() {
        let json = r#"{
            "FRONT_FONT_TITLE": "myfont.ttf",
            "PORT": 8080,
            "PORT_MUSIC": 8081,
            "PORT_SOUNDBOARD": 8082,
            "SOUNDBOARD_SHORTCUTS": {},
            "TWITCH_CHANNEL_NAME": "mychannel",
            "TWITCH_CLIENT_ID": "client123",
            "TWITCH_OAUTH_TOKEN": "token123",
            "PORT_WS_YOUTUBE_CHAT": 8083,
            "PORT_WS_DISCORD_PRESENCE": 8084
        }"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert_eq!(font_path(&config), "/public/font/myfont.ttf");
    }

    #[test]
    fn l_echeance_du_jeton_est_optionnelle() {
        // Un env.json antérieur à l'ajout du bouton de connexion doit rester
        // chargeable, avec une échéance simplement inconnue.
        let json = r#"{
            "FRONT_FONT_TITLE": "myfont.ttf",
            "PORT": 8080,
            "PORT_MUSIC": 8081,
            "PORT_SOUNDBOARD": 8082,
            "SOUNDBOARD_SHORTCUTS": {},
            "TWITCH_CHANNEL_NAME": "mychannel",
            "TWITCH_CLIENT_ID": "client123",
            "TWITCH_OAUTH_TOKEN": "token123",
            "PORT_WS_YOUTUBE_CHAT": 8083,
            "PORT_WS_DISCORD_PRESENCE": 8084
        }"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.twitch_token_expires_at, 0);
    }

    #[test]
    fn test_default_functions() {
        assert_eq!(default_obs_host(), "localhost");
        assert_eq!(default_obs_port(), 4455);
        assert_eq!(default_obs_source(), "music");
        assert_eq!(default_obs_filter(), "Limiter");
    }
}
