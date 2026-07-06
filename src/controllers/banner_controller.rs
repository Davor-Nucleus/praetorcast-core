use actix_multipart::Multipart;
use actix_web::{web, HttpResponse, Responder};
use askama::Template;
use bytes::Bytes;
use futures_util::TryStreamExt;
use std::fs;
use std::io::Write;
use uuid::Uuid;
use crate::models::banner::{self, BannerCard};

#[derive(Template)]
#[template(path = "banner_config.html")]
struct BannerConfigTemplate;

pub async fn page() -> impl Responder {
    let html = BannerConfigTemplate.render().unwrap();
    HttpResponse::Ok().content_type("text/html").body(html)
}

pub async fn get() -> impl Responder {
    match banner::read() {
        Ok(cards) => HttpResponse::Ok().json(cards),
        Err(e) => {
            eprintln!("{}", e);
            HttpResponse::Ok().json(vec![] as Vec<BannerCard>)
        }
    }
}

pub async fn save(cards: web::Json<Vec<BannerCard>>) -> impl Responder {
    match banner::write(cards.into_inner()) {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({"success": true})),
        Err(e) => {
            eprintln!("{}", e);
            HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": "Failed to save banner config"}))
        }
    }
}

pub async fn upload(mut payload: Multipart) -> impl Responder {
    if let Err(e) = fs::create_dir_all("public/banner") {
        eprintln!("Error creating banner directory: {}", e);
        return HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": "Failed to create directory"}));
    }

    while let Ok(Some(mut field)) = payload.try_next().await {
        if let Some(filename) = field.content_disposition().get_filename() {
            let ext = filename.rfind('.').map(|i| &filename[i..]).unwrap_or("").to_owned();
            let new_filename = format!("{}{}", Uuid::new_v4(), ext);
            let filepath = format!("public/banner/{}", new_filename);

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
                        "path": format!("/public/banner/{}", new_filename)
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
