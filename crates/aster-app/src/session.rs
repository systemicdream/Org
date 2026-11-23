//! Remembers the last workspace folder and file between launches, and the notebooks (notes
//! folders) used recently, for the sidebar's notebook switcher.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Session {
    pub root: Option<PathBuf>,
    pub file: Option<PathBuf>,
    /// Recently used notebooks, most recent first.
    #[serde(default)]
    pub recent: Vec<RecentNotebook>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentNotebook {
    pub root: PathBuf,
    /// The note last open in it, reopened when switching back.
    pub file: Option<PathBuf>,
}

/// How many notebooks the switcher remembers.
const MAX_RECENT: usize = 8;

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

    /// Record `root` (with `file` open in it) as the current and most recent notebook.
    pub fn remember(&mut self, root: Option<PathBuf>, file: Option<PathBuf>) {
        if let Some(root) = &root {
            self.recent.retain(|n| &n.root != root);
            self.recent.insert(
                0,
                RecentNotebook {
                    root: root.clone(),
                    file: file.clone().filter(|f| f.starts_with(root)),
                },
            );
            self.recent.truncate(MAX_RECENT);
        }
        self.root = root;
        self.file = file;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_recent_notebooks() {
        let mut s = Session::default();
        s.remember(Some("/a".into()), Some("/a/x.org".into()));
        s.remember(Some("/b".into()), Some("/elsewhere.org".into()));
        s.remember(Some("/a".into()), None);
        let roots: Vec<_> = s.recent.iter().map(|n| n.root.clone()).collect();
        assert_eq!(roots, [PathBuf::from("/a"), PathBuf::from("/b")]);
        assert_eq!(s.recent[0].file, None);
        // A file outside the notebook isn't its last note.
        assert_eq!(s.recent[1].file, None);
        for i in 0..20 {
            s.remember(Some(format!("/n{i}").into()), None);
        }
        assert_eq!(s.recent.len(), MAX_RECENT);
    }
}
