use actix_files::Files;
use actix_web::{web, App, HttpServer};
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

mod models;
mod controllers;
mod twitch;
mod twitch_auth;

use controllers::{auth_controller, banner_controller, channel_point_controller, display, goal_controller, music_controller, obs_controller, scheduler_controller, settings_controller, theme_controller, timer_controller, twitch_controller};
use models::config::load_config;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let config = load_config();
    let port = config.port;

    let twitch_data = web::Data::new(Mutex::new(twitch::TwitchState::default()));
    let bg_state = twitch_data.clone().into_inner();

    // Réveille la session EventSub quand les identifiants Twitch changent, qu'ils
    // viennent de /settings ou du renouvellement automatique du jeton.
    let reload = Arc::new(Notify::new());
    let reload_data = web::Data::new(reload.clone());

    let twitch_reload = reload.clone();
    tokio::spawn(async move {
        twitch::run(bg_state, twitch_reload).await;
    });

    // Réveille les afficheurs du compte à rebours dès qu'un bouton est cliqué, au
    // lieu d'attendre le tour de boucle suivant.
    let timer_notify = web::Data::new(timer_controller::TimerNotify(Notify::new()));

    println!("Serveur en cours d'exécution sur http://127.0.0.1:{}", port);

    HttpServer::new(move || {
        App::new()
            .app_data(twitch_data.clone())
            .app_data(reload_data.clone())
            .app_data(timer_notify.clone())
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
            .route("/api/banner_ws", web::get().to(banner_controller::banner_ws))
            // API scheduler
            .route("/api/scheduler-config", web::get().to(scheduler_controller::get))
            .route("/api/scheduler-config", web::post().to(scheduler_controller::save))
            .route("/api/scheduler-upload", web::post().to(scheduler_controller::upload_image))
            .route("/api/scheduler-background-upload", web::post().to(scheduler_controller::upload_background))
            // Pages channel points
            .route("/channel-points", web::get().to(display::channel_point))
            .route("/channel-points-config", web::get().to(channel_point_controller::page))
            // API channel points
            .route("/api/channel-points-config", web::get().to(channel_point_controller::get))
            .route("/api/channel-points-config", web::post().to(channel_point_controller::save))
            .route("/api/channel-points-upload-image", web::post().to(channel_point_controller::upload_image))
            .route("/api/channel-points-upload-sound", web::post().to(channel_point_controller::upload_sound))
            .route("/api/channel_point_ws", web::get().to(channel_point_controller::redemption_ws))
            // Barre d'objectif
            .route("/goal", web::get().to(goal_controller::display))
            .route("/goal-config", web::get().to(goal_controller::page))
            .route("/api/goal-config", web::get().to(goal_controller::get))
            .route("/api/goal-config", web::post().to(goal_controller::save))
            .route("/api/goal_ws", web::get().to(goal_controller::goal_ws))
            // Compte à rebours
            .route("/timer", web::get().to(timer_controller::display))
            .route("/timer-config", web::get().to(timer_controller::page))
            .route("/api/timer", web::get().to(timer_controller::get))
            .route("/api/timer", web::post().to(timer_controller::save))
            .route("/api/timer_ws", web::get().to(timer_controller::timer_ws))
            // Actions en GET **et** en POST, comme le limiteur OBS : la page de
            // contrôle utilise POST, mais la même URL reste déclenchable depuis un
            // raccourci ou un bouton de Stream Deck qui ne sait faire qu'un GET.
            .route("/api/timer/start", web::get().to(timer_controller::start))
            .route("/api/timer/start", web::post().to(timer_controller::start))
            .route("/api/timer/pause", web::get().to(timer_controller::pause))
            .route("/api/timer/pause", web::post().to(timer_controller::pause))
            .route("/api/timer/toggle", web::get().to(timer_controller::toggle))
            .route("/api/timer/toggle", web::post().to(timer_controller::toggle))
            .route("/api/timer/reset", web::get().to(timer_controller::reset))
            .route("/api/timer/reset", web::post().to(timer_controller::reset))
            .route("/api/timer/adjust", web::get().to(timer_controller::adjust))
            .route("/api/timer/adjust", web::post().to(timer_controller::adjust))
            // Paramètres (env.json)
            .route("/settings", web::get().to(settings_controller::page))
            .route("/api/settings", web::get().to(settings_controller::get))
            .route("/api/settings", web::post().to(settings_controller::save))
            // Connexion Twitch (implicit grant : le jeton arrive dans le fragment,
            // d'où la page de retour qui le repost sur /auth/twitch/token)
            .route("/auth/twitch", web::get().to(auth_controller::start))
            .route("/auth/callback", web::get().to(auth_controller::callback))
            .route("/auth/twitch/token", web::post().to(auth_controller::submit_token))
            .route("/api/twitch/auth-status", web::get().to(auth_controller::status))
            // Thème global des overlays
            .route("/theme.css", web::get().to(theme_controller::css))
            .route("/api/theme", web::get().to(theme_controller::get))
            .route("/api/theme", web::post().to(theme_controller::save))
            .route("/api/theme_ws", web::get().to(theme_controller::theme_ws))
            // API Twitch
            .route("/api/twitch_ws", web::get().to(twitch_controller::ws_handler))
            .route("/api/twitch/badges", web::get().to(twitch_controller::badges))
            // API OBS (limiteur sur la source audio "music")
            .route("/api/obs/limiter_ws", web::get().to(obs_controller::limiter_ws))
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
