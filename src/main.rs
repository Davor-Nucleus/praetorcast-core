use actix_files::Files;
use actix_web::{web, App, HttpServer};
use std::sync::Mutex;

mod models;
mod controllers;
mod twitch;

use controllers::{banner_controller, display, music_controller, obs_controller, scheduler_controller, twitch_controller};
use models::config::load_config;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let config = load_config();
    let port = config.port;

    let twitch_cfg = twitch::TwitchConfig {
        channel_name: config.twitch_channel_name.clone(),
        client_id: config.twitch_client_id.clone(),
        token: config.twitch_oauth_token.clone(),
    };

    let twitch_data = web::Data::new(Mutex::new(twitch::TwitchState::default()));
    let bg_state = twitch_data.clone().into_inner();

    tokio::spawn(async move {
        twitch::run(bg_state, twitch_cfg).await;
    });

    println!("Serveur en cours d'exécution sur http://127.0.0.1:{}", port);

    HttpServer::new(move || {
        App::new()
            .app_data(twitch_data.clone())
            // Fichiers statiques
            .service(Files::new("/public", "./public"))
            // Pages d'affichage
            .route("/", web::get().to(display::index))
            .route("/clock", web::get().to(display::clock))
            .route("/banner", web::get().to(display::banner))
            .route("/music-current", web::get().to(display::music_current))
            .route("/emote-corner", web::get().to(display::emote_corner))
            .route("/discord-presence", web::get().to(display::discord_presence))
            .route("/followers-info", web::get().to(display::followers_info))
            .route("/chat-horizontal", web::get().to(display::chat_horizontal))
            .route("/chat-vertical", web::get().to(display::chat_vertical))
            .route("/chat-youtube", web::get().to(display::chat_youtube))
            // Pages de configuration
            .route("/music-config", web::get().to(music_controller::music_config))
            .route("/banner-config", web::get().to(banner_controller::page))
            .route("/scheduler", web::get().to(scheduler_controller::page))
            // API banner
            .route("/api/banner-config", web::get().to(banner_controller::get))
            .route("/api/banner-config", web::post().to(banner_controller::save))
            .route("/api/banner-upload", web::post().to(banner_controller::upload))
            // API scheduler
            .route("/api/scheduler-config", web::get().to(scheduler_controller::get))
            .route("/api/scheduler-config", web::post().to(scheduler_controller::save))
            .route("/api/scheduler-upload", web::post().to(scheduler_controller::upload_image))
            .route("/api/scheduler-background-upload", web::post().to(scheduler_controller::upload_background))
            // API Twitch
            .route("/api/twitch_ws", web::get().to(twitch_controller::ws_handler))
            // API OBS (limiteur sur la source audio "music")
            .route("/api/obs/limiter", web::get().to(obs_controller::get_limiter))
            .route("/api/obs/limiter/add", web::get().to(obs_controller::add_limiter))
            .route("/api/obs/limiter/add", web::post().to(obs_controller::add_limiter))
            .route("/api/obs/limiter/subtract", web::get().to(obs_controller::subtract_limiter))
            .route("/api/obs/limiter/subtract", web::post().to(obs_controller::subtract_limiter))
            .route("/api/obs/limiter/toggle", web::get().to(obs_controller::toggle_limiter))
    })
    .bind(("127.0.0.1", port))?
    .run()
    .await
}
