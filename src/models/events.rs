//! Journal des derniers événements de la chaîne : follows, abonnements, bits, raids.
//!
//! Alimente trois overlays : la carte « Dernier événement » de la bannière, la pluie
//! d'emotes et le cadre caméra. Les alertes (`twitch::AlertEvent`) ne suffisaient
//! pas : un follow n'en est pas une, et la bannière doit retrouver le dernier
//! événement après un redémarrage du serveur — d'où le fichier.

use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Mutex;

use super::channel_point::AlertKind;
use super::fs_atomic;

pub const EVENTS_PATH: &str = "data/events.json";

/// Nombre d'événements gardés. La bannière n'en montre qu'un par type ; vingt
/// laissent de quoi retrouver un raid derrière une rafale de follows.
pub const KEEP: usize = 20;

/// Sérialise lecture-modification-écriture du journal dans ce processus.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum FeedKind {
    Follow,
    Sub,
    Resub,
    Prime,
    Gift,
    Cheer,
    Raid,
    ChannelPoints,
}

impl From<AlertKind> for FeedKind {
    fn from(kind: AlertKind) -> Self {
        match kind {
            AlertKind::ChannelPoints => Self::ChannelPoints,
            AlertKind::Sub => Self::Sub,
            AlertKind::Resub => Self::Resub,
            AlertKind::Prime => Self::Prime,
            AlertKind::Gift => Self::Gift,
            AlertKind::Cheer => Self::Cheer,
            AlertKind::Raid => Self::Raid,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FeedEvent {
    pub kind: FeedKind,
    #[serde(rename = "userName")]
    pub user_name: String,
    /// Même sens que `AlertEvent::amount` : palier, subs offerts, bits ou spectateurs.
    #[serde(default)]
    pub amount: u64,
    #[serde(default)]
    pub months: u64,
    /// Horodatage Unix, en millisecondes.
    #[serde(rename = "atMs", default)]
    pub at_ms: u64,
    /// Événement fabriqué par un bouton « Tester » : joué par les overlays, jamais
    /// journalisé — la bannière afficherait sinon « TestUser » comme dernier sub.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub test: bool,
}

/// Effet déclenché à la main depuis `/effects-config`.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum EffectKind {
    Rain,
    Frame,
}

/// Message diffusé aux overlays connectés à `/api/events_ws`.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum FeedMessage {
    Event { event: FeedEvent },
    EffectTest { effect: EffectKind },
}

/// Ajoute `event` en tête et garde les [`KEEP`] plus récents. Pur, pour être testé
/// sans toucher au disque.
pub fn push(events: &mut Vec<FeedEvent>, event: FeedEvent) {
    events.insert(0, event);
    events.truncate(KEEP);
}

/// Événements journalisés, du plus récent au plus ancien. Un fichier absent ou
/// illisible vaut une liste vide : la bannière saute alors la carte, sans erreur.
pub fn read(path: &str) -> Vec<FeedEvent> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            eprintln!("Error reading {path}: {e}");
            return Vec::new();
        }
    };
    serde_json::from_str(&content).unwrap_or_else(|e| {
        eprintln!("Error parsing {path}: {e}");
        Vec::new()
    })
}

/// Journalise `event` dans `path`.
pub fn record(path: &str, event: &FeedEvent) -> Result<(), String> {
    let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut events = read(path);
    push(&mut events, event.clone());
    let json = serde_json::to_string_pretty(&events)
        .map_err(|e| format!("Error serializing events: {e}"))?;
    fs_atomic::write(path, &json)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: FeedKind, user: &str) -> FeedEvent {
        FeedEvent {
            kind,
            user_name: user.to_string(),
            amount: 0,
            months: 0,
            at_ms: 0,
            test: false,
        }
    }

    #[test]
    fn le_plus_recent_passe_en_tete_et_la_liste_reste_bornee() {
        let mut events = Vec::new();
        for i in 0..KEEP + 5 {
            push(&mut events, event(FeedKind::Follow, &format!("viewer{i}")));
        }
        assert_eq!(events.len(), KEEP);
        assert_eq!(events[0].user_name, format!("viewer{}", KEEP + 4));
    }

    #[test]
    fn un_evenement_reel_n_ecrit_pas_le_drapeau_test() {
        let json = serde_json::to_string(&event(FeedKind::Raid, "Ronni")).unwrap();
        assert!(!json.contains("test"), "{json}");
        assert!(json.contains(r#""kind":"raid""#));
        assert!(json.contains(r#""userName":"Ronni""#));
    }

    #[test]
    fn les_messages_portent_leur_type_pour_l_overlay() {
        let json = serde_json::to_string(&FeedMessage::EffectTest { effect: EffectKind::Rain })
            .unwrap();
        assert_eq!(json, r#"{"type":"effect-test","effect":"rain"}"#);

        let json = serde_json::to_string(&FeedMessage::Event {
            event: event(FeedKind::Cheer, "Ronni"),
        })
        .unwrap();
        assert!(json.starts_with(r#"{"type":"event","event":{"kind":"cheer""#), "{json}");
    }

    #[test]
    fn journaliser_ajoute_en_tete_du_fichier() {
        // Dossier propre au test : jamais `data/`, qui porte la vraie configuration.
        let dir = std::env::temp_dir().join(format!("praetorcast-events-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("events.json");
        let path = path.to_str().unwrap();

        record(path, &event(FeedKind::Follow, "premier")).unwrap();
        record(path, &event(FeedKind::Sub, "second")).unwrap();

        let events = read(path);
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].user_name, "second");
        assert_eq!(events[1].kind, FeedKind::Follow);
    }

    #[test]
    fn un_fichier_absent_vaut_une_liste_vide() {
        assert!(read("n-existe-pas/events.json").is_empty());
    }
}
