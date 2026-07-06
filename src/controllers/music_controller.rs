use actix_web::{HttpResponse, Responder};
use askama::Template;
use crate::models::config::load_config;

#[derive(Template)]
#[template(path = "music_config.html")]
struct MusicConfigTemplate {
    music_port: u16,
    soundboard_port: u16,
    shortcuts_json: String,
}

pub async fn music_config() -> impl Responder {
    let config = load_config();
    let shortcuts_json = serde_json::to_string(&config.soundboard_shortcuts)
        .unwrap_or_else(|_| "{}".to_string());
    let html = MusicConfigTemplate {
        music_port: config.port_music,
        soundboard_port: config.port_soundboard,
        shortcuts_json,
    }.render().unwrap();
    HttpResponse::Ok().content_type("text/html").body(html)
}
