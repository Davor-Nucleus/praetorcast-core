use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Serialize, Deserialize, Clone)]
pub struct DaySchedule {
    #[serde(rename = "dayIndex")]
    pub day_index: usize,
    pub day: String,
    pub date: String,
    pub title: String,
    #[serde(rename = "coverPath")]
    pub cover_path: String,
    #[serde(rename = "time", default)]
    pub time: String,
}

#[derive(Serialize, Deserialize)]
pub struct SchedulerConfig {
    pub schedule: Vec<DaySchedule>,
    #[serde(rename = "backgroundImage")]
    pub background_image: Option<String>,
}

fn normalize_path(path: &str) -> String {
    if path.is_empty() || path.starts_with("/public") {
        path.to_string()
    } else if path.starts_with("/scheduler/") {
        format!("/public{}", path)
    } else if path.starts_with("scheduler/") {
        format!("/public/{}", path)
    } else if !path.starts_with('/') {
        format!("/public/scheduler/{}", path)
    } else {
        path.to_string()
    }
}

pub fn read() -> Result<SchedulerConfig, String> {
    let content = fs::read_to_string("data/scheduler.json")
        .map_err(|e| format!("Error reading scheduler.json: {}", e))?;
    let mut config: SchedulerConfig = serde_json::from_str(&content)
        .map_err(|e| format!("Error parsing scheduler.json: {}", e))?;
    for day in &mut config.schedule {
        day.cover_path = normalize_path(&day.cover_path);
    }
    if let Some(ref mut bg) = config.background_image {
        *bg = normalize_path(bg);
    }
    Ok(config)
}

pub fn write(config: &SchedulerConfig) -> Result<(), String> {
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Error serializing: {}", e))?;
    fs::create_dir_all("data")
        .map_err(|e| format!("Error creating data dir: {}", e))?;
    fs::write("data/scheduler.json", json)
        .map_err(|e| format!("Error writing scheduler.json: {}", e))
}
