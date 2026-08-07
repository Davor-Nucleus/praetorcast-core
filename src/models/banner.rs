use serde::{Deserialize, Serialize};
use std::fs;

/// Nature d'une carte de la rotation.
///
/// Le choix est fait à l'ajout dans `/banner-config`. `Text` reste le défaut : une
/// carte écrite avant l'arrivée des objectifs n'a pas la clé et doit rester une
/// carte texte.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum CardKind {
    /// Texte et/ou image, la carte historique.
    #[default]
    Text,
    /// Une ou toutes les barres d'objectif définies dans `/goal-config`.
    Goal,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct BannerCard {
    pub id: Option<String>,
    #[serde(default)]
    pub kind: CardKind,
    /// `default` : une carte d'objectif n'a ni texte ni image à porter.
    #[serde(default)]
    pub text: String,
    #[serde(rename = "imagePath", default)]
    pub image_path: String,
    #[serde(default = "default_transition")]
    pub transition: String,
    #[serde(default)]
    pub order: usize,
    /// Durée d'affichage de la carte avant rotation, en millisecondes.
    /// Absente sur les cartes existantes → l'overlay applique sa valeur par défaut.
    #[serde(rename = "durationMs", default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u32>,
    /// Objectif affiché par une carte `Goal`, par son `id`.
    /// Absent — ou introuvable — signifie « tous les objectifs ».
    #[serde(rename = "goalId", default, skip_serializing_if = "Option::is_none")]
    pub goal_id: Option<String>,
}

/// Bord occupé par les barres fixes.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum DockPosition {
    Top,
    #[default]
    Bottom,
}

/// Barres d'objectif affichées **en permanence** sur un bord, pendant que les
/// cartes tournent au-dessus.
///
/// Distinct d'une carte `Goal` : ce n'est pas un élément de la rotation, il ne
/// prend donc jamais son tour et ne disparaît pas.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BannerDock {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub position: DockPosition,
    /// Comme `BannerCard::goal_id` : absent = tous les objectifs.
    #[serde(rename = "goalId", default, skip_serializing_if = "Option::is_none")]
    pub goal_id: Option<String>,
    /// Ajustement de taille. `1` est la taille prévue par l'habillage bannière,
    /// qui se calibre seul sur la largeur de la source (`clamp()`) ; ce réglage
    /// ne sert qu'à s'en écarter.
    #[serde(default = "default_scale")]
    pub scale: f32,
}

fn default_transition() -> String {
    "fade".to_string()
}

fn default_scale() -> f32 {
    1.0
}

impl Default for BannerDock {
    fn default() -> Self {
        Self {
            enabled: false,
            position: DockPosition::default(),
            goal_id: None,
            scale: default_scale(),
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
pub struct BannerConfig {
    #[serde(default)]
    pub cards: Vec<BannerCard>,
    /// Clé apparue après coup : `default` pour qu'un `banner.json` antérieur reste
    /// lisible, avec les barres fixes simplement désactivées.
    #[serde(default)]
    pub dock: BannerDock,
}

fn normalize_path(path: String) -> String {
    if path.starts_with("/public") {
        path
    } else if path.starts_with("/banner/") {
        format!("/public{}", path)
    } else if path.starts_with("banner/") {
        format!("/public/{}", path)
    } else if !path.is_empty() && !path.starts_with('/') {
        format!("/public/banner/{}", path)
    } else {
        path
    }
}

pub fn read() -> Result<BannerConfig, String> {
    // `data/banner.json` n'est plus suivi par git (il est réécrit à chaque « Save »).
    // En son absence (premier lancement / clone frais), on retombe sur l'exemple
    // committé, et à défaut sur une config vide — jamais une erreur bloquante.
    let content = match fs::read_to_string("data/banner.json") {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Tente d'abord l'exemple, sinon crée un fichier vide par défaut
            if let Ok(c) = fs::read_to_string("data/banner.example.json") {
                c
            } else {
                let default = BannerConfig::default();
                write(&default)?;
                return Ok(default);
            }
        }
        Err(e) => return Err(format!("Error reading banner.json: {}", e)),
    };
    let mut config: BannerConfig = serde_json::from_str(&content)
        .map_err(|e| format!("Error parsing banner.json: {}", e))?;
    for card in &mut config.cards {
        card.image_path = normalize_path(std::mem::take(&mut card.image_path));
    }
    Ok(config)
}

/// Réindexe `order` sur la position dans le tableau. Séparée de `write` pour être
/// testable sans toucher au disque (un test qui appelait `write` écrasait la vraie
/// configuration de l'utilisateur).
fn reorder(cards: Vec<BannerCard>) -> Vec<BannerCard> {
    cards.into_iter().enumerate()
        .map(|(i, mut c)| { c.order = i; c })
        .collect()
}

pub fn write(config: &BannerConfig) -> Result<(), String> {
    let config = BannerConfig {
        cards: reorder(config.cards.clone()),
        dock: config.dock.clone(),
    };
    let json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Error serializing: {}", e))?;
    fs::create_dir_all("data")
        .map_err(|e| format!("Error creating data dir: {}", e))?;
    fs::write("data/banner.json", json)
        .map_err(|e| format!("Error writing banner.json: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_banner_card_serialization_roundtrip() {
        let card = BannerCard {
            id: Some("123".to_string()),
            kind: CardKind::Text,
            text: "Hello World".to_string(),
            image_path: "/public/banner/test.png".to_string(),
            transition: "fade".to_string(),
            order: 0,
            duration_ms: Some(5000),
            goal_id: None,
        };
        let json = serde_json::to_string(&card).unwrap();
        let deserialized: BannerCard = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, Some("123".to_string()));
        assert_eq!(deserialized.text, "Hello World");
        assert_eq!(deserialized.image_path, "/public/banner/test.png");
        assert_eq!(deserialized.transition, "fade");
        assert_eq!(deserialized.order, 0);
        assert_eq!(deserialized.duration_ms, Some(5000));
    }

    #[test]
    fn test_banner_card_optional_duration() {
        let json = r#"{
            "text": "No duration",
            "imagePath": "test.png",
            "transition": "slide",
            "order": 1
        }"#;
        let card: BannerCard = serde_json::from_str(json).unwrap();
        assert_eq!(card.duration_ms, None);
    }

    #[test]
    fn test_banner_card_optional_id() {
        let json = r#"{
            "text": "No id",
            "imagePath": "test.png",
            "transition": "slide",
            "order": 1
        }"#;
        let card: BannerCard = serde_json::from_str(json).unwrap();
        assert_eq!(card.id, None);
    }

    #[test]
    fn test_normalize_path_already_public() {
        assert_eq!(normalize_path("/public/banner/img.png".to_string()), "/public/banner/img.png");
    }

    #[test]
    fn test_normalize_path_banner_slash() {
        assert_eq!(normalize_path("/banner/img.png".to_string()), "/public/banner/img.png");
    }

    #[test]
    fn test_normalize_path_banner_no_slash() {
        assert_eq!(normalize_path("banner/img.png".to_string()), "/public/banner/img.png");
    }

    #[test]
    fn test_normalize_path_relative() {
        assert_eq!(normalize_path("img.png".to_string()), "/public/banner/img.png");
    }

    #[test]
    fn test_normalize_path_empty() {
        assert_eq!(normalize_path("".to_string()), "");
    }

    fn card(text: &str, order: usize) -> BannerCard {
        BannerCard {
            id: None,
            kind: CardKind::Text,
            text: text.to_string(),
            image_path: "a.png".to_string(),
            transition: "fade".to_string(),
            order,
            duration_ms: None,
            goal_id: None,
        }
    }

    fn parse(json: &str) -> BannerConfig {
        serde_json::from_str(json).unwrap()
    }

    // ── Cartes d'objectif ─────────────────────────────────────────────────────

    #[test]
    fn une_carte_sans_kind_reste_une_carte_texte() {
        // Tout `banner.json` antérieur : aucune carte ne doit se transformer en
        // barre d'objectif du jour au lendemain.
        let config = parse(r#"{"cards":[{"text":"Bonjour","imagePath":"a.png","transition":"fade","order":0}]}"#);
        assert_eq!(config.cards[0].kind, CardKind::Text);
        assert_eq!(config.cards[0].goal_id, None);
    }

    #[test]
    fn une_carte_d_objectif_porte_son_id_de_cible() {
        let config = parse(
            r#"{"cards":[{"kind":"goal","goalId":"abc","transition":"zoom","order":0,"durationMs":4000}]}"#,
        );
        assert_eq!(config.cards[0].kind, CardKind::Goal);
        assert_eq!(config.cards[0].goal_id, Some("abc".to_string()));
        assert_eq!(config.cards[0].duration_ms, Some(4000));
        // Ni texte ni image : les deux champs doivent être optionnels.
        assert_eq!(config.cards[0].text, "");
        assert_eq!(config.cards[0].image_path, "");
    }

    #[test]
    fn une_carte_d_objectif_sans_cible_vaut_tous_les_objectifs() {
        let config = parse(r#"{"cards":[{"kind":"goal","order":0}]}"#);
        assert_eq!(config.cards[0].goal_id, None);
        // Et la transition retombe sur le défaut plutôt que sur une chaîne vide,
        // qui produirait une classe CSS `-in` inexistante.
        assert_eq!(config.cards[0].transition, "fade");
    }

    #[test]
    fn le_kind_est_serialise_en_minuscules() {
        // C'est la forme que le JavaScript de l'overlay compare directement.
        let mut c = card("x", 0);
        c.kind = CardKind::Goal;
        c.goal_id = Some("g1".to_string());
        let json = serde_json::to_string(&BannerConfig { cards: vec![c], dock: BannerDock::default() }).unwrap();
        assert!(json.contains("\"kind\":\"goal\""));
        assert!(json.contains("\"goalId\":\"g1\""));
    }

    #[test]
    fn le_reordonnancement_preserve_le_type_et_la_cible() {
        let mut goal_card = card("", 5);
        goal_card.kind = CardKind::Goal;
        goal_card.goal_id = Some("g1".to_string());
        let reordered = reorder(vec![card("Texte", 9), goal_card]);
        assert_eq!(reordered[1].order, 1);
        assert_eq!(reordered[1].kind, CardKind::Goal);
        assert_eq!(reordered[1].goal_id, Some("g1".to_string()));
    }

    // ── Barres fixes ──────────────────────────────────────────────────────────

    #[test]
    fn un_fichier_sans_dock_reste_lisible() {
        let config = parse(r#"{"cards":[]}"#);
        assert_eq!(config.dock, BannerDock::default());
        assert!(!config.dock.enabled, "les barres fixes ne doivent pas s'activer seules");
        assert_eq!(config.dock.scale, 1.0);
    }

    #[test]
    fn le_dock_est_relu_champ_par_champ() {
        let config = parse(
            r#"{"cards":[],"dock":{"enabled":true,"position":"top","goalId":"g2","scale":2.0}}"#,
        );
        assert!(config.dock.enabled);
        assert_eq!(config.dock.position, DockPosition::Top);
        assert_eq!(config.dock.goal_id, Some("g2".to_string()));
        assert_eq!(config.dock.scale, 2.0);
    }

    #[test]
    fn un_dock_partiel_complete_les_defauts() {
        let config = parse(r#"{"cards":[],"dock":{"enabled":true}}"#);
        assert_eq!(config.dock.position, DockPosition::Bottom);
        assert_eq!(config.dock.scale, 1.0);
        assert_eq!(config.dock.goal_id, None);
    }

    #[test]
    fn roundtrip_complet_du_fichier() {
        let mut goal_card = card("", 1);
        goal_card.kind = CardKind::Goal;
        goal_card.goal_id = Some("g1".to_string());
        let config = BannerConfig {
            cards: vec![card("Bonjour", 0), goal_card],
            dock: BannerDock {
                enabled: true,
                position: DockPosition::Top,
                goal_id: None,
                scale: 1.8,
            },
        };
        let back = parse(&serde_json::to_string(&config).unwrap());
        assert_eq!(back.cards.len(), 2);
        assert_eq!(back.cards[1].kind, CardKind::Goal);
        assert_eq!(back.dock.position, DockPosition::Top);
        assert_eq!(back.dock.scale, 1.8);
        // `goalId` absent quand il vaut None : pas de `null` dans le fichier.
        assert!(!serde_json::to_string(&config).unwrap().contains("\"goalId\":null"));
    }

    #[test]
    fn test_reorder_reindexes_on_position() {
        let reordered = reorder(vec![card("Second", 99), card("First", 42)]);

        assert_eq!(reordered[0].order, 0);
        assert_eq!(reordered[1].order, 1);
        // L'ordre du tableau fait foi, le champ `order` d'entrée est ignoré.
        assert_eq!(reordered[0].text, "Second");
        assert_eq!(reordered[1].text, "First");
    }

    #[test]
    fn test_reorder_preserves_other_fields() {
        let mut input = card("Only", 7);
        input.duration_ms = Some(4200);
        let reordered = reorder(vec![input]);

        assert_eq!(reordered[0].order, 0);
        assert_eq!(reordered[0].duration_ms, Some(4200));
        assert_eq!(reordered[0].image_path, "a.png");
    }

    #[test]
    fn test_reorder_empty() {
        assert!(reorder(Vec::new()).is_empty());
    }
}
