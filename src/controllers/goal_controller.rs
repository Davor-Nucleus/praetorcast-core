//! Barre d'objectif : page de configuration, API REST et flux temps réel.
//!
//! Découpage calqué sur `banner_controller` — une page, un GET, un POST, un
//! WebSocket qui ne pousse que sur changement.

use std::sync::Mutex;
use std::time::{Duration as StdDuration, Instant};

use actix_web::{web, HttpRequest, HttpResponse, Responder};
use actix_ws::Message;
use askama::Template;
use futures_util::StreamExt;
use tokio::time::{sleep, Duration};

use crate::models::config::load_config;
use crate::models::goal::{self, Goal, GoalSource};
use crate::twitch::{broadcaster_id, subscriber_count, TwitchConfig, TwitchState};

/// Durée de validité du compteur d'abonnés.
///
/// Le nombre d'abonnés bouge de quelques unités par stream : l'interroger à chaque
/// tour de boucle brûlerait le quota Helix pour rien. C'est la leçon tirée de
/// `/api/twitch/badges`, qui refait ses appels à chaque chargement d'overlay.
const SUBS_TTL: StdDuration = StdDuration::from_secs(60);

/// Cadence du flux : identique à `banner_ws`.
const TICK: Duration = Duration::from_millis(1000);

#[derive(Template)]
#[template(path = "goal_config.html")]
struct GoalConfigTemplate;

/// La police vient de `/theme.css`, plus d'un `@font-face` par overlay.
#[derive(Template)]
#[template(path = "goal.html")]
pub struct GoalTemplate;

pub async fn page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html")
        .body(GoalConfigTemplate.render().unwrap())
}

pub async fn display() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html")
        .body(GoalTemplate.render().unwrap())
}

pub async fn get() -> impl Responder {
    match goal::read() {
        Ok(goals) => HttpResponse::Ok().json(goals),
        Err(e) => {
            eprintln!("{e}");
            // L'overlay et le configurateur doivent rester utilisables même si le
            // fichier est illisible : on renvoie une barre par défaut.
            HttpResponse::Ok().json(vec![Goal::default()])
        }
    }
}

pub async fn save(goals: web::Json<Vec<Goal>>) -> impl Responder {
    match goal::write(&goals.into_inner()) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"success": true})),
        Err(e) => {
            eprintln!("{e}");
            HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": "Failed to save goal config"}))
        }
    }
}

/// Dernier relevé d'abonnés : `(instant, valeur ou message d'erreur)`.
static SUBS_CACHE: Mutex<Option<(Instant, Result<u64, String>)>> = Mutex::new(None);

/// Compte d'abonnés, au plus une requête Helix par `SUBS_TTL`.
///
/// L'erreur est mise en cache comme le succès : un jeton sans le bon scope échoue de
/// façon permanente, inutile de retenter chaque seconde.
async fn cached_subscriber_count() -> Result<u64, String> {
    if let Some((at, cached)) = SUBS_CACHE.lock().unwrap().as_ref() {
        if at.elapsed() < SUBS_TTL {
            return cached.clone();
        }
    }

    let twitch = TwitchConfig::from_app(&load_config());

    let client = reqwest::Client::new();
    let fresh = async {
        let bid = broadcaster_id(&client, &twitch).await?;
        subscriber_count(&client, &twitch, &bid).await
    }
    .await
    .map_err(|e| e.to_string());

    *SUBS_CACHE.lock().unwrap() = Some((Instant::now(), fresh.clone()));
    fresh
}

/// Total de followers, ou l'avertissement à afficher tant qu'EventSub n'est pas prêt.
fn follower_total(state: &Mutex<TwitchState>) -> (Option<u64>, Option<String>) {
    let g = state.lock().unwrap();
    if g.connected {
        (Some(g.total_followers), None)
    } else {
        // Pas encore connecté à EventSub : le total vaut 0 par défaut, ce qui
        // afficherait une barre vide au lieu d'un état « en attente ».
        (None, Some("Connexion à Twitch en cours…".to_string()))
    }
}

/// Charge utile envoyée à l'overlay et au configurateur : une entrée par barre.
///
/// `current` est `null` quand la source est indisponible : la barre s'affiche alors
/// sans valeur plutôt qu'avec un 0 qui ferait croire à une remise à zéro.
///
/// Les deux sources externes sont résolues **une seule fois** pour toute la liste,
/// même si plusieurs barres les partagent : sans ça, trois objectifs « abonnés »
/// déclencheraient trois fois plus d'appels Helix.
async fn snapshot(state: &Mutex<TwitchState>) -> String {
    let goals = goal::read().unwrap_or_else(|e| {
        eprintln!("{e}");
        Vec::new()
    });

    let needs_followers = goals.iter().any(|g| g.source == GoalSource::Followers);
    let needs_subs = goals.iter().any(|g| g.source == GoalSource::Subs);

    let followers = needs_followers.then(|| follower_total(state));
    let subs = match needs_subs {
        true => Some(match cached_subscriber_count().await {
            Ok(total) => (Some(total), None),
            Err(e) => (None, Some(e)),
        }),
        false => None,
    };

    let entries: Vec<_> = goals
        .into_iter()
        .map(|goal| {
            let (measured, warning) = match goal.source {
                GoalSource::Followers => followers.clone().unwrap_or((None, None)),
                GoalSource::Subs => subs.clone().unwrap_or((None, None)),
                GoalSource::Manual => (None, None),
            };

            let current = goal.current_from(measured);
            let percent = current.map(|c| goal.percent(c));

            serde_json::json!({
                "config": goal,
                "current": current,
                "percent": percent,
                "warning": warning,
            })
        })
        .collect();

    serde_json::json!({ "goals": entries }).to_string()
}

/// GET /api/goal_ws — pousse la configuration et l'avancement en continu.
pub async fn goal_ws(
    req: HttpRequest,
    body: web::Payload,
    state: web::Data<Mutex<TwitchState>>,
) -> actix_web::Result<HttpResponse> {
    let (response, mut session, mut stream) = actix_ws::handle(&req, body)?;
    let state = state.into_inner();

    // `MessageStream` n'est pas `Send` : on spawne sur le runtime local d'actix.
    actix_web::rt::spawn(async move {
        // Envoi immédiat, sinon l'overlay reste vide une seconde au démarrage.
        let mut last = snapshot(&state).await;
        if session.text(last.clone()).await.is_err() {
            return;
        }

        loop {
            tokio::select! {
                // Sans consommer le flux entrant, la fermeture d'une source OBS
                // passe inaperçue et les pings restent sans réponse.
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
                _ = sleep(TICK) => {
                    let current = snapshot(&state).await;
                    if current != last {
                        if session.text(current.clone()).await.is_err() {
                            return;
                        }
                        last = current;
                    }
                }
            }
        }
    });

    Ok(response)
}
