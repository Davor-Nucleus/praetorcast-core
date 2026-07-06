use crate::AppConfig;
use actix_web::{web, HttpResponse, Responder};
use askama::Template;
use serde::Deserialize;

#[derive(Template)]
#[template(path = "clock.html")]
struct ClockTemplate {
    title_font: String,
    format_hour: bool,
    format_minute: bool,
    format_second: bool,
}

#[derive(Deserialize)]
pub struct ClockQuery {
    hour: Option<bool>,
    minute: Option<bool>,
    second: Option<bool>,
}

pub async fn clock(
    config: web::Data<AppConfig>,
    query: web::Query<ClockQuery>,
) -> impl Responder {
    // Construire le chemin complet vers la police
    let font_path = if config.front_font_title.starts_with('/') {
        config.front_font_title.clone()
    } else {
        format!("/public/font/{}", config.front_font_title)
    };

    let tmpl = ClockTemplate {
        title_font: font_path,
        format_hour: query.hour.unwrap_or(true),
        format_minute: query.minute.unwrap_or(true),
        format_second: query.second.unwrap_or(true),
    };
    
    let rendered = tmpl.render().unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}

