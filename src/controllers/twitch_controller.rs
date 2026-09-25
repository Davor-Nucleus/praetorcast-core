use std::sync::Mutex;
use actix_web::{web, HttpRequest, HttpResponse, Responder};
use actix_ws::Message;
use futures_util::StreamExt;
use serde_json::{Map, Value};
use tokio::time::{sleep, Duration};
use crate::models::config::load_config;
use crate::twitch::{broadcaster_id, BoxError, TwitchConfig, TwitchState};

/// GET /api/twitch/badges — carte des badges de chat.
///
/// Les appels Helix sont faits **ici**, côté serveur : c'est ce qui permet aux
/// overlays de chat de ne plus embarquer `TWITCH_OAUTH_TOKEN` dans leur HTML.
/// La réponse ne contient que des URL d'images publiques.
pub async fn badges() -> impl Responder {
    match fetch_badges().await {
        Ok(payload) => HttpResponse::Ok().json(payload),
        Err(e) => {
            eprintln!("[Twitch] Récupération des badges impossible: {e}");
            // L'overlay doit continuer à afficher le chat sans badges.
            HttpResponse::Ok().json(serde_json::json!({"global": {}, "channel": {}}))
        }
    }
}

/// État du direct gardé 30 s : chaque dock ouvert le relit à ce rythme, et plusieurs
/// docks ne doivent pas multiplier les appels Helix.
const STREAM_TTL: Duration = Duration::from_secs(30);

static STREAM_CACHE: Mutex<Option<(std::time::Instant, Value)>> = Mutex::new(None);

/// GET /api/twitch/stream — le direct selon Twitch : en ligne, début, spectateurs,
/// titre et catégorie.
///
/// Twitch fait foi, plutôt que `TwitchState::live` : ce dernier ne s'apprend qu'au
/// premier `stream.online`, et reste donc faux après un redémarrage du serveur en
/// plein direct. Une erreur rend `live: null` — « inconnu », pas « hors ligne ».
pub async fn stream() -> impl Responder {
    if let Some((at, cached)) = STREAM_CACHE.lock().unwrap().as_ref() {
        if at.elapsed() < STREAM_TTL {
            return HttpResponse::Ok().json(cached);
        }
    }

    match fetch_stream().await {
        Ok(info) => {
            *STREAM_CACHE.lock().unwrap() = Some((std::time::Instant::now(), info.clone()));
            HttpResponse::Ok().json(info)
        }
        Err(e) => {
            eprintln!("[Twitch] État du direct indisponible: {e}");
            HttpResponse::Ok().json(serde_json::json!({ "live": null, "error": e.to_string() }))
        }
    }
}

async fn fetch_stream() -> Result<Value, BoxError> {
    let twitch = TwitchConfig::from_app(&load_config());
    let client = reqwest::Client::new();
    let bid = broadcaster_id(&client, &twitch).await?;

    let response = client
        .get("https://api.twitch.tv/helix/streams")
        .query(&[("user_id", bid.as_str())])
        .header("Client-Id", &twitch.client_id)
        .header("Authorization", twitch.bearer())
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(format!("helix/streams a renvoyé HTTP {}", response.status()).into());
    }

    let body: Value = response.json().await?;
    Ok(stream_info(&body))
}

/// Réduit la réponse de `helix/streams` à ce qu'affiche le dock. Hors ligne, Helix
/// renvoie une liste vide.
fn stream_info(body: &Value) -> Value {
    let stream = &body["data"][0];
    if stream["type"].as_str() != Some("live") {
        return serde_json::json!({ "live": false });
    }
    serde_json::json!({
        "live": true,
        "startedAt": stream["started_at"],
        "viewers": stream["viewer_count"],
        "title": stream["title"],
        "game": stream["game_name"],
    })
}

/// Emotes gardées une heure : la pluie en demande à chaque chargement de source, et
/// la liste d'une chaîne ne change qu'avec un nouvel emote.
const EMOTES_TTL: std::time::Duration = std::time::Duration::from_secs(3600);

static EMOTES_CACHE: Mutex<Option<(std::time::Instant, Vec<String>)>> = Mutex::new(None);

/// GET /api/twitch/emotes — images des emotes de la chaîne, pour la pluie d'emotes.
///
/// Une chaîne sans emote (pas encore affiliée) retombe sur les emotes globales de
/// Twitch : une pluie vide ne ressemblerait à rien. Une erreur rend une liste vide,
/// que l'overlay remplace par ses propres symboles.
pub async fn emotes() -> impl Responder {
    if let Some((at, urls)) = EMOTES_CACHE.lock().unwrap().as_ref() {
        if at.elapsed() < EMOTES_TTL {
            return HttpResponse::Ok().json(serde_json::json!({ "emotes": urls }));
        }
    }

    match fetch_emotes().await {
        Ok(urls) => {
            *EMOTES_CACHE.lock().unwrap() = Some((std::time::Instant::now(), urls.clone()));
            HttpResponse::Ok().json(serde_json::json!({ "emotes": urls }))
        }
        Err(e) => {
            eprintln!("[Twitch] Récupération des emotes impossible: {e}");
            HttpResponse::Ok().json(serde_json::json!({ "emotes": [] }))
        }
    }
}

async fn fetch_emotes() -> Result<Vec<String>, BoxError> {
    let twitch = TwitchConfig::from_app(&load_config());
    let client = reqwest::Client::new();
    let bid = broadcaster_id(&client, &twitch).await?;

    let channel = emote_urls(
        &client,
        &twitch,
        "https://api.twitch.tv/helix/chat/emotes",
        Some(("broadcaster_id", bid.as_str())),
    )
    .await?;
    if !channel.is_empty() {
        return Ok(channel);
    }
    emote_urls(&client, &twitch, "https://api.twitch.tv/helix/chat/emotes/global", None).await
}

async fn emote_urls(
    client: &reqwest::Client,
    config: &TwitchConfig,
    url: &str,
    query: Option<(&str, &str)>,
) -> Result<Vec<String>, BoxError> {
    let mut request = client
        .get(url)
        .header("Client-Id", &config.client_id)
        .header("Authorization", config.bearer());
    if let Some(q) = query {
        request = request.query(&[q]);
    }

    let response = request.send().await?;
    if !response.status().is_success() {
        return Err(format!("{url} a renvoyé HTTP {}", response.status()).into());
    }

    let body: Value = response.json().await?;
    Ok(emote_images(&body))
}

/// URL 2x de chaque emote d'une réponse Helix : assez nette pour tomber en grand.
fn emote_images(body: &Value) -> Vec<String> {
    body["data"]
        .as_array()
        .map(|emotes| {
            emotes
                .iter()
                .filter_map(|e| e["images"]["url_2x"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

async fn fetch_badges() -> Result<Value, BoxError> {
    let twitch = TwitchConfig::from_app(&load_config());

    let client = reqwest::Client::new();
    let bid = broadcaster_id(&client, &twitch).await?;

    let global = badge_set(&client, &twitch, "https://api.twitch.tv/helix/chat/badges/global", None).await?;
    let channel = badge_set(
        &client,
        &twitch,
        "https://api.twitch.tv/helix/chat/badges",
        Some(("broadcaster_id", bid.as_str())),
    )
    .await?;

    Ok(serde_json::json!({ "global": global, "channel": channel }))
}

/// Aplatit la réponse Helix en `{"set_id/version": "url"}`, la forme attendue par
/// `chat-common.js::parseBadges`.
async fn badge_set(
    client: &reqwest::Client,
    config: &TwitchConfig,
    url: &str,
    query: Option<(&str, &str)>,
) -> Result<Map<String, Value>, BoxError> {
    let mut request = client
        .get(url)
        .header("Client-Id", &config.client_id)
        .header("Authorization", config.bearer());
    if let Some(q) = query {
        request = request.query(&[q]);
    }

    let response = request.send().await?;
    if !response.status().is_success() {
        return Err(format!("{url} a renvoyé HTTP {}", response.status()).into());
    }

    let body: Value = response.json().await?;
    let mut map = Map::new();
    if let Some(sets) = body["data"].as_array() {
        for set in sets {
            let Some(set_id) = set["set_id"].as_str() else { continue };
            let Some(versions) = set["versions"].as_array() else { continue };
            for version in versions {
                let (Some(id), Some(image)) =
                    (version["id"].as_str(), version["image_url_1x"].as_str())
                else {
                    continue;
                };
                map.insert(format!("{set_id}/{id}"), Value::String(image.to_string()));
            }
        }
    }
    Ok(map)
}

pub async fn ws_handler(
    req: HttpRequest,
    body: web::Payload,
    state: web::Data<Mutex<TwitchState>>,
) -> actix_web::Result<HttpResponse> {
    let (response, mut session, mut stream) = actix_ws::handle(&req, body)?;
    let state = state.into_inner();

    // `MessageStream` n'est pas `Send` : on spawne sur le runtime local d'actix.
    actix_web::rt::spawn(async move {
        let snapshot = |state: &Mutex<TwitchState>| {
            let g = state.lock().unwrap();
            serde_json::json!({
                "total_followers": g.total_followers,
                "last_follower": g.last_follower,
                // RFC 3339, pour le « il y a 4 min » du dock /followers-info.
                "lastFollowerAt": g.last_follower_at,
                "connected": g.connected,
                // `stream.online` / `stream.offline`. Faux tant qu'aucune
                // notification n'est arrivée : un serveur lancé en cours de direct
                // ne le sait pas avant le prochain basculement.
                "live": g.live,
                // RFC 3339 tel que Twitch l'envoie : `Date.parse` le lit nativement.
                "streamStartedAt": g.stream_started_at,
            })
            .to_string()
        };

        // Envoi immédiat : la boucle n'émettait rien pendant la première demi-seconde.
        let mut last = snapshot(&state);
        if session.text(last.clone()).await.is_err() {
            return;
        }

        loop {
            tokio::select! {
                // Sans consommer le flux entrant, une déconnexion client passe
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
                _ = sleep(Duration::from_millis(500)) => {
                    let current = snapshot(&state);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_images_2x_sont_extraites_de_la_reponse_helix() {
        // Forme de la documentation Helix « Get Channel Emotes », abrégée.
        let body = serde_json::json!({
            "data": [
                { "id": "304456832", "name": "twitchdevPitchfork",
                  "images": { "url_1x": "https://x/1.0", "url_2x": "https://x/2.0", "url_4x": "https://x/3.0" } },
                { "id": "sans-images", "name": "cassé" }
            ],
            "template": "https://static-cdn.jtvnw.net/emoticons/v2/{{id}}/{{format}}/{{theme_mode}}/{{scale}}"
        });
        assert_eq!(emote_images(&body), vec!["https://x/2.0".to_string()]);
    }

    #[test]
    fn une_reponse_sans_donnees_ne_donne_aucune_emote() {
        assert!(emote_images(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn un_direct_en_cours_donne_debut_spectateurs_titre_et_categorie() {
        // Forme de la documentation Helix « Get Streams », abrégée.
        let body = serde_json::json!({
            "data": [{
                "id": "123", "user_login": "lordrutra", "game_name": "Warhammer 40,000: Space Marine 2",
                "type": "live", "title": "Pour l'Empereur !", "viewer_count": 42,
                "started_at": "2026-09-25T17:00:00Z"
            }],
            "pagination": {}
        });
        assert_eq!(
            stream_info(&body),
            serde_json::json!({
                "live": true,
                "startedAt": "2026-09-25T17:00:00Z",
                "viewers": 42,
                "title": "Pour l'Empereur !",
                "game": "Warhammer 40,000: Space Marine 2",
            })
        );
    }

    #[test]
    fn hors_ligne_helix_rend_une_liste_vide() {
        let body = serde_json::json!({ "data": [], "pagination": {} });
        assert_eq!(stream_info(&body), serde_json::json!({ "live": false }));
    }
}
