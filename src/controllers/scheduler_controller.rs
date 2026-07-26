use actix_multipart::Multipart;
use actix_web::{web, HttpResponse, Responder};
use askama::Template;
use crate::controllers::upload::{save_upload, IMAGE_EXTENSIONS};
use crate::models::config::{font_path, load_config};
use crate::models::scheduler::{self, SchedulerConfig};

#[derive(Template)]
#[template(path = "scheduler.html")]
struct SchedulerTemplate {
    title_font: String,
}

pub async fn page() -> impl Responder {
    let config = load_config();
    let html = SchedulerTemplate { title_font: font_path(&config) }.render().unwrap();
    HttpResponse::Ok().content_type("text/html").body(html)
}

pub async fn get() -> impl Responder {
    match scheduler::read() {
        Ok(config) => HttpResponse::Ok().json(config),
        Err(e) => {
            eprintln!("{}", e);
            HttpResponse::Ok().json(SchedulerConfig { schedule: vec![], background_image: None })
        }
    }
}

pub async fn save(config: web::Json<SchedulerConfig>) -> impl Responder {
    match scheduler::write(&config.into_inner()) {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({"success": true})),
        Err(e) => {
            eprintln!("{}", e);
            HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": "Failed to save scheduler config"}))
        }
    }
}

pub async fn upload_image(payload: Multipart) -> impl Responder {
    save_upload(payload, "public/scheduler", "/public/scheduler", IMAGE_EXTENSIONS).await
}

pub async fn upload_background(payload: Multipart) -> impl Responder {
    save_upload(payload, "public/scheduler", "/public/scheduler", IMAGE_EXTENSIONS).await
}
