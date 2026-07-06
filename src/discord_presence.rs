use actix_web::{HttpResponse, Responder};
use askama::Template;
use crate::load_config;

#[derive(Template)]
#[template(path = "discord_presence.html")]
struct DiscordPresenceTemplate {
    port_discord: u16,
}

pub async fn discord_presence() -> impl Responder {
    let config = load_config();
    let tmpl = DiscordPresenceTemplate {
        port_discord: config.port_discord,
    };
    let rendered = tmpl.render().unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}
