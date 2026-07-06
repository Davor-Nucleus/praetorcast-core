use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Serialize, Deserialize, Clone)]
pub struct BannerCard {
    pub id: Option<String>,
    pub text: String,
    #[serde(rename = "imagePath")]
    pub image_path: String,
    pub transition: String,
    pub order: usize,
    /// Durée d'affichage de la carte avant rotation, en millisecondes.
    /// Absente sur les cartes existantes → l'overlay applique sa valeur par défaut.
    #[serde(rename = "durationMs", default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u32>,
}

#[derive(Serialize, Deserialize)]
struct BannerConfig {
    cards: Vec<BannerCard>,
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

pub fn read() -> Result<Vec<BannerCard>, String> {
    // `data/banner.json` n'est plus suivi par git (il est réécrit à chaque « Save »).
    // En son absence (premier lancement / clone frais), on retombe sur l'exemple
    // committé, et à défaut sur une config vide — jamais une erreur bloquante.
    let content = match fs::read_to_string("data/banner.json") {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            match fs::read_to_string("data/banner.example.json") {
                Ok(c) => c,
                Err(_) => return Ok(Vec::new()),
            }
        }
        Err(e) => return Err(format!("Error reading banner.json: {}", e)),
    };
    let config: BannerConfig = serde_json::from_str(&content)
        .map_err(|e| format!("Error parsing banner.json: {}", e))?;
    Ok(config.cards.into_iter().map(|mut c| {
        c.image_path = normalize_path(c.image_path);
        c
    }).collect())
}

pub fn write(cards: Vec<BannerCard>) -> Result<(), String> {
    let ordered: Vec<BannerCard> = cards.into_iter().enumerate()
        .map(|(i, mut c)| { c.order = i; c })
        .collect();
    let config = BannerConfig { cards: ordered };
    let json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Error serializing: {}", e))?;
    fs::create_dir_all("data")
        .map_err(|e| format!("Error creating data dir: {}", e))?;
    fs::write("data/banner.json", json)
        .map_err(|e| format!("Error writing banner.json: {}", e))
}
