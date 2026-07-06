use actix_web::{HttpResponse, Responder};
use askama::Template;
use crate::load_config;

#[derive(Template)]
#[template(path = "music_config.html")]
struct MusicConfigTemplate {
    music_port: u16,
    soundboard_port: u16,
    shortcuts_json: String,
}

pub async fn music_config() -> impl Responder {
    let config = load_config();
    // Convertir les raccourcis en JSON pour le JavaScript
    let shortcuts_json = serde_json::to_string(&config.soundboard_shortcuts)
        .unwrap_or_else(|_| "{}".to_string());

    let tmpl = MusicConfigTemplate {
        music_port: config.port_music,
        soundboard_port: config.port_soundboard,
        shortcuts_json,
    };
    
    let rendered = tmpl.render().unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}

