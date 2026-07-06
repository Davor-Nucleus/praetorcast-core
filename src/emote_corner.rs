use actix_web::{HttpResponse, Responder};
use askama::Template;
use crate::load_config;

#[derive(Template)]
#[template(path = "emote_corner.html")]
struct EmoteCornerTemplate {
    twitch_channel_name: String,
    twitch_oauth_token: String,
}

pub async fn emote_corner() -> impl Responder {
    let config = load_config();
    let tmpl = EmoteCornerTemplate {
        twitch_channel_name: config.twitch_channel_name.clone(),
        twitch_oauth_token: config.twitch_oauth_token.clone(),
    };
    
    let rendered = tmpl.render().unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}

