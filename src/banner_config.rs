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
pub struct BannerCard {
    pub id: Option<String>,
    pub text: String,
    #[serde(rename = "imagePath")]
    pub image_path: String,
    #[serde(rename = "transition")]
    pub transition: String,
    pub order: usize,
}

#[derive(Serialize, Deserialize)]
struct BannerConfig {
    cards: Vec<BannerCard>,
}

#[derive(Template)]
#[template(path = "banner_config.html")]
struct BannerConfigTemplate;

pub async fn banner_config() -> impl Responder {
    let tmpl = BannerConfigTemplate;
    let rendered = tmpl.render().unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}

pub async fn get_banner_config() -> impl Responder {
    match fs::read_to_string("data/banner.json") {
        Ok(content) => {
            match serde_json::from_str::<BannerConfig>(&content) {
                Ok(config) => {
                    // Normaliser les chemins d'images pour qu'ils soient relatifs à /public
                    let normalized_cards: Vec<BannerCard> = config
                        .cards
                        .into_iter()
                        .map(|mut card| {
                            if !card.image_path.starts_with("/public") && !card.image_path.starts_with("/banner") {
                                if card.image_path.starts_with("banner/") {
                                    card.image_path = format!("/public/{}", card.image_path);
                                } else if !card.image_path.is_empty() && !card.image_path.starts_with("/") {
                                    card.image_path = format!("/public/banner/{}", card.image_path);
                                }
                            } else if card.image_path.starts_with("/banner/") {
                                card.image_path = format!("/public{}", card.image_path);
                            }
                            card
                        })
                        .collect();
                    HttpResponse::Ok().json(normalized_cards)
                }
                Err(e) => {
                    eprintln!("Error parsing banner.json: {}", e);
                    HttpResponse::InternalServerError().json(serde_json::json!({"error": "Failed to parse config"}))
                }
            }
        }
        Err(e) => {
            eprintln!("Error reading banner.json: {}", e);
            HttpResponse::Ok().json(vec![] as Vec<BannerCard>)
        }
    }
}

pub async fn save_banner_config(cards: web::Json<Vec<BannerCard>>) -> impl Responder {
    // Réordonner les cartes selon leur index
    let ordered_cards: Vec<BannerCard> = cards
        .into_inner()
        .into_iter()
        .enumerate()
        .map(|(index, mut card)| {
            card.order = index;
            card
        })
        .collect();

    let config = BannerConfig {
        cards: ordered_cards,
    };

    match serde_json::to_string_pretty(&config) {
        Ok(json) => {
            // Créer le dossier data s'il n'existe pas
            if let Err(e) = fs::create_dir_all("data") {
                eprintln!("Error creating data directory: {}", e);
                return HttpResponse::InternalServerError()
                    .json(serde_json::json!({"error": "Failed to create data directory"}));
            }

            match fs::write("data/banner.json", json) {
                Ok(_) => HttpResponse::Ok().json(serde_json::json!({"success": true})),
                Err(e) => {
                    eprintln!("Error writing banner.json: {}", e);
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

pub async fn upload_banner_image(mut payload: Multipart) -> impl Responder {
    // Créer le dossier public/banner s'il n'existe pas
    if let Err(e) = fs::create_dir_all("public/banner") {
        eprintln!("Error creating banner directory: {}", e);
        return HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": "Failed to create banner directory"}));
    }

    while let Ok(Some(mut field)) = payload.try_next().await {
        if field.content_disposition().get_filename().is_some() {
            let filename = field.content_disposition().get_filename().unwrap();
            let extension = filename
                .rfind('.')
                .map(|i| &filename[i..])
                .unwrap_or("");
            let new_filename = format!("{}{}", Uuid::new_v4(), extension);
            let filepath = format!("public/banner/{}", new_filename);

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

