use crate::AppConfig;
use actix_web::{web, HttpResponse, Responder};
use askama::Template;

#[derive(Template)]
#[template(path = "music_current.html")]
struct MusicCurrentTemplate {
    music_port: u16,
    title_font: String,
}

pub async fn music_current(config: web::Data<AppConfig>) -> impl Responder {
    // Construire le chemin complet vers la police
    let font_path = if config.front_font_title.starts_with('/') {
        config.front_font_title.clone()
    } else {
        format!("/public/{}", config.front_font_title)
    };

    let tmpl = MusicCurrentTemplate {
        music_port: config.port_music,
        title_font: font_path,
    };
    
    let rendered = tmpl.render().unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}

