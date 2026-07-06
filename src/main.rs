use actix_files::Files;
use actix_web::{web, App, HttpServer};

mod models;
mod controllers;

use controllers::{banner_controller, display, music_controller, scheduler_controller};
use models::config::load_config;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let config = load_config();
    let port = config.port;

    println!("Serveur en cours d'exécution sur http://127.0.0.1:{}", port);

    HttpServer::new(|| {
        App::new()
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
    })
    .bind(("127.0.0.1", port))?
    .run()
    .await
}
