//! Alertes à l'écran : points de chaîne, abonnements, bits, raids.
//!
//! Le fichier s'appelle toujours `channel_points.json` et la route toujours
//! `/channel-points` : les alertes d'événements sont venues se greffer sur le moteur
//! des points de chaîne (file d'attente, watchdog audio, transitions) plutôt que
//! d'ouvrir un second overlay, donc renommer aurait cassé les sources OBS existantes
//! sans rien apporter.

use serde::{Deserialize, Serialize};
use std::fs;

use super::fs_atomic;

/// Ce qui déclenche une alerte.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum AlertKind {
    /// Échange de points de chaîne, discriminé par le titre exact de la récompense.
    /// Valeur par défaut : un `channel_points.json` écrit avant l'arrivée des autres
    /// types n'a pas de clé `kind` et doit rester une liste de récompenses.
    #[default]
    ChannelPoints,
    /// Premier abonnement. `amount` porte le palier (1000 / 2000 / 3000).
    Sub,
    /// Réabonnement. `amount` porte le palier, `months` les mois cumulés.
    Resub,
    /// Abonnement Prime, premier ou renouvellement. Un seul type pour les deux : la
    /// distinction est déjà lisible dans `months` (0 au premier), et deux lignes à
    /// configurer pour un abonnement qui n'a qu'un palier n'apporterait rien.
    ///
    /// `amount` porte le palier, toujours 1000 — Twitch classe un Prime en tier 1.
    Prime,
    /// Abonnements offerts. `amount` porte le nombre offert d'un coup.
    Gift,
    /// Bits. `amount` porte le nombre de bits.
    Cheer,
    /// Raid entrant. `amount` porte le nombre de spectateurs amenés.
    Raid,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Alert {
    /// Absent d'un fichier existant : `ChannelPoints`, donc rien ne bouge.
    #[serde(default)]
    pub kind: AlertKind,
    /// Ne concerne que `ChannelPoints`, où il est le discriminant.
    #[serde(rename = "reward_title", default)]
    pub reward_title: String,
    /// Palier : cette ligne ne joue qu'à partir de ce montant. `0` attrape tout.
    /// Ignoré par `ChannelPoints`.
    #[serde(rename = "minAmount", default)]
    pub min_amount: u64,
    pub phrase: String,
    #[serde(rename = "imagePath")]
    pub image_path: String,
    #[serde(rename = "soundPath")]
    pub sound_path: String,
    #[serde(default)]
    pub transition: String,
    #[serde(rename = "durationMs", default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u32>,
}

/// Type à jouer quand aucune ligne n'est configurée pour celui reçu.
///
/// N'existe que pour `Prime`, et par rétro-compatibilité : avant qu'il ait son propre
/// déclencheur, un abonnement Prime arrivait en `Sub` ou en `Resub`. Sans ce repli,
/// ajouter le type ferait *disparaître* les alertes des configurations existantes,
/// qui n'ont évidemment pas de ligne `prime`.
fn fallback(kind: AlertKind, months: u64) -> Option<AlertKind> {
    match kind {
        // `months` distingue le premier abonnement du renouvellement, exactement
        // comme le faisaient les deux anciens types.
        AlertKind::Prime if months == 0 => Some(AlertKind::Sub),
        AlertKind::Prime => Some(AlertKind::Resub),
        _ => None,
    }
}

/// Ligne de configuration à jouer pour un événement reçu.
///
/// Points de chaîne : le titre exact reste le discriminant, comme avant. Autres
/// types : même `kind`, et le `min_amount` le plus élevé qui ne dépasse pas le
/// montant reçu — un palier « ≥ 1000 bits » ne doit pas se déclencher sur 50, et
/// entre deux paliers valables c'est le plus spécifique qui gagne.
///
/// La sélection vit ici et pas dans l'overlay pour deux raisons : elle est testable
/// sans navigateur, et la version JS devait re-télécharger la configuration à chaque
/// titre inconnu — ce qui, avec des `kind` sans ligne configurée, aurait déclenché
/// une requête par événement.
///
/// `months` ne sert qu'au repli de `Prime` : il n'entre dans aucun palier.
pub fn select<'a>(
    alerts: &'a [Alert],
    kind: AlertKind,
    title: &str,
    amount: u64,
    months: u64,
) -> Option<&'a Alert> {
    if kind == AlertKind::ChannelPoints {
        return alerts
            .iter()
            .find(|a| a.kind == AlertKind::ChannelPoints && a.reward_title == title);
    }

    pick(alerts, kind, amount)
        .or_else(|| fallback(kind, months).and_then(|k| pick(alerts, k, amount)))
}

fn pick(alerts: &[Alert], kind: AlertKind, amount: u64) -> Option<&Alert> {
    alerts
        .iter()
        .filter(|a| a.kind == kind && a.min_amount <= amount)
        .max_by_key(|a| a.min_amount)
}

fn normalize_path(path: &str) -> String {
    if path.is_empty() || path.starts_with("/public") {
        path.to_string()
    } else if path.starts_with("/soundboard/") || path.starts_with("/channelpoint/") {
        format!("/public{}", path)
    } else if path.starts_with("soundboard/") || path.starts_with("channelpoint/") {
        format!("/public/{}", path)
    } else if !path.starts_with('/') && !path.is_empty() {
        format!("/public/channelpoint/{}", path)
    } else {
        path.to_string()
    }
}

/// Applique à une ligne les mêmes corrections de chemin que `read` fait sur le
/// fichier.
///
/// Le bouton « Tester » du configurateur envoie la ligne **affichée**, enregistrée ou
/// non : sans ce passage, un chemin encore sous sa forme courte (`son.mp3`) jouerait
/// dans l'aperçu autrement qu'en direct.
pub fn normalized(mut alert: Alert) -> Alert {
    alert.image_path = normalize_path(&alert.image_path);
    alert.sound_path = normalize_path(&alert.sound_path);
    alert
}

pub fn read() -> Result<Vec<Alert>, String> {
    let content = match fs::read_to_string("data/channel_points.json") {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let default: Vec<Alert> = Vec::new();
            let json = serde_json::to_string_pretty(&default)
                .map_err(|e| format!("Error serializing default: {}", e))?;
            fs_atomic::write("data/channel_points.json", &json)?;
            return Ok(Vec::new());
        }
        Err(e) => return Err(format!("Error reading channel_points.json: {}", e)),
    };
    let rewards: Vec<Alert> = serde_json::from_str(&content)
        .map_err(|e| format!("Error parsing channel_points.json: {}", e))?;
    Ok(rewards.into_iter().map(normalized).collect())
}

pub fn write(rewards: Vec<Alert>) -> Result<(), String> {
    let json = serde_json::to_string_pretty(&rewards)
        .map_err(|e| format!("Error serializing: {}", e))?;
    fs_atomic::write("data/channel_points.json", &json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path_already_public() {
        assert_eq!(
            normalize_path("/public/channelpoint/img.png"),
            "/public/channelpoint/img.png"
        );
    }

    #[test]
    fn test_normalize_path_channelpoint_slash() {
        assert_eq!(
            normalize_path("/channelpoint/son.mp3"),
            "/public/channelpoint/son.mp3"
        );
    }

    #[test]
    fn test_normalize_path_channelpoint_no_slash() {
        assert_eq!(
            normalize_path("channelpoint/son.mp3"),
            "/public/channelpoint/son.mp3"
        );
    }

    #[test]
    fn test_normalize_path_soundboard_is_preserved() {
        // Un son partagé avec la soundboard ne doit pas être réécrit vers channelpoint.
        assert_eq!(
            normalize_path("/soundboard/rire.mp3"),
            "/public/soundboard/rire.mp3"
        );
        assert_eq!(
            normalize_path("soundboard/rire.mp3"),
            "/public/soundboard/rire.mp3"
        );
    }

    #[test]
    fn test_normalize_path_bare_filename() {
        assert_eq!(normalize_path("img.gif"), "/public/channelpoint/img.gif");
    }

    #[test]
    fn test_normalize_path_empty() {
        assert_eq!(normalize_path(""), "");
    }

    #[test]
    fn test_normalize_path_absolute_other_is_left_alone() {
        assert_eq!(normalize_path("/autre/chemin/img.png"), "/autre/chemin/img.png");
    }

    #[test]
    fn test_reward_roundtrip_uses_json_field_names() {
        let json = r#"{
            "reward_title": "Un cookie ?!",
            "phrase": "Merci",
            "imagePath": "a.gif",
            "soundPath": "b.mp3",
            "transition": "zoom"
        }"#;
        let reward: Alert = serde_json::from_str(json).unwrap();

        assert_eq!(reward.reward_title, "Un cookie ?!");
        assert_eq!(reward.image_path, "a.gif");
        assert_eq!(reward.sound_path, "b.mp3");
        assert_eq!(reward.transition, "zoom");
        assert_eq!(reward.duration_ms, None);

        // Les clés camelCase attendues par le configurateur doivent survivre.
        let out = serde_json::to_string(&reward).unwrap();
        assert!(out.contains("\"imagePath\""));
        assert!(out.contains("\"soundPath\""));
        assert!(out.contains("\"minAmount\""));
    }

    #[test]
    fn test_reward_transition_defaults_when_absent() {
        let json = r#"{
            "reward_title": "X",
            "phrase": "",
            "imagePath": "",
            "soundPath": ""
        }"#;
        let reward: Alert = serde_json::from_str(json).unwrap();
        assert_eq!(reward.transition, "");
    }

    // ── Types d'alerte et paliers ───────────────────────────────────────────────

    fn alert(kind: AlertKind, min_amount: u64) -> Alert {
        Alert {
            kind,
            reward_title: String::new(),
            min_amount,
            phrase: String::new(),
            image_path: String::new(),
            sound_path: String::new(),
            transition: String::new(),
            duration_ms: None,
        }
    }

    #[test]
    fn un_fichier_sans_kind_reste_une_recompense() {
        // Rétro-compatibilité : c'est ce qui permet à un channel_points.json
        // existant de continuer à fonctionner sans migration.
        let json = r#"{
            "reward_title": "X",
            "phrase": "",
            "imagePath": "",
            "soundPath": ""
        }"#;
        let reward: Alert = serde_json::from_str(json).unwrap();
        assert_eq!(reward.kind, AlertKind::ChannelPoints);
        assert_eq!(reward.min_amount, 0);
    }

    #[test]
    fn les_kind_se_serialisent_en_snake_case() {
        // La page de configuration compare ces chaînes telles quelles.
        let out = serde_json::to_string(&alert(AlertKind::ChannelPoints, 0)).unwrap();
        assert!(out.contains("\"channel_points\""), "{out}");
        let out = serde_json::to_string(&alert(AlertKind::Cheer, 0)).unwrap();
        assert!(out.contains("\"cheer\""), "{out}");
        let out = serde_json::to_string(&alert(AlertKind::Prime, 0)).unwrap();
        assert!(out.contains("\"prime\""), "{out}");
    }

    #[test]
    fn une_recompense_se_choisit_par_titre_exact() {
        let mut a = alert(AlertKind::ChannelPoints, 0);
        a.reward_title = "Hydratation".to_string();
        let mut b = alert(AlertKind::ChannelPoints, 0);
        b.reward_title = "Un cookie".to_string();
        let alerts = vec![a, b];

        let found = select(&alerts, AlertKind::ChannelPoints, "Un cookie", 0, 0).unwrap();
        assert_eq!(found.reward_title, "Un cookie");
        // Titre inconnu : rien, pas de repli sur la première ligne.
        assert!(select(&alerts, AlertKind::ChannelPoints, "Autre", 0, 0).is_none());
    }

    #[test]
    fn le_palier_le_plus_eleve_atteint_gagne() {
        let alerts = vec![
            alert(AlertKind::Cheer, 0),
            alert(AlertKind::Cheer, 100),
            alert(AlertKind::Cheer, 1000),
        ];

        assert_eq!(select(&alerts, AlertKind::Cheer, "", 50, 0).unwrap().min_amount, 0);
        assert_eq!(select(&alerts, AlertKind::Cheer, "", 100, 0).unwrap().min_amount, 100);
        assert_eq!(select(&alerts, AlertKind::Cheer, "", 500, 0).unwrap().min_amount, 100);
        assert_eq!(select(&alerts, AlertKind::Cheer, "", 5000, 0).unwrap().min_amount, 1000);
    }

    #[test]
    fn un_montant_sous_le_plus_petit_palier_ne_declenche_rien() {
        // Volontaire : c'est ainsi qu'on ignore les petits cheers, en n'ayant
        // aucune ligne à 0.
        let alerts = vec![alert(AlertKind::Cheer, 100)];
        assert!(select(&alerts, AlertKind::Cheer, "", 50, 0).is_none());
    }

    #[test]
    fn les_paliers_ne_traversent_pas_les_types() {
        // Un palier de bits ne doit jamais répondre pour un raid, même si le
        // montant conviendrait.
        let alerts = vec![alert(AlertKind::Cheer, 0)];
        assert!(select(&alerts, AlertKind::Raid, "", 10, 0).is_none());
    }

    #[test]
    fn un_ordre_de_fichier_quelconque_donne_le_meme_palier() {
        // La sélection ne doit pas dépendre de l'ordre des lignes : le
        // configurateur laisse réordonner librement.
        let alerts = vec![
            alert(AlertKind::Sub, 3000),
            alert(AlertKind::Sub, 1000),
            alert(AlertKind::Sub, 2000),
        ];
        assert_eq!(select(&alerts, AlertKind::Sub, "", 2000, 0).unwrap().min_amount, 2000);
    }

    // ── Repli du type Prime ─────────────────────────────────────────────────────

    #[test]
    fn une_ligne_prime_configuree_passe_avant_le_repli() {
        let mut prime = alert(AlertKind::Prime, 0);
        prime.phrase = "prime".to_string();
        let mut sub = alert(AlertKind::Sub, 0);
        sub.phrase = "sub".to_string();
        let mut resub = alert(AlertKind::Resub, 0);
        resub.phrase = "resub".to_string();
        let alerts = vec![sub, resub, prime];

        assert_eq!(select(&alerts, AlertKind::Prime, "", 1000, 0).unwrap().phrase, "prime");
        assert_eq!(select(&alerts, AlertKind::Prime, "", 1000, 12).unwrap().phrase, "prime");
    }

    #[test]
    fn sans_ligne_prime_un_premier_abonnement_joue_la_ligne_sub() {
        // Le cas d'une configuration écrite avant l'arrivée du type : elle doit
        // continuer à alerter, sinon ajouter le déclencheur en supprimerait.
        let mut sub = alert(AlertKind::Sub, 0);
        sub.phrase = "sub".to_string();
        let mut resub = alert(AlertKind::Resub, 0);
        resub.phrase = "resub".to_string();
        let alerts = vec![sub, resub];

        assert_eq!(select(&alerts, AlertKind::Prime, "", 1000, 0).unwrap().phrase, "sub");
    }

    #[test]
    fn sans_ligne_prime_un_renouvellement_joue_la_ligne_resub() {
        let mut sub = alert(AlertKind::Sub, 0);
        sub.phrase = "sub".to_string();
        let mut resub = alert(AlertKind::Resub, 0);
        resub.phrase = "resub".to_string();
        let alerts = vec![sub, resub];

        assert_eq!(select(&alerts, AlertKind::Prime, "", 1000, 5).unwrap().phrase, "resub");
    }

    #[test]
    fn le_repli_respecte_les_paliers_de_la_ligne_visee() {
        // Un palier « ≥ 2000 » ne doit pas rattraper un Prime, qui vaut 1000.
        let alerts = vec![alert(AlertKind::Sub, 2000)];
        assert!(select(&alerts, AlertKind::Prime, "", 1000, 0).is_none());
    }

    #[test]
    fn le_repli_ne_joue_que_dans_le_sens_prime_vers_abonnement() {
        // Une ligne `prime` ne doit jamais servir à un abonnement payant.
        let alerts = vec![alert(AlertKind::Prime, 0)];
        assert!(select(&alerts, AlertKind::Sub, "", 1000, 0).is_none());
        assert!(select(&alerts, AlertKind::Resub, "", 1000, 5).is_none());
    }
}