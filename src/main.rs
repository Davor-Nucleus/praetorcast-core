
use actix_files::Files;
use actix_web::{web, App, HttpServer, HttpResponse, Responder};
use askama::Template;
use serde::Deserialize;
use std::sync::Arc;

mod clock;
mod music_config;
mod music_current;
mod banner_config;
mod banner;
mod scheduler;
mod followers_info;
mod chat_horizontal;
mod chat_vertical;
mod chat_youtube;

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate;

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
}

async fn index() -> impl Responder {
    let tmpl = IndexTemplate;
    let rendered = tmpl.render().unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Charger la configuration depuis env.json
    let env_content = std::fs::read_to_string("env.json")
        .expect("Impossible de lire env.json");
    let config: AppConfig = serde_json::from_str(&env_content)
        .expect("Impossible de parser env.json");
    let config = Arc::new(config);

    let config_clone = config.clone();
    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::from(config_clone.clone()))
            .service(Files::new("/public", "./public"))
            .route("/", web::get().to(index))
            .route("/clock", web::get().to(clock::clock))
            .route("/music-config", web::get().to(music_config::music_config))
            .route("/music-current", web::get().to(music_current::music_current))
            .route("/banner-config", web::get().to(banner_config::banner_config))
            .route("/banner", web::get().to(banner::banner))
            .route("/scheduler", web::get().to(scheduler::scheduler))
            .route("/followers-info", web::get().to(followers_info::followers_info))
            .route("/chat-horizontal", web::get().to(chat_horizontal::chat_horizontal))
            .route("/chat-vertical", web::get().to(chat_vertical::chat_vertical))
            .route("/chat-youtube", web::get().to(chat_youtube::chat_youtube))
            .route("/api/banner-config", web::get().to(banner_config::get_banner_config))
            .route("/api/banner-config", web::post().to(banner_config::save_banner_config))
            .route("/api/banner-upload", web::post().to(banner_config::upload_banner_image))
            .route("/api/scheduler-config", web::get().to(scheduler::get_scheduler_config))
            .route("/api/scheduler-config", web::post().to(scheduler::save_scheduler_config))
            .route("/api/scheduler-upload", web::post().to(scheduler::upload_scheduler_image))
            .route("/api/scheduler-background-upload", web::post().to(scheduler::upload_background_image))
    })
    .bind(("127.0.0.1", config.port))?;

    println!("Serveur en cours d'exécution sur http://127.0.0.1:{}", config.port);
    
    server.run().await
}
