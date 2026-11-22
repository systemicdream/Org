//! Watches the workspace folder and reports changed `.org` files and folders in batches.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use futures::channel::mpsc;
use notify::{RecursiveMode, Watcher};

pub struct FolderWatcher {
    pub _watcher: notify::RecommendedWatcher,
    pub changes: mpsc::UnboundedReceiver<PathBuf>,
}

/// Start watching `root` recursively. Changed paths arrive on `changes`.
pub fn watch(root: &Path) -> notify::Result<FolderWatcher> {
    let (tx, rx) = mpsc::unbounded();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            for path in event.paths {
                let _ = tx.unbounded_send(path);
            }
        }
    })?;
    watcher.watch(root, RecursiveMode::Recursive)?;
    Ok(FolderWatcher {
        _watcher: watcher,
        changes: rx,
    })
}

/// Collect a batch of changed paths that matter (deduplicated): `.org` files, and folders
/// (or what may have been one: a deleted path without an extension). Skips hidden files such as
/// our own temp files.
pub fn relevant_paths(paths: impl IntoIterator<Item = PathBuf>) -> BTreeSet<PathBuf> {
    paths
        .into_iter()
        .filter(|p| {
            let hidden = p
                .file_name()
                .is_none_or(|n| n.to_string_lossy().starts_with('.'));
            !hidden
                && (aster_index::is_org_file(p)
                    || p.is_dir()
                    || (!p.exists() && p.extension().is_none()))
        })
        .collect()
}
