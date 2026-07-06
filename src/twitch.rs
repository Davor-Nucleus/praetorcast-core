use std::sync::{Arc, Mutex};
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::Value;
use tokio::time::{sleep, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};

const EVENTSUB_URL: &str = "wss://eventsub.wss.twitch.tv/ws";

#[derive(Default)]
pub struct TwitchState {
    pub total_followers: u64,
    pub last_follower: Option<String>,
    pub connected: bool,
}

pub struct TwitchConfig {
    pub channel_name: String,
    pub client_id: String,
    pub token: String,
}

impl TwitchConfig {
    fn bearer(&self) -> String {
        let t = self.token.strip_prefix("oauth:").unwrap_or(&self.token);
        format!("Bearer {}", t)
    }
}

type BoxError = Box<dyn std::error::Error + Send + Sync>;

pub async fn run(state: Arc<Mutex<TwitchState>>, config: TwitchConfig) {
    let client = Client::new();
    loop {
        if let Err(e) = session(&client, &config, &state).await {
            eprintln!("[Twitch] Erreur: {e}");
        }
        state.lock().unwrap().connected = false;
        sleep(Duration::from_secs(5)).await;
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
                subscribe(client, config, &bid, &sid).await?;
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
            "session_reconnect" => {
                println!("[Twitch] Reconnexion demandée par Twitch");
                break;
            }
            _ => {}
        }
    }

    Ok(())
}

async fn broadcaster_id(client: &Client, config: &TwitchConfig) -> Result<String, BoxError> {
    let resp: Value = client
        .get("https://api.twitch.tv/helix/users")
        .query(&[("login", &config.channel_name)])
        .header("Client-Id", &config.client_id)
        .header("Authorization", config.bearer())
        .send()
        .await?
        .json()
        .await?;

    resp["data"][0]["id"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| format!("Channel '{}' introuvable sur Twitch", config.channel_name).into())
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

async fn subscribe(
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
            "Souscription EventSub échouée ({}): {}",
            resp.status(),
            resp.text().await?
        )
        .into());
    }

    Ok(())
}
