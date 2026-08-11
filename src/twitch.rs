use crate::models::channel_point::AlertKind;
use std::sync::{Arc, Mutex};
use futures_util::StreamExt;
use reqwest::Client;
use serde::Serialize;
use serde_json::{json, Value};
use tokio::sync::{broadcast, Notify};
use tokio::time::{sleep, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};

const EVENTSUB_URL: &str = "wss://eventsub.wss.twitch.tv/ws";

/// Nombre d'alertes gardées en tampon pour un consommateur momentanément à la traîne.
const ALERT_BUFFER: usize = 32;

/// Événement digne d'une alerte à l'écran.
///
/// Un seul type pour tous les `kind` plutôt qu'une énumération de charges utiles :
/// l'overlay et la tâche subathon ne consomment que `kind` et `amount`, et le reste
/// n'est là que pour remplir les jetons de phrase.
#[derive(Serialize, Clone, Debug)]
pub struct AlertEvent {
    pub kind: AlertKind,
    /// Rempli seulement pour les points de chaîne, où il est le discriminant.
    pub reward_title: String,
    pub user_name: String,
    /// Message accompagnant un réabonnement ou un cheer, sinon vide.
    pub user_input: String,
    /// Palier pour un abonnement, nombre de subs offerts, de bits ou de
    /// spectateurs selon le `kind`. Voir `AlertKind`.
    pub amount: u64,
    /// Mois cumulés, réabonnements uniquement.
    pub months: u64,
}

pub struct TwitchState {
    pub total_followers: u64,
    pub last_follower: Option<String>,
    pub connected: bool,
    /// Stream en ligne, d'après `stream.online` / `stream.offline`. Faux au démarrage
    /// tant qu'aucune notification n'est arrivée : le serveur peut être lancé en
    /// cours de direct, l'état ne devient donc fiable qu'au premier basculement.
    pub live: bool,
    /// Début du direct, tel que Twitch l'envoie (RFC 3339). Gardé en chaîne : le seul
    /// consommateur est du JavaScript, où `Date.parse` le lit nativement, alors
    /// qu'une conversion ici imposerait une dépendance de date.
    pub stream_started_at: Option<String>,
    /// Diffusion des alertes : chaque abonné (overlay connecté, tâche subathon)
    /// reçoit une copie de chaque événement. Une file drainée n'en servait qu'un
    /// seul à la fois (deux sources ouvertes se répartissaient les redemptions), et
    /// elle grossissait sans fin quand personne n'écoutait.
    pub alerts: broadcast::Sender<AlertEvent>,
}

impl Default for TwitchState {
    fn default() -> Self {
        Self {
            total_followers: 0,
            last_follower: None,
            connected: false,
            live: false,
            stream_started_at: None,
            alerts: broadcast::channel(ALERT_BUFFER).0,
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
                subscribe_all(client, config, &bid, &sid).await?;
                subscribed = true;
                state.lock().unwrap().connected = true;
                println!("[Twitch] EventSub actif (session: {sid})");
            }
            // Un bras par type serait sept bras gardés dans le `match` extérieur :
            // on descend d'un niveau plutôt que d'empiler les gardes.
            "notification" => {
                let kind = data["metadata"]["subscription_type"].as_str().unwrap_or("");
                let event = &data["payload"]["event"];

                match kind {
                    "channel.follow" => {
                        let name = event["user_name"].as_str().unwrap_or("Inconnu").to_string();
                        let mut g = state.lock().unwrap();
                        g.total_followers += 1;
                        g.last_follower = Some(name.clone());
                        println!("[Twitch] Nouveau follower: {name}");
                    }
                    "stream.online" => {
                        let started = event["started_at"].as_str().map(str::to_string);
                        let mut g = state.lock().unwrap();
                        g.live = true;
                        g.stream_started_at = started;
                        println!("[Twitch] Stream en ligne");
                    }
                    "stream.offline" => {
                        let mut g = state.lock().unwrap();
                        g.live = false;
                        g.stream_started_at = None;
                        println!("[Twitch] Stream hors ligne");
                    }
                    _ => {
                        if let Some(alert) = alert_from(kind, event) {
                            // Le `Sender` est cloné hors du verrou : `send` est un
                            // point d'attente potentiel et le garde ne doit jamais
                            // traverser un `.await`.
                            let sender = state.lock().unwrap().alerts.clone();
                            println!(
                                "[Twitch] Alerte {:?} : {} ({})",
                                alert.kind, alert.user_name, alert.amount
                            );
                            // `send` n'échoue que sans aucun abonné. Depuis que la
                            // tâche subathon en est un permanent, ça ne se produit
                            // plus qu'au tout début du démarrage.
                            let _ = sender.send(alert);
                        }
                    }
                }
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

/// Palier d'abonnement en valeur numérique.
///
/// EventSub envoie `"1000"` / `"2000"` / `"3000"` en **chaîne**. Toute autre forme est
/// ramenée au premier palier plutôt que laissée à 0, sinon aucune ligne configurée ne
/// la rattraperait — `min_amount: 0` mis à part. C'est le cas d'un Prime, que Twitch
/// classe en tier 1 mais dont il n'est pas garanti qu'il porte toujours `"1000"`.
fn tier_of(sub: &Value) -> u64 {
    match sub["sub_tier"].as_str() {
        Some(t) => t.parse().unwrap_or(1000),
        None => sub["sub_tier"].as_u64().unwrap_or(1000),
    }
}

/// Type d'alerte d'un abonnement, Prime ou payant.
///
/// `is_prime` est la seule raison d'être de `channel.chat.notification` dans ce
/// module : `channel.subscribe` annonçait un Prime comme un tier 1000 quelconque,
/// donc indiscernable d'un abonnement payant.
fn sub_kind(sub: &Value, paid: AlertKind) -> AlertKind {
    if sub["is_prime"].as_bool().unwrap_or(false) {
        AlertKind::Prime
    } else {
        paid
    }
}

/// Traduit une notification EventSub en alerte, ou `None` si le type ne nous
/// intéresse pas.
///
/// Tout est lu en `Value` plutôt qu'en structures serde, comme le reste du module :
/// les charges utiles diffèrent d'un type à l'autre et la plupart des champs sont
/// optionnels, une structure par type coûterait plus qu'elle ne protégerait.
fn alert_from(subscription_type: &str, event: &Value) -> Option<AlertEvent> {
    let text = |key: &str| event[key].as_str().unwrap_or("").to_string();
    // Un cheer ou un don anonyme n'a pas de `user_name`.
    let user = |key: &str| match event[key].as_str() {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => "Anonyme".to_string(),
    };

    let (kind, user_name, user_input, amount, months) = match subscription_type {
        // Source unique de tous les abonnements — premier, renouvellement et don.
        // C'est le seul type EventSub qui expose `is_prime` : `channel.subscribe`
        // annonçait un Prime comme un tier 1000 quelconque. Le prendre pour les dons
        // aussi évite d'avoir à dédoublonner entre deux souscriptions concurrentes.
        "channel.chat.notification" => {
            // Un don anonyme n'a pas de `chatter_user_name` : `user` le replie déjà
            // sur « Anonyme », `chatter_is_anonymous` n'a donc rien à ajouter.
            let chatter = user("chatter_user_name");
            match event["notice_type"].as_str().unwrap_or("") {
                "sub" => {
                    let sub = &event["sub"];
                    (sub_kind(sub, AlertKind::Sub), chatter, String::new(), tier_of(sub), 0)
                }
                "resub" => {
                    let resub = &event["resub"];
                    (
                        sub_kind(resub, AlertKind::Resub),
                        chatter,
                        event["message"]["text"].as_str().unwrap_or("").to_string(),
                        tier_of(resub),
                        resub["cumulative_months"].as_u64().unwrap_or(0),
                    )
                }
                "community_sub_gift" => (
                    AlertKind::Gift,
                    chatter,
                    String::new(),
                    // Un don sans `total` reste un don : on compte 1 plutôt que 0,
                    // sinon le subathon n'ajouterait rien.
                    event["community_sub_gift"]["total"].as_u64().unwrap_or(1),
                    0,
                ),
                "sub_gift" => {
                    // Un don groupé émet le récapitulatif `community_sub_gift`
                    // **puis** un `sub_gift` par bénéficiaire, tous porteurs du même
                    // `community_gift_id`. Les compter tous les deux doublerait les
                    // alertes et le temps du subathon.
                    if !event["sub_gift"]["community_gift_id"].is_null() {
                        return None;
                    }
                    (AlertKind::Gift, chatter, String::new(), 1, 0)
                }
                // Annonces de modération, raids relayés dans le chat, badges de
                // bits… : tout ce que Twitch pousse aussi sur ce type et qui a déjà
                // sa propre souscription, ou n'a pas d'alerte associée.
                _ => return None,
            }
        }
        "channel.cheer" => (
            AlertKind::Cheer,
            user("user_name"),
            text("message"),
            event["bits"].as_u64().unwrap_or(0),
            0,
        ),
        "channel.raid" => (
            AlertKind::Raid,
            user("from_broadcaster_user_name"),
            String::new(),
            event["viewers"].as_u64().unwrap_or(0),
            0,
        ),
        "channel.channel_points_custom_reward_redemption.add" => {
            return Some(AlertEvent {
                kind: AlertKind::ChannelPoints,
                reward_title: event["reward"]["title"]
                    .as_str()
                    .unwrap_or("Inconnu")
                    .to_string(),
                user_name: user("user_name"),
                user_input: text("user_input"),
                amount: 0,
                months: 0,
            })
        }
        _ => return None,
    };

    Some(AlertEvent {
        kind,
        reward_title: String::new(),
        user_name,
        user_input,
        amount,
        months,
    })
}

/// Souscrit à tout ce dont les overlays ont besoin.
///
/// Deux régimes volontairement différents. Les follows et les points de chaîne sont
/// indispensables et autorisés de longue date : leur échec propage et coupe la
/// session, ce qui rend le problème visible. Les alertes d'événements **loguent et
/// continuent** — un `bits:read` pas encore accordé refuse `channel.cheer`, un
/// `user:read:chat` manquant refuse `channel.chat.notification`, et avec un `?` ce
/// refus emporterait follows et points de chaîne avec lui, en rebouclant toutes les
/// 5 secondes. Le corps de la réponse Twitch est inclus dans l'erreur, donc le log
/// explique de lui-même quel droit manque.
async fn subscribe_all(
    client: &Client,
    config: &TwitchConfig,
    broadcaster_id: &str,
    session_id: &str,
) -> Result<(), BoxError> {
    let own = json!({ "broadcaster_user_id": broadcaster_id });

    subscribe(
        client,
        config,
        session_id,
        "channel.follow",
        "2",
        json!({
            "broadcaster_user_id": broadcaster_id,
            "moderator_user_id": broadcaster_id
        }),
    )
    .await?;
    subscribe(
        client,
        config,
        session_id,
        "channel.channel_points_custom_reward_redemption.add",
        "1",
        own.clone(),
    )
    .await?;

    for (kind, version, condition) in [
        // Couvre à lui seul les trois anciennes souscriptions d'abonnement
        // (`channel.subscribe`, `.subscription.message`, `.subscription.gift`) et,
        // contrairement à elles, distingue un Prime d'un tier 1 payant. Sa condition
        // demande un `user_id` en plus : celui du compte qui *lit* le chat, ici le
        // diffuseur lui-même, dont le jeton porte `user:read:chat`.
        (
            "channel.chat.notification",
            "1",
            json!({ "broadcaster_user_id": broadcaster_id, "user_id": broadcaster_id }),
        ),
        ("channel.cheer", "1", own.clone()),
        // Seul type dont la condition n'est pas `broadcaster_user_id` : un raid se
        // filtre sur sa destination. C'est pour lui que la condition est passée
        // telle quelle au lieu d'être construite ici.
        ("channel.raid", "1", json!({ "to_broadcaster_user_id": broadcaster_id })),
        ("stream.online", "1", own.clone()),
        ("stream.offline", "1", own.clone()),
    ] {
        if let Err(e) = subscribe(client, config, session_id, kind, version, condition).await {
            eprintln!("[Twitch] {kind} indisponible : {e}");
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

/// Crée une souscription EventSub sur la session WebSocket en cours.
///
/// La `condition` est passée telle quelle et non construite depuis le
/// `broadcaster_id` : `channel.raid` se filtre sur `to_broadcaster_user_id`, la
/// symétrie des autres types ne tient donc pas. La `version` est une **chaîne** —
/// Twitch refuse un entier.
async fn subscribe(
    client: &Client,
    config: &TwitchConfig,
    session_id: &str,
    kind: &str,
    version: &str,
    condition: Value,
) -> Result<(), BoxError> {
    let resp = client
        .post("https://api.twitch.tv/helix/eventsub/subscriptions")
        .header("Client-Id", &config.client_id)
        .header("Authorization", config.bearer())
        .json(&json!({
            "type": kind,
            "version": version,
            "condition": condition,
            "transport": {
                "method": "websocket",
                "session_id": session_id
            }
        }))
        .send()
        .await?;

    if !resp.status().is_success() {
        // Le corps est conservé : c'est lui qui nomme le droit manquant, et il
        // remonte tel quel dans le log de `subscribe_all`.
        return Err(format!(
            "souscription refusée ({}): {}",
            resp.status(),
            resp.text().await?
        )
        .into());
    }

    println!("[Twitch] Abonné à {kind}");
    Ok(())
}

/// Les charges utiles de `channel.chat.notification` sont le seul endroit du projet
/// où une erreur de nom de champ ne se voit qu'en direct, sur un vrai abonnement :
/// une faute de frappe donnerait un `Sub` silencieux au lieu d'un `Prime`, sans rien
/// dans les logs. D'où ces cas, calqués sur les exemples de la documentation Twitch.
#[cfg(test)]
mod tests {
    use super::*;

    fn notification(notice_type: &str, extra: Value) -> Value {
        let mut event = json!({
            "chatter_user_name": "Ronni",
            "chatter_is_anonymous": false,
            "notice_type": notice_type,
            "message": { "text": "" },
        });
        for (key, value) in extra.as_object().unwrap() {
            event[key] = value.clone();
        }
        event
    }

    fn alert_of(notice_type: &str, extra: Value) -> Option<AlertEvent> {
        alert_from("channel.chat.notification", &notification(notice_type, extra))
    }

    #[test]
    fn un_premier_abonnement_prime_est_un_prime() {
        let alert = alert_of(
            "sub",
            json!({ "sub": { "sub_tier": "1000", "is_prime": true, "duration_months": 1 } }),
        )
        .unwrap();

        assert_eq!(alert.kind, AlertKind::Prime);
        assert_eq!(alert.user_name, "Ronni");
        assert_eq!(alert.amount, 1000);
        // Ce qui distingue un premier abonnement d'un renouvellement, et ce sur quoi
        // `channel_point::select` choisit son repli.
        assert_eq!(alert.months, 0);
    }

    #[test]
    fn un_premier_abonnement_payant_reste_un_sub_avec_son_palier() {
        let alert = alert_of(
            "sub",
            json!({ "sub": { "sub_tier": "3000", "is_prime": false, "duration_months": 1 } }),
        )
        .unwrap();

        assert_eq!(alert.kind, AlertKind::Sub);
        assert_eq!(alert.amount, 3000);
    }

    #[test]
    fn un_renouvellement_prime_est_un_prime_avec_ses_mois() {
        let alert = alert_of(
            "resub",
            json!({
                "resub": {
                    "sub_tier": "1000",
                    "is_prime": true,
                    "cumulative_months": 12,
                    "duration_months": 1
                }
            }),
        )
        .unwrap();

        assert_eq!(alert.kind, AlertKind::Prime);
        assert_eq!(alert.months, 12);
    }

    #[test]
    fn un_renouvellement_payant_reste_un_resub() {
        let alert = alert_of(
            "resub",
            json!({
                "resub": { "sub_tier": "2000", "is_prime": false, "cumulative_months": 4 }
            }),
        )
        .unwrap();

        assert_eq!(alert.kind, AlertKind::Resub);
        assert_eq!(alert.amount, 2000);
        assert_eq!(alert.months, 4);
    }

    #[test]
    fn le_message_d_un_renouvellement_remplit_le_jeton_de_phrase() {
        let mut event = notification(
            "resub",
            json!({ "resub": { "sub_tier": "1000", "cumulative_months": 3 } }),
        );
        event["message"]["text"] = json!("merci pour tout !");

        let alert = alert_from("channel.chat.notification", &event).unwrap();
        assert_eq!(alert.user_input, "merci pour tout !");
    }

    #[test]
    fn un_don_groupe_compte_ses_beneficiaires_une_seule_fois() {
        // Le récapitulatif porte le total…
        let alert = alert_of(
            "community_sub_gift",
            json!({ "community_sub_gift": { "id": "1", "total": 5, "sub_tier": "1000" } }),
        )
        .unwrap();
        assert_eq!(alert.kind, AlertKind::Gift);
        assert_eq!(alert.amount, 5);

        // …et les cinq notifications individuelles qui le suivent sont ignorées.
        assert!(alert_of(
            "sub_gift",
            json!({ "sub_gift": { "sub_tier": "1000", "community_gift_id": "1" } })
        )
        .is_none());
    }

    #[test]
    fn un_don_isole_compte_pour_un() {
        // Sans `community_gift_id`, il n'y a aucun récapitulatif pour le porter :
        // l'ignorer ferait disparaître le don.
        let alert = alert_of("sub_gift", json!({ "sub_gift": { "sub_tier": "1000" } })).unwrap();

        assert_eq!(alert.kind, AlertKind::Gift);
        assert_eq!(alert.amount, 1);
    }

    #[test]
    fn un_don_anonyme_a_quand_meme_un_nom_affichable() {
        let mut event = notification("sub_gift", json!({ "sub_gift": { "sub_tier": "1000" } }));
        event["chatter_user_name"] = json!("");
        event["chatter_is_anonymous"] = json!(true);

        assert_eq!(
            alert_from("channel.chat.notification", &event).unwrap().user_name,
            "Anonyme"
        );
    }

    #[test]
    fn les_autres_notifications_de_chat_ne_declenchent_rien() {
        // Le type transporte aussi les annonces, les raids et les badges de bits.
        // Les raids ont leur propre souscription : les laisser passer ici les
        // doublerait.
        assert!(alert_of("announcement", json!({})).is_none());
        assert!(alert_of("raid", json!({ "raid": { "viewer_count": 30 } })).is_none());
        assert!(alert_of("", json!({})).is_none());
    }
}