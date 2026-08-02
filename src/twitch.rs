use std::sync::{Arc, Mutex};
use futures_util::StreamExt;
use reqwest::Client;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::{broadcast, Notify};
use tokio::time::{sleep, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};

const EVENTSUB_URL: &str = "wss://eventsub.wss.twitch.tv/ws";

/// Nombre de redemptions gardées en tampon pour un overlay momentanément à la traîne.
const REDEMPTION_BUFFER: usize = 32;

#[derive(Serialize, Clone)]
pub struct RedemptionEvent {
    pub reward_title: String,
    pub user_name: String,
    pub user_input: String,
}

pub struct TwitchState {
    pub total_followers: u64,
    pub last_follower: Option<String>,
    pub connected: bool,
    /// Diffusion des échanges de points de chaîne : chaque overlay connecté reçoit
    /// une copie de chaque événement. Une file drainée n'en servait qu'un seul à la
    /// fois (deux sources ouvertes se répartissaient les redemptions), et elle
    /// grossissait sans fin quand aucun overlay n'était connecté.
    pub redemptions: broadcast::Sender<RedemptionEvent>,
}

impl Default for TwitchState {
    fn default() -> Self {
        Self {
            total_followers: 0,
            last_follower: None,
            connected: false,
            redemptions: broadcast::channel(REDEMPTION_BUFFER).0,
        }
    }
}

pub struct TwitchConfig {
    pub channel_name: String,
    pub client_id: String,
    pub token: String,
}

impl TwitchConfig {
    pub(crate) fn bearer(&self) -> String {
        let t = self.token.strip_prefix("oauth:").unwrap_or(&self.token);
        format!("Bearer {}", t)
    }

    /// Extrait les identifiants Twitch de la configuration courante.
    pub fn from_app(config: &crate::models::config::AppConfig) -> Self {
        Self {
            channel_name: config.twitch_channel_name.clone(),
            client_id: config.twitch_client_id.clone(),
            token: config.twitch_oauth_token.clone(),
        }
    }
}

pub(crate) type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Boucle EventSub.
///
/// Les identifiants sont relus **à chaque tour** plutôt que capturés au démarrage :
/// c'est ce qui permet à un jeton obtenu depuis `/settings` d'être pris en compte
/// sans redémarrer le serveur. `reload` sert à couper la session en cours
/// immédiatement après un enregistrement, au lieu d'attendre qu'elle tombe d'elle-même
/// (ce qui, avec un jeton encore valide, pourrait ne jamais arriver).
pub async fn run(state: Arc<Mutex<TwitchState>>, reload: Arc<Notify>) {
    let client = Client::new();
    loop {
        let config = TwitchConfig::from_app(&crate::models::config::load_config());

        if config.client_id.is_empty() || config.token.is_empty() {
            // Première installation : inutile de marteler Twitch avec un jeton vide,
            // on attend que la page de réglages en fournisse un.
            println!("[Twitch] Aucun jeton configuré — en attente de /settings");
            reload.notified().await;
            continue;
        }

        tokio::select! {
            result = session(&client, &config, &state) => {
                if let Err(e) = result {
                    eprintln!("[Twitch] Erreur: {e}");
                }
                state.lock().unwrap().connected = false;
                sleep(Duration::from_secs(5)).await;
            }
            _ = reload.notified() => {
                println!("[Twitch] Configuration modifiée — reconnexion");
                state.lock().unwrap().connected = false;
            }
        }
    }
}

async fn session(
    client: &Client,
    config: &TwitchConfig,
    state: &Arc<Mutex<TwitchState>>,
) -> Result<(), BoxError> {
    let bid = broadcaster_id(client, config).await?;

    let (total, last) = followers(client, config, &bid).await?;
    {
        let mut g = state.lock().unwrap();
        g.total_followers = total;
        g.last_follower = last;
    }

    let (mut ws, _) = connect_async(EVENTSUB_URL).await?;
    let mut subscribed = false;

    while let Some(msg) = ws.next().await {
        let Message::Text(text) = msg? else { continue };
        let data: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };

        match data["metadata"]["message_type"].as_str().unwrap_or("") {
            "session_welcome" if !subscribed => {
                let sid = data["payload"]["session"]["id"]
                    .as_str()
                    .ok_or("session_id manquant")?
                    .to_string();
                subscribe_follow(client, config, &bid, &sid).await?;
                subscribe_channel_points(client, config, &bid, &sid).await?;
                subscribed = true;
                state.lock().unwrap().connected = true;
                println!("[Twitch] EventSub actif (session: {sid})");
            }
            "notification"
                if data["metadata"]["subscription_type"].as_str()
                    == Some("channel.follow") =>
            {
                let name = data["payload"]["event"]["user_name"]
                    .as_str()
                    .unwrap_or("Inconnu")
                    .to_string();
                let mut g = state.lock().unwrap();
                g.total_followers += 1;
                g.last_follower = Some(name.clone());
                println!("[Twitch] Nouveau follower: {name}");
            }
            "notification"
                if data["metadata"]["subscription_type"].as_str()
                    == Some("channel.channel_points_custom_reward_redemption.add") =>
            {
                let event = &data["payload"]["event"];
                let reward_title = event["reward"]["title"]
                    .as_str()
                    .unwrap_or("Inconnu")
                    .to_string();
                let user_name = event["user_name"]
                    .as_str()
                    .unwrap_or("Inconnu")
                    .to_string();
                let user_input = event["user_input"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                let sender = state.lock().unwrap().redemptions.clone();
                // `send` échoue seulement si aucun overlay n'est connecté : dans ce
                // cas l'événement est simplement abandonné, ce qui est voulu.
                let _ = sender.send(RedemptionEvent {
                    reward_title: reward_title.clone(),
                    user_name: user_name.clone(),
                    user_input: user_input.clone(),
                });
                println!("[Twitch] Point de chaîne: {reward_title} par {user_name}");
            }
            "session_reconnect" => {
                println!("[Twitch] Reconnexion demandée par Twitch");
                break;
            }
            _ => {}
        }
    }

    Ok(())
}

pub(crate) async fn broadcaster_id(
    client: &Client,
    config: &TwitchConfig,
) -> Result<String, BoxError> {
    let resp = client
        .get("https://api.twitch.tv/helix/users")
        .query(&[("login", &config.channel_name)])
        .header("Client-Id", &config.client_id)
        .header("Authorization", config.bearer())
        .send()
        .await?;

    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(
            "Token Twitch invalide ou expiré (HTTP 401) — régénère TWITCH_OAUTH_TOKEN".into(),
        );
    }
    if !status.is_success() {
        return Err(format!("Requête helix/users échouée (HTTP {status})").into());
    }

    let resp: Value = resp.json().await?;
    resp["data"][0]["id"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| format!("Channel '{}' introuvable sur Twitch", config.channel_name).into())
}

/// Nombre total d'abonnés payants.
///
/// Contrairement aux followers, ce compteur n'arrive pas par EventSub : il faut
/// l'interroger. Helix renvoie le total dans `total`, sans qu'on ait à paginer la
/// liste — d'où `first=1`.
///
/// Demande le scope `channel:read:subscriptions`, absent du jeton par défaut du
/// projet : l'erreur est explicite pour que le configurateur puisse l'afficher tel
/// quel plutôt qu'un « échec » opaque.
pub(crate) async fn subscriber_count(
    client: &Client,
    config: &TwitchConfig,
    broadcaster_id: &str,
) -> Result<u64, BoxError> {
    let resp = client
        .get("https://api.twitch.tv/helix/subscriptions")
        .query(&[("broadcaster_id", broadcaster_id), ("first", "1")])
        .header("Client-Id", &config.client_id)
        .header("Authorization", config.bearer())
        .send()
        .await?;

    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(
            "Le jeton n'a pas le scope « channel:read:subscriptions » — régénère \
             TWITCH_OAUTH_TOKEN en l'ajoutant à l'URL d'autorisation"
                .into(),
        );
    }
    if !status.is_success() {
        return Err(format!("Requête helix/subscriptions échouée (HTTP {status})").into());
    }

    let body: Value = resp.json().await?;
    Ok(body["total"].as_u64().unwrap_or(0))
}

async fn followers(
    client: &Client,
    config: &TwitchConfig,
    broadcaster_id: &str,
) -> Result<(u64, Option<String>), BoxError> {
    let resp: Value = client
        .get("https://api.twitch.tv/helix/channels/followers")
        .query(&[("broadcaster_id", broadcaster_id), ("first", "1")])
        .header("Client-Id", &config.client_id)
        .header("Authorization", config.bearer())
        .send()
        .await?
        .json()
        .await?;

    Ok((
        resp["total"].as_u64().unwrap_or(0),
        resp["data"][0]["user_name"].as_str().map(String::from),
    ))
}

async fn subscribe_follow(
    client: &Client,
    config: &TwitchConfig,
    broadcaster_id: &str,
    session_id: &str,
) -> Result<(), BoxError> {
    let resp = client
        .post("https://api.twitch.tv/helix/eventsub/subscriptions")
        .header("Client-Id", &config.client_id)
        .header("Authorization", config.bearer())
        .json(&serde_json::json!({
            "type": "channel.follow",
            "version": "2",
            "condition": {
                "broadcaster_user_id": broadcaster_id,
                "moderator_user_id": broadcaster_id
            },
            "transport": {
                "method": "websocket",
                "session_id": session_id
            }
        }))
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(format!(
            "Souscription EventSub follow échouée ({}): {}",
            resp.status(),
            resp.text().await?
        )
        .into());
    }

    Ok(())
}

async fn subscribe_channel_points(
    client: &Client,
    config: &TwitchConfig,
    broadcaster_id: &str,
    session_id: &str,
) -> Result<(), BoxError> {
    let resp = client
        .post("https://api.twitch.tv/helix/eventsub/subscriptions")
        .header("Client-Id", &config.client_id)
        .header("Authorization", config.bearer())
        .json(&serde_json::json!({
            "type": "channel.channel_points_custom_reward_redemption.add",
            "version": "1",
            "condition": {
                "broadcaster_user_id": broadcaster_id
            },
            "transport": {
                "method": "websocket",
                "session_id": session_id
            }
        }))
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(format!(
            "Souscription EventSub channel_points échouée ({}): {}",
            resp.status(),
            resp.text().await?
        )
        .into());
    }

    println!("[Twitch] Abonné aux points de chaîne");
    Ok(())
}