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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_day_schedule_deserialization() {
        let json = r#"{
            "dayIndex": 0,
            "day": "Monday",
            "date": "2024-01-01",
            "title": "Stream #1",
            "coverPath": "cover1.jpg",
            "time": "20:00"
        }"#;
        let day: DaySchedule = serde_json::from_str(json).unwrap();
        assert_eq!(day.day_index, 0);
        assert_eq!(day.day, "Monday");
        assert_eq!(day.date, "2024-01-01");
        assert_eq!(day.title, "Stream #1");
        assert_eq!(day.cover_path, "cover1.jpg");
        assert_eq!(day.time, "20:00");
    }

    #[test]
    fn test_day_schedule_optional_time_defaults_to_empty() {
        let json = r#"{
            "dayIndex": 1,
            "day": "Tuesday",
            "date": "2024-01-02",
            "title": "Stream #2",
            "coverPath": "cover2.jpg"
        }"#;
        let day: DaySchedule = serde_json::from_str(json).unwrap();
        assert_eq!(day.time, "");
    }

    #[test]
    fn test_scheduler_config_deserialization() {
        let json = r#"{
            "schedule": [
                {
                    "dayIndex": 0,
                    "day": "Monday",
                    "date": "2024-01-01",
                    "title": "Stream #1",
                    "coverPath": "cover1.jpg",
                    "time": "20:00"
                },
                {
                    "dayIndex": 1,
                    "day": "Tuesday",
                    "date": "2024-01-02",
                    "title": "Stream #2",
                    "coverPath": "cover2.jpg"
                }
            ],
            "backgroundImage": "bg.jpg"
        }"#;
        let config: SchedulerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.schedule.len(), 2);
        assert_eq!(config.background_image, Some("bg.jpg".to_string()));
        assert_eq!(config.schedule[0].day, "Monday");
        assert_eq!(config.schedule[1].day, "Tuesday");
    }

    #[test]
    fn test_scheduler_config_optional_background() {
        let json = r#"{
            "schedule": []
        }"#;
        let config: SchedulerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.schedule.len(), 0);
        assert_eq!(config.background_image, None);
    }

    #[test]
    fn test_normalize_path_already_public() {
        assert_eq!(normalize_path("/public/scheduler/img.png"), "/public/scheduler/img.png");
    }

    #[test]
    fn test_normalize_path_scheduler_slash() {
        assert_eq!(normalize_path("/scheduler/img.png"), "/public/scheduler/img.png");
    }

    #[test]
    fn test_normalize_path_scheduler_no_slash() {
        assert_eq!(normalize_path("scheduler/img.png"), "/public/scheduler/img.png");
    }

    #[test]
    fn test_normalize_path_relative() {
        assert_eq!(normalize_path("img.png"), "/public/scheduler/img.png");
    }

    #[test]
    fn test_normalize_path_empty() {
        assert_eq!(normalize_path(""), "");
    }

    #[test]
    fn test_normalize_path_absolute_other() {
        assert_eq!(normalize_path("/other/path/img.png"), "/other/path/img.png");
    }
}
