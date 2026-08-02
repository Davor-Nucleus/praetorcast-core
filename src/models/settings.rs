//! Liste blanche des réglages éditables depuis `/settings`.
//!
//! `env.json` contient plus de clés que ce formulaire n'en expose, pour deux
//! raisons opposées :
//!
//! - `VOLUME` et `normalizationEnabled` sont de l'**état du lecteur**, pas des
//!   réglages : JanusCore et PhonosCore les réécrivent à chaque mouvement de
//!   curseur. Les mettre dans un formulaire qu'on enregistre d'un bloc ferait
//!   revenir en arrière le volume réglé entre-temps.
//! - `TWITCH_TOKEN_EXPIRES_AT` est écrite par le bouton « Connecter Twitch »
//!   (`controllers::auth_controller`) ; l'éditer à la main ne peut que
//!   désynchroniser le jeton et sa date d'expiration.
//!
//! Toute clé absente de [`FIELDS`] est refusée à l'enregistrement, ce qui évite
//! qu'un POST malformé n'injecte n'importe quoi dans le fichier partagé.

use serde_json::{Map, Value};

/// Valeur renvoyée à la place d'un secret.
///
/// Les secrets ne quittent jamais le serveur : la page reçoit ce sentinelle, et un
/// POST qui le renvoie tel quel signifie « inchangé ». Sans ça, ouvrir `/settings`
/// suffirait à exposer le jeton Twitch dans le HTML de la page, exactement le
/// problème qu'on avait retiré des overlays de chat.
pub const MASK: &str = "••••••••";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// Port TCP : entier 1–65535, unique parmi les ports.
    Port,
    /// Chaîne libre.
    Text,
    /// Chaîne masquée à la lecture.
    Secret,
    /// Booléen.
    Bool,
    /// Entier positif quelconque.
    Number,
    /// Table `{"1": "alt+1", ...}` des raccourcis soundboard.
    Shortcuts,
}

/// Service qu'il faut redémarrer pour qu'un changement prenne effet.
///
/// `HOT` signifie que la valeur est relue à chaque requête côté core : le
/// rechargement de `models::config` suffit.
pub const HOT: &str = "";

pub struct FieldSpec {
    pub key: &'static str,
    pub kind: FieldKind,
    /// Service à redémarrer, ou [`HOT`].
    pub service: &'static str,
    /// Le champ peut-il être vide ?
    pub optional: bool,
}

const fn field(key: &'static str, kind: FieldKind, service: &'static str) -> FieldSpec {
    FieldSpec { key, kind, service, optional: true }
}

const fn required(key: &'static str, kind: FieldKind, service: &'static str) -> FieldSpec {
    FieldSpec { key, kind, service, optional: false }
}

pub const FIELDS: &[FieldSpec] = &[
    // Twitch — relu à chaque requête, et la boucle EventSub est réveillée après
    // l'enregistrement (cf. `twitch::run`).
    required("TWITCH_CHANNEL_NAME", FieldKind::Text, HOT),
    required("TWITCH_CLIENT_ID", FieldKind::Text, HOT),
    // Reste éditable pour qui a déjà un jeton sous la main ; le chemin normal est
    // le bouton « Connecter Twitch », qui l'écrit lui-même.
    field("TWITCH_OAUTH_TOKEN", FieldKind::Secret, HOT),
    // Apparence
    required("FRONT_FONT_TITLE", FieldKind::Text, HOT),
    field("SOUNDBOARD_SHORTCUTS", FieldKind::Shortcuts, HOT),
    // Ports — la socket est déjà liée quand le serveur tourne, et les autres
    // services ne lisent env.json qu'au démarrage.
    required("PORT", FieldKind::Port, "Core"),
    required("PORT_MUSIC", FieldKind::Port, "Core, Janus et Phonos"),
    required("PORT_SOUNDBOARD", FieldKind::Port, "Phonos"),
    required("PORT_WS_YOUTUBE_CHAT", FieldKind::Port, "YT-Chat"),
    required("PORT_WS_DISCORD_PRESENCE", FieldKind::Port, "Discord"),
    // OBS — `obs_controller` reconstruit sa connexion à chaque requête.
    field("OBS_WS_HOST", FieldKind::Text, HOT),
    field("OBS_WS_PORT", FieldKind::Port, HOT),
    field("OBS_WS_PASSWORD", FieldKind::Secret, HOT),
    field("OBS_AUDIO_SOURCE", FieldKind::Text, HOT),
    field("OBS_LIMITER_FILTER", FieldKind::Text, HOT),
    // Services externes
    field("YOUTUBE_CHANNEL_ID", FieldKind::Text, "YT-Chat"),
    field("YOUTUBE_CHAT_RETRY_DELAY_MS", FieldKind::Number, "YT-Chat"),
    field("DISCORD_CLIENT_ID", FieldKind::Text, "Discord"),
    field("DISCORD_CLIENT_SECRET", FieldKind::Secret, "Discord"),
    field("janusCoreGui", FieldKind::Bool, "Janus"),
    field("phonosCoreGui", FieldKind::Bool, "Phonos"),
];

/// Les clés qui doivent rester uniques entre elles : deux services sur le même port
/// donnent une erreur « adresse déjà utilisée » incompréhensible au démarrage.
const PORT_KEYS: &[&str] = &[
    "PORT",
    "PORT_MUSIC",
    "PORT_SOUNDBOARD",
    "PORT_WS_YOUTUBE_CHAT",
    "PORT_WS_DISCORD_PRESENCE",
];

pub fn spec(key: &str) -> Option<&'static FieldSpec> {
    FIELDS.iter().find(|f| f.key == key)
}

/// Charge utile envoyée à la page : la valeur de chaque champ connu, secrets
/// masqués, et la métadonnée qui permet d'afficher « redémarrer Janus ».
pub fn view(raw: &Map<String, Value>) -> Value {
    let mut values = Map::new();
    let mut meta = Map::new();

    for field in FIELDS {
        let stored = raw.get(field.key);
        let value = match field.kind {
            FieldKind::Secret => match stored.and_then(Value::as_str) {
                Some(s) if !s.is_empty() => Value::String(MASK.to_string()),
                _ => Value::String(String::new()),
            },
            _ => stored.cloned().unwrap_or(Value::Null),
        };
        values.insert(field.key.to_string(), value);
        meta.insert(
            field.key.to_string(),
            serde_json::json!({
                "kind": kind_name(field.kind),
                "service": field.service,
                "optional": field.optional,
            }),
        );
    }

    serde_json::json!({ "values": values, "meta": meta })
}

fn kind_name(kind: FieldKind) -> &'static str {
    match kind {
        FieldKind::Port => "port",
        FieldKind::Text => "text",
        FieldKind::Secret => "secret",
        FieldKind::Bool => "bool",
        FieldKind::Number => "number",
        FieldKind::Shortcuts => "shortcuts",
    }
}

/// Convertit et valide un formulaire soumis.
///
/// `current` est l'`env.json` déjà en place : il sert à vérifier l'unicité des ports
/// sur la configuration **résultante** et non sur les seuls champs modifiés.
///
/// Renvoie les clés à écrire — les secrets laissés au sentinelle et les valeurs
/// identiques à l'existant en sont retirés, pour n'écrire que ce qui change.
pub fn sanitize(
    submitted: &Map<String, Value>,
    current: &Map<String, Value>,
) -> Result<Map<String, Value>, String> {
    let mut changes = Map::new();

    for (key, raw) in submitted {
        let Some(field) = spec(key) else {
            return Err(format!("Réglage inconnu : « {key} »"));
        };

        // Secret laissé masqué → l'utilisateur n'y a pas touché.
        if field.kind == FieldKind::Secret && raw.as_str() == Some(MASK) {
            continue;
        }

        let value = coerce(field, raw)?;
        if current.get(key) != Some(&value) {
            changes.insert(key.clone(), value);
        }
    }

    check_unique_ports(&changes, current)?;
    Ok(changes)
}

fn coerce(field: &FieldSpec, raw: &Value) -> Result<Value, String> {
    let key = field.key;
    match field.kind {
        FieldKind::Port => {
            let n = as_u64(raw).ok_or_else(|| format!("« {key} » doit être un nombre"))?;
            if !(1..=65535).contains(&n) {
                return Err(format!("« {key} » doit être compris entre 1 et 65535"));
            }
            Ok(Value::from(n))
        }
        FieldKind::Number => {
            let n = as_u64(raw).ok_or_else(|| format!("« {key} » doit être un nombre"))?;
            Ok(Value::from(n))
        }
        FieldKind::Bool => match raw {
            Value::Bool(b) => Ok(Value::Bool(*b)),
            Value::String(s) if s == "true" || s == "false" => Ok(Value::Bool(s == "true")),
            _ => Err(format!("« {key} » doit être vrai ou faux")),
        },
        FieldKind::Text | FieldKind::Secret => {
            let s = raw
                .as_str()
                .ok_or_else(|| format!("« {key} » doit être du texte"))?
                .trim();
            if s.is_empty() && !field.optional {
                return Err(format!("« {key} » ne peut pas être vide"));
            }
            Ok(Value::String(s.to_string()))
        }
        FieldKind::Shortcuts => {
            let map = raw
                .as_object()
                .ok_or_else(|| format!("« {key} » doit être une table de raccourcis"))?;
            if map.values().any(|v| !v.is_string()) {
                return Err(format!("« {key} » : chaque raccourci doit être du texte"));
            }
            Ok(Value::Object(map.clone()))
        }
    }
}

/// Accepte aussi bien `3000` que `"3000"` : un `<input type="number">` renvoie une
/// chaîne dès qu'on lit sa valeur en JS sans conversion.
fn as_u64(raw: &Value) -> Option<u64> {
    match raw {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn check_unique_ports(
    changes: &Map<String, Value>,
    current: &Map<String, Value>,
) -> Result<(), String> {
    let mut seen: Vec<(&str, u64)> = Vec::new();
    for key in PORT_KEYS {
        let value = changes.get(*key).or_else(|| current.get(*key));
        let Some(port) = value.and_then(as_u64) else { continue };
        if let Some((other, _)) = seen.iter().find(|(_, p)| *p == port) {
            return Err(format!(
                "Le port {port} est déjà utilisé par « {other} » — chaque service doit avoir le sien"
            ));
        }
        seen.push((key, port));
    }
    Ok(())
}

/// Services à redémarrer pour que `changes` prenne effet, sans doublon.
pub fn impacted_services(changes: &Map<String, Value>) -> Vec<String> {
    let mut services: Vec<String> = Vec::new();
    for key in changes.keys() {
        let Some(field) = spec(key) else { continue };
        if field.service != HOT && !services.iter().any(|s| s == field.service) {
            services.push(field.service.to_string());
        }
    }
    services
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(json: &str) -> Map<String, Value> {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn les_secrets_sont_masques_a_la_lecture() {
        let raw = map(r#"{"TWITCH_OAUTH_TOKEN": "vrai-jeton", "TWITCH_CLIENT_ID": "abc"}"#);
        let view = view(&raw);
        assert_eq!(view["values"]["TWITCH_OAUTH_TOKEN"], MASK);
        assert_eq!(view["values"]["TWITCH_CLIENT_ID"], "abc");
    }

    #[test]
    fn un_secret_vide_reste_vide_et_non_masque() {
        // Sinon la page afficherait des points sur un champ jamais renseigné.
        let raw = map(r#"{"OBS_WS_PASSWORD": ""}"#);
        assert_eq!(view(&raw)["values"]["OBS_WS_PASSWORD"], "");
    }

    #[test]
    fn le_sentinelle_ne_reecrit_pas_le_secret() {
        let current = map(r#"{"TWITCH_OAUTH_TOKEN": "vrai-jeton"}"#);
        let submitted = map(&format!(r#"{{"TWITCH_OAUTH_TOKEN": "{MASK}"}}"#));
        assert!(sanitize(&submitted, &current).unwrap().is_empty());
    }

    #[test]
    fn seules_les_valeurs_modifiees_sont_ecrites() {
        let current = map(r#"{"PORT": 3000, "OBS_AUDIO_SOURCE": "music"}"#);
        let submitted = map(r#"{"PORT": 3000, "OBS_AUDIO_SOURCE": "micro"}"#);
        let changes = sanitize(&submitted, &current).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes["OBS_AUDIO_SOURCE"], "micro");
    }

    #[test]
    fn une_cle_inconnue_est_refusee() {
        let err = sanitize(&map(r#"{"VOLUME": 0.5}"#), &Map::new()).unwrap_err();
        assert!(err.contains("VOLUME"));
    }

    #[test]
    fn un_port_hors_bornes_est_refuse() {
        let err = sanitize(&map(r#"{"PORT": 70000}"#), &Map::new()).unwrap_err();
        assert!(err.contains("65535"));
    }

    #[test]
    fn un_port_en_chaine_est_accepte() {
        let changes = sanitize(&map(r#"{"PORT": "3010"}"#), &Map::new()).unwrap();
        assert_eq!(changes["PORT"], 3010);
    }

    #[test]
    fn deux_services_ne_peuvent_pas_partager_un_port() {
        let current = map(r#"{"PORT": 3000, "PORT_MUSIC": 3001}"#);
        let err = sanitize(&map(r#"{"PORT_MUSIC": 3000}"#), &current).unwrap_err();
        assert!(err.contains("3000"));
    }

    #[test]
    fn un_champ_obligatoire_ne_peut_pas_etre_vide() {
        let err = sanitize(&map(r#"{"TWITCH_CHANNEL_NAME": "   "}"#), &Map::new()).unwrap_err();
        assert!(err.contains("TWITCH_CHANNEL_NAME"));
    }

    #[test]
    fn les_services_impactes_sont_dedupliques() {
        let changes = map(r#"{"PORT_SOUNDBOARD": 3002, "phonosCoreGui": false, "OBS_WS_HOST": "x"}"#);
        assert_eq!(impacted_services(&changes), vec!["Phonos".to_string()]);
    }

    #[test]
    fn un_reglage_a_chaud_n_impose_aucun_redemarrage() {
        let changes = map(r#"{"OBS_AUDIO_SOURCE": "micro"}"#);
        assert!(impacted_services(&changes).is_empty());
    }
}
