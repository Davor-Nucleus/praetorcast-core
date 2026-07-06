use crate::AppConfig;
use actix_web::{web, HttpResponse, Responder};
use askama::Template;

#[derive(Template)]
#[template(path = "chat_horizontal.html")]
struct ChatHorizontalTemplate {
    twitch_channel_name: String,
    twitch_client_id: String,
    twitch_oauth_token: String,
    port_ws_youtube_chat: u16,
}

pub async fn chat_horizontal(config: web::Data<AppConfig>) -> impl Responder {
    let tmpl = ChatHorizontalTemplate {
        twitch_channel_name: config.twitch_channel_name.clone(),
        twitch_client_id: config.twitch_client_id.clone(),
        twitch_oauth_token: config.twitch_oauth_token.clone(),
        port_ws_youtube_chat: config.port_ws_youtube_chat,
    };
    
    let rendered = tmpl.render().unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}

