use actix_web::{web, HttpResponse, Responder};
use askama::Template;
use serde::Deserialize;
use crate::models::config::{font_path, load_config};

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate;

#[derive(Template)]
#[template(path = "clock.html")]
struct ClockTemplate {
    title_font: String,
    format_hour: bool,
    format_minute: bool,
    format_second: bool,
}

#[derive(Template)]
#[template(path = "banner.html")]
struct BannerTemplate {
    title_font: String,
}

#[derive(Template)]
#[template(path = "music_current.html")]
struct MusicCurrentTemplate {
    music_port: u16,
    title_font: String,
}

#[derive(Template)]
#[template(path = "emote_corner.html")]
struct EmoteCornerTemplate {
    twitch_channel_name: String,
    twitch_oauth_token: String,
}

#[derive(Template)]
#[template(path = "discord_presence.html")]
struct DiscordPresenceTemplate {
    port_discord: u16,
}

#[derive(Template)]
#[template(path = "followers_info.html")]
struct FollowersInfoTemplate {
    music_port: u16,
}

#[derive(Template)]
#[template(path = "chat_horizontal.html")]
struct ChatHorizontalTemplate {
    twitch_channel_name: String,
    twitch_client_id: String,
    twitch_oauth_token: String,
    port_ws_youtube_chat: u16,
}

#[derive(Template)]
#[template(path = "chat_vertical.html")]
struct ChatVerticalTemplate {
    twitch_channel_name: String,
    twitch_client_id: String,
    twitch_oauth_token: String,
    port_ws_youtube_chat: u16,
}

#[derive(Template)]
#[template(path = "chat_youtube.html")]
struct ChatYoutubeTemplate;

#[derive(Deserialize)]
pub struct ClockQuery {
    pub hour: Option<bool>,
    pub minute: Option<bool>,
    pub second: Option<bool>,
}

fn render(html: String) -> HttpResponse {
    HttpResponse::Ok().content_type("text/html").body(html)
}

pub async fn index() -> impl Responder {
    render(IndexTemplate.render().unwrap())
}

pub async fn clock(query: web::Query<ClockQuery>) -> impl Responder {
    let config = load_config();
    render(ClockTemplate {
        title_font: font_path(&config),
        format_hour: query.hour.unwrap_or(true),
        format_minute: query.minute.unwrap_or(true),
        format_second: query.second.unwrap_or(true),
    }.render().unwrap())
}

pub async fn banner() -> impl Responder {
    let config = load_config();
    render(BannerTemplate { title_font: font_path(&config) }.render().unwrap())
}

pub async fn music_current() -> impl Responder {
    let config = load_config();
    render(MusicCurrentTemplate {
        music_port: config.port_music,
        title_font: font_path(&config),
    }.render().unwrap())
}

pub async fn emote_corner() -> impl Responder {
    let config = load_config();
    render(EmoteCornerTemplate {
        twitch_channel_name: config.twitch_channel_name,
        twitch_oauth_token: config.twitch_oauth_token,
    }.render().unwrap())
}

pub async fn discord_presence() -> impl Responder {
    let config = load_config();
    render(DiscordPresenceTemplate { port_discord: config.port_discord }.render().unwrap())
}

pub async fn followers_info() -> impl Responder {
    let config = load_config();
    render(FollowersInfoTemplate {
        music_port: config.port_music,
    }.render().unwrap())
}

pub async fn chat_horizontal() -> impl Responder {
    let config = load_config();
    render(ChatHorizontalTemplate {
        twitch_channel_name: config.twitch_channel_name,
        twitch_client_id: config.twitch_client_id,
        twitch_oauth_token: config.twitch_oauth_token,
        port_ws_youtube_chat: config.port_ws_youtube_chat,
    }.render().unwrap())
}

pub async fn chat_vertical() -> impl Responder {
    let config = load_config();
    render(ChatVerticalTemplate {
        twitch_channel_name: config.twitch_channel_name,
        twitch_client_id: config.twitch_client_id,
        twitch_oauth_token: config.twitch_oauth_token,
        port_ws_youtube_chat: config.port_ws_youtube_chat,
    }.render().unwrap())
}

pub async fn chat_youtube() -> impl Responder {
    render(ChatYoutubeTemplate.render().unwrap())
}
