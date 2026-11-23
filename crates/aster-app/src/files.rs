//! File operations from the sidebar: new note, new folder, rename, delete (to the trash).

use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};

/// Check a single path component typed by the user.
fn valid_name(name: &str) -> anyhow::Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        bail!("Enter a name");
    }
    if name.contains(['/', '\\', ':']) || name.chars().any(char::is_control) {
        bail!("Names can't contain / \\ or :");
    }
    if name.starts_with('.') {
        bail!("Names can't start with a dot");
    }
    Ok(name)
}

/// `name` as an Org file name: `.org` is added unless it's already there.
fn org_file_name(name: &str) -> anyhow::Result<String> {
    let name = valid_name(name)?;
    Ok(if name.to_ascii_lowercase().ends_with(".org") {
        name.to_string()
    } else {
        format!("{name}.org")
    })
}

fn ensure_free(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        bail!(
            "“{}” already exists",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
    }
    Ok(())
}

/// Create `dir/<name>.org` with a title line. Returns its path.
pub fn new_note(dir: &Path, name: &str) -> anyhow::Result<PathBuf> {
    let file = org_file_name(name)?;
    let path = dir.join(&file);
    ensure_free(&path)?;
    let title = file.trim_end_matches(".org").trim_end_matches(".ORG");
    std::fs::create_dir_all(dir)?;
    crate::workspace::write_atomic(&path, &format!("#+TITLE: {title}\n\n"))
        .with_context(|| format!("creating {file}"))?;
    Ok(path)
}

/// Create the folder `dir/<name>`. Returns its path.
pub fn new_folder(dir: &Path, name: &str) -> anyhow::Result<PathBuf> {
    let path = dir.join(valid_name(name)?);
    ensure_free(&path)?;
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

/// Rename a file (keeping it an `.org` file) or folder in place. Returns the new path.
pub fn rename(path: &Path, name: &str) -> anyhow::Result<PathBuf> {
    let name = if path.is_dir() {
        valid_name(name)?.to_string()
    } else {
        org_file_name(name)?
    };
    let target = path.with_file_name(&name);
    if target == path {
        return Ok(target);
    }
    // A case-only rename on a case-insensitive disk "exists" already; allow it.
    let same_file = std::fs::canonicalize(&target).ok() == std::fs::canonicalize(path).ok();
    if !same_file {
        ensure_free(&target)?;
    }
    std::fs::rename(path, &target)?;
    Ok(target)
}

/// Move a file or folder to the system trash.
pub fn delete(path: &Path) -> anyhow::Result<()> {
    // In a Flatpak the trash folder Aster sees is its own sandboxed one; ask the desktop's
    // trash portal instead, so it lands in the user's real trash.
    #[cfg(target_os = "linux")]
    if std::env::var_os("FLATPAK_ID").is_some() {
        let file = std::fs::File::open(path)?;
        return futures::executor::block_on(ashpd::desktop::trash::trash_file(&file))
            .map_err(|e| anyhow::anyhow!("{e}"));
    }
    trash::delete(path).map_err(|e| anyhow::anyhow!("{e}"))
}

/// Where `path` ends up after `from` was renamed to `to` (if it was `from` or inside it).
pub fn remap(path: &Path, from: &Path, to: &Path) -> Option<PathBuf> {
    path.strip_prefix(from).ok().map(|rest| {
        if rest.as_os_str().is_empty() {
            to.to_path_buf()
        } else {
            to.join(rest)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "aster-files-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn creates_and_renames() {
        let dir = temp();
        let note = new_note(&dir, "Ideas").unwrap();
        assert_eq!(note, dir.join("Ideas.org"));
        assert_eq!(
            std::fs::read_to_string(&note).unwrap(),
            "#+TITLE: Ideas\n\n"
        );
        assert!(new_note(&dir, "Ideas.org").is_err(), "exists");
        assert!(new_note(&dir, "a/b").is_err());
        assert!(new_note(&dir, "  ").is_err());
        assert!(new_note(&dir, ".hidden").is_err());

        let folder = new_folder(&dir, "Projects").unwrap();
        assert!(folder.is_dir());
        let renamed = rename(&note, "Better ideas").unwrap();
        assert_eq!(renamed, dir.join("Better ideas.org"));
        assert!(!note.exists() && renamed.exists());
        let moved = rename(&folder, "Work").unwrap();
        assert!(moved.is_dir());
        assert!(rename(&renamed, "Work").is_ok(), "Work.org is free");

        assert_eq!(
            remap(
                &dir.join("Work/a.org"),
                &dir.join("Work"),
                &dir.join("Jobs")
            ),
            Some(dir.join("Jobs/a.org"))
        );
        assert_eq!(
            remap(&dir.join("x.org"), &dir.join("Work"), &dir.join("J")),
            None
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
