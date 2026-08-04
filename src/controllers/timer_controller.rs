//! Compte à rebours : overlay, page de contrôle, actions et flux temps réel.
//!
//! Découpage calqué sur `goal_controller` — une page d'affichage, une page de
//! configuration, un GET, un POST, un WebSocket. La différence tient aux **actions** :
//! l'overlay OBS n'a aucun bouton (les contrôles y sont désactivés et un
//! rafraîchissement de source remettrait le décompte à zéro), donc Démarrer / Pause /
//! Reset vivent sur `/timer-config` et passent par `/api/timer/*`.

use actix_web::{web, HttpRequest, HttpResponse, Responder};
use actix_ws::Message;
use askama::Template;
use futures_util::StreamExt;
use serde::Deserialize;
use tokio::sync::Notify;
use tokio::time::{sleep, Duration};

use crate::models::timer::{self, Timer, TimerSettings};

/// Filet de sécurité du flux : un client qui n'était pas encore en attente au moment
/// du clic rattrape l'état au tour suivant. La diffusion normale, elle, est immédiate.
const TICK: Duration = Duration::from_millis(1000);

/// Réveil des flux `/api/timer_ws` après une action.
///
/// Type propre plutôt qu'un `Arc<Notify>` nu : `app_data` indexe par type, et
/// `web::Data<Arc<Notify>>` est déjà pris par le rechargement Twitch.
pub struct TimerNotify(pub Notify);

#[derive(Template)]
#[template(path = "timer.html")]
pub struct TimerTemplate;

#[derive(Template)]
#[template(path = "timer_config.html")]
struct TimerConfigTemplate;

pub async fn display() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html")
        .body(TimerTemplate.render().unwrap())
}

pub async fn page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html")
        .body(TimerConfigTemplate.render().unwrap())
}

/// Lecture tolérante : un fichier illisible ne doit pas vider l'overlay en direct.
fn current() -> Timer {
    timer::read().unwrap_or_else(|e| {
        eprintln!("{e}");
        Timer::default()
    })
}

/// Charge utile envoyée à l'overlay et à la page de contrôle.
///
/// `remainingMs` est la valeur **au moment de l'envoi** : l'afficheur extrapole
/// ensuite avec son propre chronomètre, ce qui évite de pousser un message par
/// seconde et de dépendre d'une horloge partagée.
fn snapshot(timer: &Timer) -> serde_json::Value {
    serde_json::json!({
        "title": timer.title,
        "running": timer.running,
        "remainingMs": timer.remaining_at(timer::now_ms()),
        "durationMs": timer.duration_ms,
        "endText": timer.end_text,
        "hideWhenZero": timer.hide_when_zero,
        "showProgress": timer.show_progress,
        "accentColor": timer.accent_color,
    })
}

/// Applique une mutation, la persiste, et réveille les afficheurs connectés.
fn mutate(notify: &TimerNotify, change: impl FnOnce(&mut Timer, u64)) -> HttpResponse {
    let mut timer = current();
    change(&mut timer, timer::now_ms());

    match timer::write(&timer) {
        Ok(()) => {
            notify.0.notify_waiters();
            HttpResponse::Ok().json(snapshot(&timer))
        }
        Err(e) => {
            eprintln!("{e}");
            HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": "Failed to save timer state"}))
        }
    }
}

pub async fn get() -> impl Responder {
    HttpResponse::Ok().json(snapshot(&current()))
}

pub async fn save(
    settings: web::Json<TimerSettings>,
    notify: web::Data<TimerNotify>,
) -> impl Responder {
    mutate(&notify, |timer, _| timer.apply_settings(settings.into_inner()))
}

pub async fn start(notify: web::Data<TimerNotify>) -> impl Responder {
    mutate(&notify, |timer, now| timer.start_at(now))
}

pub async fn pause(notify: web::Data<TimerNotify>) -> impl Responder {
    mutate(&notify, |timer, now| timer.pause_at(now))
}

/// Démarrer / mettre en pause d'un seul appel — la forme qui se câble sur une touche
/// unique ou un bouton de Stream Deck.
pub async fn toggle(notify: web::Data<TimerNotify>) -> impl Responder {
    mutate(&notify, |timer, now| timer.toggle_at(now))
}

pub async fn reset(notify: web::Data<TimerNotify>) -> impl Responder {
    mutate(&notify, |timer, _| timer.reset())
}

#[derive(Deserialize)]
pub struct Adjust {
    /// Millisecondes à ajouter, négatif pour retirer.
    #[serde(rename = "deltaMs")]
    delta_ms: i64,
}

/// `/api/timer/adjust?deltaMs=60000`.
///
/// Le paramètre est lu dans l'URL et pas dans le corps : la même URL fonctionne
/// alors en GET comme en POST, donc depuis un simple raccourci ou un favori.
pub async fn adjust(query: web::Query<Adjust>, notify: web::Data<TimerNotify>) -> impl Responder {
    let delta = query.delta_ms;
    mutate(&notify, |timer, now| timer.adjust_at(now, delta))
}

/// GET /api/timer_ws — pousse l'état à chaque action, et une fois par seconde en
/// filet de sécurité.
pub async fn timer_ws(
    req: HttpRequest,
    body: web::Payload,
    notify: web::Data<TimerNotify>,
) -> actix_web::Result<HttpResponse> {
    let (response, mut session, mut stream) = actix_ws::handle(&req, body)?;
    let notify = notify.into_inner();

    // `MessageStream` n'est pas `Send` : on spawne sur le runtime local d'actix.
    actix_web::rt::spawn(async move {
        // Envoi immédiat, sinon l'overlay reste vide jusqu'au premier tour.
        let timer = current();
        if session.text(snapshot(&timer).to_string()).await.is_err() {
            return;
        }
        let mut last = serde_json::to_string(&timer).unwrap_or_default();

        loop {
            let forced = tokio::select! {
                // Sans consommer le flux entrant, la fermeture d'une source OBS
                // passe inaperçue et les pings restent sans réponse.
                incoming = stream.next() => match incoming {
                    Some(Ok(Message::Ping(bytes))) => {
                        if session.pong(&bytes).await.is_err() {
                            return;
                        }
                        false
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        let _ = session.close(None).await;
                        return;
                    }
                    Some(Err(_)) => return,
                    _ => false,
                },
                _ = notify.0.notified() => true,
                _ = sleep(TICK) => false,
            };

            let timer = current();
            // La comparaison porte sur l'état persisté, pas sur l'instantané : ce
            // dernier contient le temps restant, qui change en permanence pendant
            // que le compteur tourne et déclencherait un envoi par tour.
            let state = serde_json::to_string(&timer).unwrap_or_default();
            if forced || state != last {
                if session.text(snapshot(&timer).to_string()).await.is_err() {
                    return;
                }
                last = state;
            }
        }
    });

    Ok(response)
}
