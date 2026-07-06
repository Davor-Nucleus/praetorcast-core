use crate::AppConfig;
use actix_multipart::Multipart;
use actix_web::{web, HttpResponse, Responder};
use askama::Template;
use bytes::Bytes;
use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone)]
pub struct DaySchedule {
    #[serde(rename = "dayIndex")]
    pub day_index: usize,
    #[serde(rename = "day")]
    pub day: String,
    #[serde(rename = "date")]
    pub date: String,
    #[serde(rename = "title")]
    pub title: String,
    #[serde(rename = "coverPath")]
    pub cover_path: String,
}

#[derive(Serialize, Deserialize)]
pub struct SchedulerConfig {
    pub schedule: Vec<DaySchedule>,
    #[serde(rename = "backgroundImage")]
    pub background_image: Option<String>,
}

#[derive(Template)]
#[template(path = "scheduler.html")]
struct SchedulerTemplate {
    title_font: String,
}

pub async fn scheduler(config: web::Data<AppConfig>) -> impl Responder {
    // Construire le chemin complet vers la police de titre front
    let font_path = if config.front_font_title.starts_with('/') {
        config.front_font_title.clone()
    } else {
        format!("/public/font/{}", config.front_font_title)
    };

    let tmpl = SchedulerTemplate {
        title_font: font_path,
    };
    let rendered = tmpl.render().unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}

pub async fn get_scheduler_config() -> impl Responder {
    match fs::read_to_string("data/scheduler.json") {
        Ok(content) => {
            match serde_json::from_str::<SchedulerConfig>(&content) {
                Ok(mut config) => {
                    // Normaliser les chemins d'images
                    for day in &mut config.schedule {
                        if !day.cover_path.is_empty() && !day.cover_path.starts_with("/public") {
                            if day.cover_path.starts_with("/scheduler/") {
                                day.cover_path = format!("/public{}", day.cover_path);
                            } else if day.cover_path.starts_with("scheduler/") {
                                day.cover_path = format!("/public/{}", day.cover_path);
                            } else if !day.cover_path.starts_with("/") {
                                day.cover_path = format!("/public/scheduler/{}", day.cover_path);
                            }
                        }
                    }
                    if let Some(ref mut bg) = config.background_image {
                        if !bg.is_empty() && !bg.starts_with("/public") {
                            if bg.starts_with("/scheduler/") {
                                *bg = format!("/public{}", bg);
                            } else if bg.starts_with("scheduler/") {
                                *bg = format!("/public/{}", bg);
                            } else if !bg.starts_with("/") {
                                *bg = format!("/public/scheduler/{}", bg);
                            }
                        }
                    }
                    HttpResponse::Ok().json(config)
                }
                Err(e) => {
                    eprintln!("Error parsing scheduler.json: {}", e);
                    HttpResponse::InternalServerError()
                        .json(serde_json::json!({"error": "Failed to parse config"}))
                }
            }
        }
        Err(e) => {
            eprintln!("Error reading scheduler.json: {}", e);
            HttpResponse::Ok().json(SchedulerConfig {
                schedule: vec![],
                background_image: None,
            })
        }
    }
}

pub async fn save_scheduler_config(config: web::Json<SchedulerConfig>) -> impl Responder {
    let config = config.into_inner();

    match serde_json::to_string_pretty(&config) {
        Ok(json) => {
            if let Err(e) = fs::create_dir_all("data") {
                eprintln!("Error creating data directory: {}", e);
                return HttpResponse::InternalServerError()
                    .json(serde_json::json!({"error": "Failed to create data directory"}));
            }

            match fs::write("data/scheduler.json", json) {
                Ok(_) => HttpResponse::Ok().json(serde_json::json!({"success": true})),
                Err(e) => {
                    eprintln!("Error writing scheduler.json: {}", e);
                    HttpResponse::InternalServerError()
                        .json(serde_json::json!({"error": "Failed to write config"}))
                }
            }
        }
        Err(e) => {
            eprintln!("Error serializing config: {}", e);
            HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": "Failed to serialize config"}))
        }
    }
}

pub async fn upload_scheduler_image(mut payload: Multipart) -> impl Responder {
    if let Err(e) = fs::create_dir_all("public/scheduler") {
        eprintln!("Error creating scheduler directory: {}", e);
        return HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": "Failed to create scheduler directory"}));
    }

    while let Ok(Some(mut field)) = payload.try_next().await {
        if field.content_disposition().get_filename().is_some() {
            let filename = field.content_disposition().get_filename().unwrap();
            let extension = filename
                .rfind('.')
                .map(|i| &filename[i..])
                .unwrap_or("");
            let new_filename = format!("{}{}", Uuid::new_v4(), extension);
            let filepath = format!("public/scheduler/{}", new_filename);

            match fs::File::create(&filepath) {
                Ok(mut file) => {
                    while let Ok(Some(chunk)) = field.try_next().await {
                        let bytes: Bytes = chunk;
                        if let Err(e) = file.write_all(&bytes) {
                            eprintln!("Error writing file chunk: {}", e);
                            return HttpResponse::InternalServerError()
                                .json(serde_json::json!({"error": "Failed to write file"}));
                        }
                    }
                    return HttpResponse::Ok().json(serde_json::json!({
                        "success": true,
                        "path": format!("/public/scheduler/{}", new_filename)
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

pub async fn upload_background_image(mut payload: Multipart) -> impl Responder {
    if let Err(e) = fs::create_dir_all("public/scheduler") {
        eprintln!("Error creating scheduler directory: {}", e);
        return HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": "Failed to create scheduler directory"}));
    }

    while let Ok(Some(mut field)) = payload.try_next().await {
        if field.content_disposition().get_filename().is_some() {
            let filename = field.content_disposition().get_filename().unwrap();
            let extension = filename
                .rfind('.')
                .map(|i| &filename[i..])
                .unwrap_or("");
            let new_filename = format!("{}{}", Uuid::new_v4(), extension);
            let filepath = format!("public/scheduler/{}", new_filename);

            match fs::File::create(&filepath) {
                Ok(mut file) => {
                    while let Ok(Some(chunk)) = field.try_next().await {
                        let bytes: Bytes = chunk;
                        if let Err(e) = file.write_all(&bytes) {
                            eprintln!("Error writing file chunk: {}", e);
                            return HttpResponse::InternalServerError()
                                .json(serde_json::json!({"error": "Failed to write file"}));
                        }
                    }
                    return HttpResponse::Ok().json(serde_json::json!({
                        "success": true,
                        "path": format!("/public/scheduler/{}", new_filename)
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

