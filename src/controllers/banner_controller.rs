use actix_multipart::Multipart;
use actix_web::{web, HttpRequest, HttpResponse, Responder};
use actix_ws::Message;
use askama::Template;
use futures_util::StreamExt;
use tokio::time::{sleep, Duration};
use crate::controllers::upload::{save_upload, IMAGE_EXTENSIONS};
use crate::models::banner::{self, BannerConfig};

#[derive(Template)]
#[template(path = "banner_config.html")]
struct BannerConfigTemplate;

pub async fn page() -> impl Responder {
    let html = BannerConfigTemplate.render().unwrap();
    HttpResponse::Ok().content_type("text/html").body(html)
}

pub async fn get() -> impl Responder {
    match banner::read() {
        Ok(config) => HttpResponse::Ok().json(config),
        Err(e) => {
            eprintln!("{}", e);
            HttpResponse::Ok().json(BannerConfig::default())
        }
    }
}

/// GET /api/banner_ws — pousse la config du banner vers l'overlay en continu, pour
/// qu'un « Save » dans le configurateur se reflète sans rafraîchir la source OBS.
/// Même approche que `obs_controller::limiter_ws` : une boucle qui relit l'état
/// (ici le fichier `banner.json`) et n'émet que sur changement.
pub async fn banner_ws(req: HttpRequest, body: web::Payload) -> actix_web::Result<HttpResponse> {
    let (response, mut session, mut stream) = actix_ws::handle(&req, body)?;

    // `MessageStream` n'est pas `Send` : on spawne sur le runtime local d'actix.
    actix_web::rt::spawn(async move {
        // En cas d'erreur de lecture, on pousse une configuration vide (l'overlay
        // affiche alors son « empty state ») plutôt que de couper le flux.
        let read_snapshot = || {
            banner::read()
                .ok()
                .and_then(|config| serde_json::to_string(&config).ok())
                .unwrap_or_else(|| r#"{"cards":[],"dock":{"enabled":false}}"#.to_string())
        };

        // Premier envoi immédiat : la boucle ci-dessous n'émet qu'au bout d'une
        // seconde, l'overlay resterait vide jusque-là.
        let mut last = read_snapshot();
        if session.text(last.clone()).await.is_err() {
            return;
        }

        loop {
            tokio::select! {
                // Sans consommer le flux entrant, la fermeture d'une source OBS passe
                // inaperçue et les pings restent sans réponse.
                incoming = stream.next() => match incoming {
                    Some(Ok(Message::Ping(bytes))) => {
                        if session.pong(&bytes).await.is_err() {
                            return;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        let _ = session.close(None).await;
                        return;
                    }
                    Some(Err(_)) => return,
                    _ => {}
                },
                _ = sleep(Duration::from_millis(1000)) => {
                    let snapshot = read_snapshot();

                    if snapshot != last {
                        if session.text(snapshot.clone()).await.is_err() {
                            return;
                        }
                        last = snapshot;
                    }
                }
            }
        }
    });

    Ok(response)
}

pub async fn save(config: web::Json<BannerConfig>) -> impl Responder {
    match banner::write(&config.into_inner()) {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({"success": true})),
        Err(e) => {
            eprintln!("{}", e);
            HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": "Failed to save banner config"}))
        }
    }
}

pub async fn upload(payload: Multipart) -> impl Responder {
    save_upload(payload, "public/banner", "/public/banner", IMAGE_EXTENSIONS).await
}
