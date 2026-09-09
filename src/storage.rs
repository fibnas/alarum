use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::model::AlarmStore;

pub struct LoadOutcome {
    pub store: AlarmStore,
    pub warning: Option<String>,
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("alarum")
        .join("alarms.json")
}

pub fn load() -> LoadOutcome {
    let path = config_path();
    match fs::read_to_string(&path) {
        Err(err) if err.kind() == io::ErrorKind::NotFound => LoadOutcome {
            store: AlarmStore::new(),
            warning: None,
        },
        Err(err) => LoadOutcome {
            store: AlarmStore::new(),
            warning: Some(format!("Could not read {}: {err}", path.display())),
        },
        Ok(text) => match serde_json::from_str(&text) {
            Ok(store) => LoadOutcome {
                store,
                warning: None,
            },
            Err(err) => {
                let stamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let backup = path.with_file_name(format!("alarms.json.corrupt-{stamp}"));
                let backup_note = match fs::copy(&path, &backup) {
                    Ok(_) => format!(" A copy was saved as {}.", backup.display()),
                    Err(_) => String::new(),
                };
                LoadOutcome {
                    store: AlarmStore::new(),
                    warning: Some(format!(
                        "Alarm file was unreadable ({err}). Started empty.{backup_note}"
                    )),
                }
            }
        },
    }
}

pub fn save(store: &AlarmStore) -> Result<(), String> {
    let path = config_path();
    let parent = path
        .parent()
        .ok_or_else(|| "invalid alarm file path".to_string())?;
    fs::create_dir_all(parent).map_err(|err| format!("Could not create {}: {err}", parent.display()))?;

    let tmp = parent.join("alarms.json.tmp");
    let text = serde_json::to_string_pretty(store)
        .map_err(|err| format!("Could not serialize alarms: {err}"))?;

    {
        let mut file = File::create(&tmp)
            .map_err(|err| format!("Could not write {}: {err}", tmp.display()))?;
        file.write_all(text.as_bytes())
            .map_err(|err| format!("Could not write {}: {err}", tmp.display()))?;
        file.sync_all()
            .map_err(|err| format!("Could not flush {}: {err}", tmp.display()))?;
    }

    fs::rename(&tmp, &path).map_err(|err| format!("Could not replace {}: {err}", path.display()))?;
    Ok(())
}
