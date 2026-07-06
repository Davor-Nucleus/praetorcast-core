use crate::AppConfig;
use actix_web::{web, HttpResponse, Responder};
use askama::Template;

#[derive(Template)]
#[template(path = "followers_info.html")]
struct FollowersInfoTemplate {
    music_port: u16,
    twitch_channel_name: String,
    twitch_client_id: String,
    twitch_oauth_token: String,
}

pub async fn followers_info(config: web::Data<AppConfig>) -> impl Responder {
    let tmpl = FollowersInfoTemplate {
        music_port: config.port_music,
        twitch_channel_name: config.twitch_channel_name.clone(),
        twitch_client_id: config.twitch_client_id.clone(),
        twitch_oauth_token: config.twitch_oauth_token.clone(),
    };
    
    let rendered = tmpl.render().unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}

