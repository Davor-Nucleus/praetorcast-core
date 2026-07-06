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
    fn test_default_functions() {
        assert_eq!(default_obs_host(), "localhost");
        assert_eq!(default_obs_port(), 4455);
        assert_eq!(default_obs_source(), "music");
        assert_eq!(default_obs_filter(), "Limiter");
    }
}
