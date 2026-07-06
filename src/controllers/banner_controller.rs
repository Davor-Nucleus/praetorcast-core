use actix_multipart::Multipart;
use actix_web::{web, HttpRequest, HttpResponse, Responder};
use askama::Template;
use bytes::Bytes;
use futures_util::TryStreamExt;
use std::fs;
use std::io::Write;
use tokio::time::{sleep, Duration};
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

/// GET /api/banner_ws — pousse la config du banner vers l'overlay en continu, pour
/// qu'un « Save » dans le configurateur se reflète sans rafraîchir la source OBS.
/// Même approche que `obs_controller::limiter_ws` : une boucle qui relit l'état
/// (ici le fichier `banner.json`) et n'émet que sur changement.
pub async fn banner_ws(req: HttpRequest, body: web::Payload) -> actix_web::Result<HttpResponse> {
    let (response, mut session, _) = actix_ws::handle(&req, body)?;

    tokio::spawn(async move {
        let mut last = String::new();

        loop {
            // En cas d'erreur de lecture, on pousse une liste vide (l'overlay
            // affiche alors son « empty state ») plutôt que de couper le flux.
            let snapshot = banner::read()
                .ok()
                .and_then(|cards| serde_json::to_string(&cards).ok())
                .unwrap_or_else(|| "[]".to_string());

            if snapshot != last {
                if session.text(snapshot.clone()).await.is_err() {
                    break;
                }
                last = snapshot;
            }

            sleep(Duration::from_millis(1000)).await;
        }
    });

    Ok(response)
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
