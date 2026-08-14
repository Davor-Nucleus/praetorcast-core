//! Textes animés adressables par URL (`/text?name=...`).
//!
//! Même découpage que `banner` : une liste dans un fichier JSON de `data/`, une
//! lecture qui ne bloque jamais l'overlay, une écriture qui recrée le dossier au
//! besoin. La différence tient au mode d'adressage : une bannière affiche toutes
//! ses cartes en rotation, alors qu'une source OBS ne demande **qu'une** section,
//! désignée par son `name` dans la query string. Ce nom est donc une clé d'URL et
//! non un simple libellé — d'où `slugify_names` (cf. plus bas).

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;

const TEXT_PATH: &str = "data/text.json";

/// Animation d'**entrée**, jouée une seule fois quand le texte apparaît.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum TextAnimation {
    /// Le texte est simplement posé, sans apparition.
    #[default]
    None,
    Fade,
    Slide,
    Zoom,
    Flip,
    /// Arrivée élastique : le texte dépasse sa taille finale puis se cale.
    Bounce,
    /// Chute depuis le haut, avec un léger dépassement à l'atterrissage.
    Drop,
    /// Pivot autour du bord supérieur, comme une pancarte qu'on lâche.
    Swing,
    /// Mise au point : le texte se précise depuis un flou.
    Blur,
    /// Machine à écrire : les caractères s'affichent un à un.
    Typewriter,
    /// Cascade : les lettres montent une à une. Comme `Typewriter`, mais chaque
    /// lettre est animée au lieu d'être simplement dévoilée.
    Cascade,
}

/// Animation **continue**, rejouée en boucle tant que le texte est affiché.
///
/// Distincte de `TextAnimation` et cumulable avec elle : un `<select>` unique
/// obligerait à choisir entre « apparaît en fondu » et « ondule en permanence »,
/// alors que les deux se combinent naturellement.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum TextEffect {
    #[default]
    None,
    /// Défilement horizontal sans fin. Occupe toute la largeur : `align` n'a
    /// plus d'effet sur une section en marquee.
    Marquee,
    Pulse,
    Wave,
    Glitch,
    /// Dégradé du thème (`--pc-gradient`) balayant le texte.
    Gradient,
    /// Lévitation : le texte monte et descend doucement.
    Float,
    /// Balancier : rotation lente d'un bord à l'autre.
    Tilt,
    /// Tremblement nerveux, sans repos.
    Shake,
    /// Halo pulsant aux couleurs d'accent du thème.
    Neon,
    /// Dégradé arc-en-ciel défilant. Distinct de `Gradient`, qui suit le thème :
    /// celui-ci a ses propres teintes et reste visible quel que soit l'habillage.
    Rainbow,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum TextVAlign {
    Top,
    #[default]
    Middle,
    Bottom,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TextSection {
    /// Identifiant stable posé par le configurateur, comme `Goal::id` : sert
    /// uniquement côté client à suivre une ligne pendant les réordonnancements.
    /// `name` peut changer (l'utilisateur le renomme), pas lui.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Clé d'URL — c'est le `name` de `/text?name=...`. Normalisé à l'écriture.
    #[serde(default)]
    pub name: String,
    /// Libellé lisible affiché dans le configurateur, jamais dans l'overlay.
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub animation: TextAnimation,
    #[serde(default)]
    pub effect: TextEffect,
    /// Durée de l'animation d'entrée, en ms.
    #[serde(rename = "animationMs", default = "default_animation_ms")]
    pub animation_ms: u32,
    /// Période d'un cycle de l'effet continu, en ms. Champ distinct de
    /// `animation_ms` : une valeur unique ne peut pas convenir aux deux, un
    /// marquee lisible demande ~12 s là où une entrée en fondu tient en 0,8 s.
    #[serde(rename = "effectMs", default = "default_effect_ms")]
    pub effect_ms: u32,
    /// Ajustement de taille. `1` est le calibrage prévu par l'habillage, qui se
    /// cale seul sur la largeur de la source (`clamp()`) ; ce réglage ne sert
    /// qu'à s'en écarter.
    #[serde(default = "default_scale")]
    pub scale: f32,
    /// Couleur du texte. Vide = couleur du thème (`--pc-text`).
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub align: TextAlign,
    #[serde(rename = "verticalAlign", default)]
    pub vertical_align: TextVAlign,
    /// Fond de la section. Vide = transparent, le défaut voulu pour une source
    /// navigateur OBS posée par-dessus un gameplay.
    #[serde(rename = "backgroundColor", default)]
    pub background_color: String,
    #[serde(default)]
    pub order: usize,
}

fn default_animation_ms() -> u32 {
    800
}

fn default_effect_ms() -> u32 {
    2000
}

fn default_scale() -> f32 {
    1.0
}

impl Default for TextSection {
    fn default() -> Self {
        Self {
            id: None,
            name: String::new(),
            label: String::new(),
            content: String::new(),
            animation: TextAnimation::default(),
            effect: TextEffect::default(),
            animation_ms: default_animation_ms(),
            effect_ms: default_effect_ms(),
            scale: default_scale(),
            color: String::new(),
            align: TextAlign::default(),
            vertical_align: TextVAlign::default(),
            background_color: String::new(),
            order: 0,
        }
    }
}

/// Enveloppe plutôt qu'un `Vec` nu (comme `BannerConfig`, pas comme `goal.rs`) :
/// un réglage global ajouté plus tard n'imposera pas de migrer le fichier.
#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
pub struct TextConfig {
    #[serde(default)]
    pub sections: Vec<TextSection>,
}

/// Réduit un nom libre à une clé d'URL : minuscules, accents aplatis, espaces en
/// tirets, et rien d'autre que `[a-z0-9-_]`.
///
/// Les accents sont traduits à la main plutôt qu'avec une normalisation Unicode :
/// la seule attente ici est qu'« Écran de début » donne `ecran-de-debut`, ce qui
/// ne justifie pas une dépendance supplémentaire.
fn slugify(raw: &str) -> String {
    let mut slug = String::with_capacity(raw.len());
    let mut last_dash = true; // évite un tiret en tête

    for c in raw.trim().to_lowercase().chars() {
        let mapped = match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' | 'å' => 'a',
            'ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ñ' => 'n',
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ý' | 'ÿ' => 'y',
            c => c,
        };

        if mapped.is_ascii_alphanumeric() || mapped == '_' {
            slug.push(mapped);
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }

    while slug.ends_with('-') {
        slug.pop();
    }
    slug
}

/// Normalise et rend uniques les `name` de la liste.
///
/// Deux sections homonymes rendraient l'une des deux inatteignable, `/text?name=`
/// ne pouvant en désigner qu'une : les doublons reçoivent donc un suffixe `-2`,
/// `-3`… Une section sans nom prend `section-<position>`.
///
/// Séparée de `write` pour être testable sans toucher au disque — même raison que
/// `reorder` : un test qui appelait `write` écrasait la vraie configuration.
fn slugify_names(sections: Vec<TextSection>) -> Vec<TextSection> {
    let mut taken: HashSet<String> = HashSet::new();

    sections
        .into_iter()
        .enumerate()
        .map(|(i, mut section)| {
            let mut name = slugify(&section.name);
            if name.is_empty() {
                name = format!("section-{}", i + 1);
            }

            if taken.contains(&name) {
                let base = name.clone();
                let mut suffix = 2;
                while taken.contains(&name) {
                    name = format!("{}-{}", base, suffix);
                    suffix += 1;
                }
            }

            taken.insert(name.clone());
            section.name = name;
            section
        })
        .collect()
}

/// Réindexe `order` sur la position dans le tableau. Séparée de `write` pour la
/// même raison que `slugify_names`.
fn reorder(sections: Vec<TextSection>) -> Vec<TextSection> {
    sections
        .into_iter()
        .enumerate()
        .map(|(i, mut s)| {
            s.order = i;
            s
        })
        .collect()
}

pub fn read() -> Result<TextConfig, String> {
    // `data/text.json` n'est pas suivi par git (il est réécrit à chaque « Save »).
    // En son absence (premier lancement / clone frais), on le crée vide sur-le-champ
    // plutôt que de passer par un `text.example.json` committé comme le fait
    // `banner` : la page de configuration part d'une liste vierge, sans sections de
    // démonstration à supprimer avant de poser les siennes.
    let content = match fs::read_to_string(TEXT_PATH) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let default = TextConfig::default();
            write(&default)?;
            return Ok(default);
        }
        Err(e) => return Err(format!("Error reading text.json: {}", e)),
    };

    serde_json::from_str(&content).map_err(|e| format!("Error parsing text.json: {}", e))
}

pub fn write(config: &TextConfig) -> Result<(), String> {
    let config = TextConfig {
        sections: reorder(slugify_names(config.sections.clone())),
    };
    let json =
        serde_json::to_string_pretty(&config).map_err(|e| format!("Error serializing: {}", e))?;
    fs::create_dir_all("data").map_err(|e| format!("Error creating data dir: {}", e))?;
    fs::write(TEXT_PATH, json).map_err(|e| format!("Error writing text.json: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str) -> TextSection {
        TextSection {
            name: name.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_slugify_accents_and_spaces() {
        assert_eq!(slugify("Écran de début"), "ecran-de-debut");
    }

    #[test]
    fn test_slugify_strips_punctuation_and_edges() {
        assert_eq!(slugify("  Je reviens !!  "), "je-reviens");
        assert_eq!(slugify("a/b?c=d"), "a-b-c-d");
    }

    #[test]
    fn test_slugify_keeps_underscores_and_digits() {
        assert_eq!(slugify("scene_2"), "scene_2");
    }

    #[test]
    fn test_slugify_names_deduplicates() {
        let sections = slugify_names(vec![named("Start"), named("start"), named("START")]);
        let names: Vec<&str> = sections.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["start", "start-2", "start-3"]);
    }

    #[test]
    fn test_slugify_names_fills_empty() {
        let sections = slugify_names(vec![named(""), named("!!!")]);
        let names: Vec<&str> = sections.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["section-1", "section-2"]);
    }

    /// Le suffixe de dédoublonnage ne doit pas entrer en collision avec un nom
    /// que l'utilisateur a lui-même écrit `start-2`.
    #[test]
    fn test_slugify_names_suffix_avoids_existing() {
        let sections = slugify_names(vec![named("start"), named("start-2"), named("start")]);
        let names: Vec<&str> = sections.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["start", "start-2", "start-3"]);
    }

    #[test]
    fn test_reorder_reindexes_positions() {
        let mut a = named("a");
        a.order = 7;
        let mut b = named("b");
        b.order = 7;
        let sections = reorder(vec![a, b]);
        assert_eq!(sections[0].order, 0);
        assert_eq!(sections[1].order, 1);
    }

    /// Un fichier écrit avant l'ajout d'un champ doit rester lisible : tout est
    /// `#[serde(default)]`, seuls `name` et `content` sont réellement portés par
    /// la configuration minimale.
    #[test]
    fn test_minimal_json_fills_defaults() {
        let json = r#"{"sections":[{"name":"start","content":"x"}]}"#;
        let config: TextConfig = serde_json::from_str(json).unwrap();
        let section = &config.sections[0];
        assert_eq!(section.name, "start");
        assert_eq!(section.content, "x");
        assert_eq!(section.animation, TextAnimation::None);
        assert_eq!(section.effect, TextEffect::None);
        assert_eq!(section.animation_ms, 800);
        assert_eq!(section.effect_ms, 2000);
        assert_eq!(section.scale, 1.0);
        assert_eq!(section.align, TextAlign::Center);
        assert_eq!(section.vertical_align, TextVAlign::Middle);
        assert_eq!(section.background_color, "");
        assert_eq!(section.id, None);
    }

    #[test]
    fn test_empty_json_gives_empty_config() {
        let config: TextConfig = serde_json::from_str("{}").unwrap();
        assert!(config.sections.is_empty());
    }

    #[test]
    fn test_section_serialization_roundtrip() {
        let section = TextSection {
            id: Some("abc".to_string()),
            name: "start".to_string(),
            label: "Début de stream".to_string(),
            content: "Le stream commence !".to_string(),
            animation: TextAnimation::Typewriter,
            effect: TextEffect::Pulse,
            animation_ms: 1200,
            effect_ms: 1600,
            scale: 1.5,
            color: "#9146FF".to_string(),
            align: TextAlign::Left,
            vertical_align: TextVAlign::Bottom,
            background_color: "#00000080".to_string(),
            order: 3,
        };
        let json = serde_json::to_string(&section).unwrap();
        let back: TextSection = serde_json::from_str(&json).unwrap();
        assert_eq!(back, section);
    }

    /// Les enums doivent sortir en minuscules : le JS compare des chaînes brutes
    /// (`section.animation === 'typewriter'`) pour poser ses classes CSS.
    #[test]
    fn test_enums_serialize_lowercase() {
        let json = serde_json::to_string(&TextAnimation::Typewriter).unwrap();
        assert_eq!(json, "\"typewriter\"");
        let json = serde_json::to_string(&TextEffect::Gradient).unwrap();
        assert_eq!(json, "\"gradient\"");
        let json = serde_json::to_string(&TextVAlign::Middle).unwrap();
        assert_eq!(json, "\"middle\"");
        let json = serde_json::to_string(&TextAnimation::Cascade).unwrap();
        assert_eq!(json, "\"cascade\"");
        let json = serde_json::to_string(&TextEffect::Rainbow).unwrap();
        assert_eq!(json, "\"rainbow\"");
    }

    /// Chaque variante doit se relire telle quelle : c'est ce qui garantit qu'un
    /// `data/text.json` écrit par le configurateur revient identique après un
    /// aller-retour, y compris pour les animations ajoutées après coup.
    #[test]
    fn test_every_variant_roundtrips() {
        let entrances = [
            TextAnimation::None,
            TextAnimation::Fade,
            TextAnimation::Slide,
            TextAnimation::Zoom,
            TextAnimation::Flip,
            TextAnimation::Bounce,
            TextAnimation::Drop,
            TextAnimation::Swing,
            TextAnimation::Blur,
            TextAnimation::Typewriter,
            TextAnimation::Cascade,
        ];
        for animation in entrances {
            let json = serde_json::to_string(&animation).unwrap();
            assert!(
                json.chars().all(|c| c == '"' || c.is_ascii_lowercase()),
                "{} doit sortir en minuscules — le JS compare des chaînes brutes",
                json
            );
            let back: TextAnimation = serde_json::from_str(&json).unwrap();
            assert_eq!(back, animation);
        }

        let effects = [
            TextEffect::None,
            TextEffect::Marquee,
            TextEffect::Pulse,
            TextEffect::Wave,
            TextEffect::Glitch,
            TextEffect::Gradient,
            TextEffect::Float,
            TextEffect::Tilt,
            TextEffect::Shake,
            TextEffect::Neon,
            TextEffect::Rainbow,
        ];
        for effect in effects {
            let json = serde_json::to_string(&effect).unwrap();
            assert!(
                json.chars().all(|c| c == '"' || c.is_ascii_lowercase()),
                "{} doit sortir en minuscules — le JS compare des chaînes brutes",
                json
            );
            let back: TextEffect = serde_json::from_str(&json).unwrap();
            assert_eq!(back, effect);
        }
    }
}
