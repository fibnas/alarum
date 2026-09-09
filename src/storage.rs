use std::fs;
use std::path::PathBuf;

use crate::model::AlarmStore;

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("alarum")
        .join("alarms.json")
}

pub fn load() -> AlarmStore {
    let path = config_path();
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|_| AlarmStore::new()),
        Err(_) => AlarmStore::new(),
    }
}

pub fn save(store: &AlarmStore) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(store) {
        let _ = fs::write(path, text);
    }
}
