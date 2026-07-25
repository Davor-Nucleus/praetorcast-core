use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Serialize, Deserialize, Clone)]
pub struct ChannelPointReward {
    #[serde(rename = "reward_title")]
    pub reward_title: String,
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

pub fn read() -> Result<Vec<ChannelPointReward>, String> {
    let content = match fs::read_to_string("data/channel_points.json") {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let default: Vec<ChannelPointReward> = Vec::new();
            let json = serde_json::to_string_pretty(&default)
                .map_err(|e| format!("Error serializing default: {}", e))?;
            fs::create_dir_all("data")
                .map_err(|e| format!("Error creating data dir: {}", e))?;
            fs::write("data/channel_points.json", json)
                .map_err(|e| format!("Error writing channel_points.json: {}", e))?;
            return Ok(Vec::new());
        }
        Err(e) => return Err(format!("Error reading channel_points.json: {}", e)),
    };
    let rewards: Vec<ChannelPointReward> = serde_json::from_str(&content)
        .map_err(|e| format!("Error parsing channel_points.json: {}", e))?;
    Ok(rewards.into_iter().map(|mut c| {
        c.image_path = normalize_path(&c.image_path);
        c.sound_path = normalize_path(&c.sound_path);
        c
    }).collect())
}

pub fn write(rewards: Vec<ChannelPointReward>) -> Result<(), String> {
    let json = serde_json::to_string_pretty(&rewards)
        .map_err(|e| format!("Error serializing: {}", e))?;
    fs::create_dir_all("data")
        .map_err(|e| format!("Error creating data dir: {}", e))?;
    fs::write("data/channel_points.json", json)
        .map_err(|e| format!("Error writing channel_points.json: {}", e))
}