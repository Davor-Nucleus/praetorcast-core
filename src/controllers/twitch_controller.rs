use std::sync::Mutex;
use actix_web::{web, HttpRequest, HttpResponse};
use tokio::time::{sleep, Duration};
use crate::twitch::TwitchState;

pub async fn ws_handler(
    req: HttpRequest,
    body: web::Payload,
    state: web::Data<Mutex<TwitchState>>,
) -> actix_web::Result<HttpResponse> {
    let (response, mut session, _) = actix_ws::handle(&req, body)?;
    let state = state.into_inner();

    tokio::spawn(async move {
        let mut last = String::new();
        loop {
            sleep(Duration::from_millis(500)).await;

            let snapshot = {
                let g = state.lock().unwrap();
                serde_json::json!({
                    "total_followers": g.total_followers,
                    "last_follower": g.last_follower,
                    "connected": g.connected,
                })
                .to_string()
            };

            if snapshot != last {
                if session.text(snapshot.clone()).await.is_err() {
                    break;
                }
                last = snapshot;
            }
        }
    });

    Ok(response)
}
