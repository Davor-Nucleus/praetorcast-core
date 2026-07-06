use actix_multipart::Multipart;
use actix_web::{web, HttpResponse, Responder};
use askama::Template;
use bytes::Bytes;
use futures_util::TryStreamExt;
use std::fs;
use std::io::Write;
use uuid::Uuid;
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

async fn save_upload(mut payload: Multipart, dir: &str, public_path: &str) -> HttpResponse {
    if let Err(e) = fs::create_dir_all(dir) {
        eprintln!("Error creating directory {}: {}", dir, e);
        return HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": "Failed to create directory"}));
    }

    while let Ok(Some(mut field)) = payload.try_next().await {
        if let Some(filename) = field.content_disposition().get_filename() {
            let ext = filename.rfind('.').map(|i| &filename[i..]).unwrap_or("").to_owned();
            let new_filename = format!("{}{}", Uuid::new_v4(), ext);
            let filepath = format!("{}/{}", dir, new_filename);

            match fs::File::create(&filepath) {
                Ok(mut file) => {
                    while let Ok(Some(chunk)) = field.try_next().await {
                        let bytes: Bytes = chunk;
                        if let Err(e) = file.write_all(&bytes) {
                            eprintln!("Error writing chunk: {}", e);
                            return HttpResponse::InternalServerError()
                                .json(serde_json::json!({"error": "Failed to write file"}));
                        }
                    }
                    return HttpResponse::Ok().json(serde_json::json!({
                        "success": true,
                        "path": format!("{}/{}", public_path, new_filename)
                    }));
                }
                Err(e) => {
                    eprintln!("Error creating file: {}", e);
                    return HttpResponse::InternalServerError()
                        .json(serde_json::json!({"error": "Failed to create file"}));
                }
            }
        }
    }

    HttpResponse::BadRequest().json(serde_json::json!({"error": "No file provided"}))
}

pub async fn upload_image(payload: Multipart) -> impl Responder {
    save_upload(payload, "public/scheduler", "/public/scheduler").await
}

pub async fn upload_background(payload: Multipart) -> impl Responder {
    save_upload(payload, "public/scheduler", "/public/scheduler").await
}
