use actix_web::{HttpResponse, Responder};
use askama::Template;
use crate::load_config;

#[derive(Template)]
#[template(path = "banner.html")]
struct BannerTemplate {
    title_font: String,
}

pub async fn banner() -> impl Responder {
    let config = load_config();
    // Construire le chemin complet vers la police
    let font_path = if config.front_font_title.starts_with('/') {
        config.front_font_title.clone()
    } else {
        format!("/public/{}", config.front_font_title)
    };

    let tmpl = BannerTemplate {
        title_font: font_path,
    };
    
    let rendered = tmpl.render().unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}

