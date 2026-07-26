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
        let reward: ChannelPointReward = serde_json::from_str(json).unwrap();

        assert_eq!(reward.reward_title, "Un cookie ?!");
        assert_eq!(reward.image_path, "a.gif");
        assert_eq!(reward.sound_path, "b.mp3");
        assert_eq!(reward.transition, "zoom");
        assert_eq!(reward.duration_ms, None);

        // Les clés camelCase attendues par le configurateur doivent survivre.
        let out = serde_json::to_string(&reward).unwrap();
        assert!(out.contains("\"imagePath\""));
        assert!(out.contains("\"soundPath\""));
    }

    #[test]
    fn test_reward_transition_defaults_when_absent() {
        let json = r#"{
            "reward_title": "X",
            "phrase": "",
            "imagePath": "",
            "soundPath": ""
        }"#;
        let reward: ChannelPointReward = serde_json::from_str(json).unwrap();
        assert_eq!(reward.transition, "");
    }
}