//! Remembers the last workspace folder and file between launches.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Session {
    pub root: Option<PathBuf>,
    pub file: Option<PathBuf>,
}

/// `$ASTER_SESSION_FILE` (for tests and multiple setups) or the platform config dir.
fn session_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("ASTER_SESSION_FILE") {
        return Some(PathBuf::from(p));
    }
    Some(dirs::config_dir()?.join("aster").join("session.json"))
}

impl Session {
    pub fn load() -> Self {
        session_path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = session_path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }
}
