use serde::Deserialize;

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
}

pub fn load_config() -> AppConfig {
    let content = std::fs::read_to_string("env.json")
        .expect("Impossible de lire env.json");
    serde_json::from_str(&content)
        .expect("Impossible de parser env.json")
}

pub fn font_path(config: &AppConfig) -> String {
    if config.front_font_title.starts_with('/') {
        config.front_font_title.clone()
    } else {
        format!("/public/font/{}", config.front_font_title)
    }
}
