//! Releases: the running version, update notifications and crash reports.
//!
//! - **Updates:** at most once a day Org asks GitHub Releases for the latest version and, if
//!   it's newer, offers a link to it. There's no in-place updater.
//! - **Crashes:** a panic writes a report to the state folder. The next launch offers to open
//!   a prefilled GitHub issue with it; nothing is sent automatically.
//!
//! Both keep their files in [`state_dir`] (`$ORG_STATE_DIR` overrides it, for tests).

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REPO_URL: &str = "https://github.com/systemicdream/Org";
pub const RELEASES_URL: &str = "https://github.com/systemicdream/Org/releases";
const LATEST_API: &str = "https://api.github.com/repos/systemicdream/Org/releases/latest";

/// How often the automatic update check runs.
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
/// Crash reports kept on disk.
const KEEP_CRASHES: usize = 10;

/// Where Org keeps state that isn't settings: update-check results and crash reports.
pub fn state_dir() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("ORG_STATE_DIR") {
        return Some(PathBuf::from(p));
    }
    Some(dirs::data_local_dir()?.join("org"))
}

// ---- versions -------------------------------------------------------------------------

/// `major.minor.patch`, and whether it's a full release (no `-beta.1` style suffix).
fn parse_version(v: &str) -> Option<(u64, u64, u64, bool)> {
    let v = v.trim().trim_start_matches(['v', 'V']);
    // `+build` metadata doesn't affect ordering; a `-pre` suffix sorts before the release.
    let v = v.split('+').next().unwrap_or(v);
    let (core, pre) = match v.split_once('-') {
        Some((core, _)) => (core, true),
        None => (v, false),
    };
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let major = parts.next()??;
    let minor = parts.next().unwrap_or(Some(0))?;
    let patch = parts.next().unwrap_or(Some(0))?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch, !pre))
}

/// Whether `latest` is a newer version than `current` (unparseable versions never are).
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

// ---- update check -----------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Update {
    /// Without a leading `v`.
    pub version: String,
    /// The release page.
    pub url: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct UpdateState {
    /// Unix seconds of the last successful check.
    last_check: Option<u64>,
    /// The newest release the last check found.
    latest: Option<Update>,
    /// A version whose notification the user closed.
    dismissed: Option<String>,
}

fn update_state_path() -> Option<PathBuf> {
    Some(state_dir()?.join("update-check.json"))
}

fn load_update_state() -> UpdateState {
    update_state_path()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_update_state(state: &UpdateState) {
    let Some(path) = update_state_path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = crate::workspace::write_atomic(&path, &json);
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Ask GitHub for the latest release. `Ok(None)`: there's no published release yet.
fn fetch_latest() -> anyhow::Result<Option<Update>> {
    // For testing the notification without a release: ORG_FAKE_LATEST=0.2.0.
    if let Ok(v) = std::env::var("ORG_FAKE_LATEST") {
        return Ok(Some(Update {
            version: v.trim_start_matches('v').to_string(),
            url: RELEASES_URL.to_string(),
        }));
    }
    #[derive(Deserialize)]
    struct Release {
        tag_name: String,
        html_url: String,
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .into();
    let response = agent
        .get(LATEST_API)
        // Only the app name and version; nothing about the user.
        .header("User-Agent", &format!("Org/{VERSION}"))
        .header("Accept", "application/vnd.github+json")
        .call();
    let mut response = match response {
        Ok(r) => r,
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    let body = response.body_mut().read_to_string()?;
    let release: Release = serde_json::from_str(&body)?;
    Ok(Some(Update {
        version: release.tag_name.trim_start_matches('v').to_string(),
        url: release.html_url,
    }))
}

/// The result of an update check.
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateStatus {
    UpToDate,
    Available(Update),
}

/// Check for a newer release. Blocking: run it off the main thread.
///
/// The automatic check (`manual == false`) reuses a result less than a day old, and stays
/// quiet about a version the user dismissed. A manual check always asks GitHub.
pub fn check_for_update(manual: bool) -> anyhow::Result<UpdateStatus> {
    let mut state = load_update_state();
    let fresh = state
        .last_check
        .is_some_and(|t| now_secs().saturating_sub(t) < CHECK_INTERVAL.as_secs());
    if manual || !fresh {
        state.latest = fetch_latest()?;
        state.last_check = Some(now_secs());
        save_update_state(&state);
    }
    Ok(match state.latest {
        Some(update)
            if is_newer(&update.version, VERSION)
                && (manual || state.dismissed.as_deref() != Some(update.version.as_str())) =>
        {
            UpdateStatus::Available(update)
        }
        _ => UpdateStatus::UpToDate,
    })
}

/// Don't notify about `version` again (a manual check still reports it).
pub fn dismiss_update(version: &str) {
    let mut state = load_update_state();
    state.dismissed = Some(version.to_string());
    save_update_state(&state);
}

// ---- crash reports ----------------------------------------------------------------------

fn crash_dir() -> Option<PathBuf> {
    Some(state_dir()?.join("crashes"))
}

/// A short description of the system, for bug reports.
fn system_info() -> String {
    let mut os = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
    #[cfg(target_os = "linux")]
    {
        if let Ok(release) = std::fs::read_to_string("/etc/os-release")
            && let Some(name) = release.lines().find_map(|l| l.strip_prefix("PRETTY_NAME="))
        {
            os = format!("{} ({os})", name.trim_matches('"'));
        }
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        if !desktop.is_empty() {
            os.push_str(&format!(", {desktop}"));
        }
        if std::env::var_os("FLATPAK_ID").is_some() {
            os.push_str(", Flatpak");
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = std::process::Command::new("sw_vers")
            .arg("-productVersion")
            .output()
        {
            os = format!(
                "macOS {} ({os})",
                String::from_utf8_lossy(&out.stdout).trim()
            );
        }
    }
    os
}

/// Write a crash report when Org panics (then let the default hook print it as usual).
pub fn install_panic_hook() {
    // Gather this up front: the panic hook should do as little as possible.
    let system = system_info();
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let report = format!(
            "Org {VERSION}\nSystem: {system}\nTime: {}\nThread: {}\n\n{info}\n\nBacktrace:\n{}\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S %z"),
            thread.name().unwrap_or("unnamed"),
            std::backtrace::Backtrace::force_capture(),
        );
        if let Some(dir) = crash_dir()
            && std::fs::create_dir_all(&dir).is_ok()
        {
            let name = format!("crash-{}.txt", chrono::Local::now().format("%Y%m%d-%H%M%S"));
            let _ = std::fs::write(dir.join(name), &report);
        }
        default_hook(info);
    }));
}

/// The newest crash report the user hasn't seen yet. Also prunes old reports.
pub fn pending_crash() -> Option<PathBuf> {
    let dir = crash_dir()?;
    let mut reports: Vec<PathBuf> = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("crash-") && n.ends_with(".txt"))
        })
        .collect();
    reports.sort();
    while reports.len() > KEEP_CRASHES {
        let _ = std::fs::remove_file(reports.remove(0));
    }
    let newest = reports.pop()?;
    let seen = std::fs::read_to_string(dir.join("last-seen")).unwrap_or_default();
    let name = newest.file_name()?.to_str()?;
    (name > seen.trim()).then_some(newest)
}

/// Don't offer this report (or older ones) again.
pub fn mark_crash_seen(report: &Path) {
    if let (Some(dir), Some(name)) = (crash_dir(), report.file_name()) {
        let _ = std::fs::write(dir.join("last-seen"), name.to_string_lossy().as_bytes());
    }
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3 / 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn encoded_byte_len(b: u8) -> usize {
    match b {
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => 1,
        _ => 3,
    }
}

/// Truncate `s` on a UTF-8 boundary so `percent_encode(prefix).len() <= max_encoded`.
fn truncate_for_url(s: &str, max_encoded: usize) -> (&str, bool) {
    let mut encoded = 0;
    let mut cut = 0;
    for (idx, ch) in s.char_indices() {
        let len: usize = ch
            .encode_utf8(&mut [0; 4])
            .bytes()
            .map(encoded_byte_len)
            .sum();
        if encoded + len > max_encoded {
            return (&s[..cut], true);
        }
        encoded += len;
        cut = idx + ch.len_utf8();
    }
    (s, false)
}

/// A "new issue" link with the report filled in, kept under GitHub's ~8 KB URL limit.
pub fn crash_issue_url(report: &str) -> String {
    const MAX_URL_LEN: usize = 7500;
    const SHORTENED_NOTE: &str = "\n… (shortened: the full report is on your computer)";

    let report = report.trim();
    let summary = report
        .lines()
        .find_map(|l| {
            l.strip_prefix("panicked at ")
                .or(l.contains("panicked").then_some(l))
        })
        .map(|l| l.chars().take(80).collect::<String>())
        .unwrap_or_else(|| "Org crashed".into());
    let title = format!("Crash: {summary}");
    let prefix = format!(
        "{REPO_URL}/issues/new?title={}&body=",
        percent_encode(&title)
    );
    let body_head =
        "**What were you doing when Org crashed?**\n\n\n\n**Crash report**\n\n```text\n";
    let body_tail = "\n```\n";
    let fixed_len =
        prefix.len() + percent_encode(body_head).len() + percent_encode(body_tail).len();
    let budget = MAX_URL_LEN.saturating_sub(fixed_len);

    let full_encoded = percent_encode(report);
    let body = if full_encoded.len() <= budget {
        format!("{body_head}{report}{body_tail}")
    } else {
        let note_len = percent_encode(SHORTENED_NOTE).len();
        let (head, _) = truncate_for_url(report, budget.saturating_sub(note_len));
        format!("{body_head}{head}{SHORTENED_NOTE}{body_tail}")
    };
    format!("{prefix}{}", percent_encode(&body))
}

/// A "new issue" link for reporting a problem, with the version and system filled in.
pub fn issue_url() -> String {
    let body = format!(
        "**What happened?**\n\n\n\n**What did you expect?**\n\n\n\nOrg {VERSION}, {}\n",
        system_info()
    );
    format!("{REPO_URL}/issues/new?body={}", percent_encode(&body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(is_newer("1.0", "0.9.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("v0.1.0", "0.2.0"));
        // A pre-release comes before its release.
        assert!(is_newer("0.2.0", "0.2.0-beta.1"));
        assert!(!is_newer("0.2.0-beta.1", "0.2.0"));
        assert!(is_newer("0.2.0-beta.1", "0.1.0"));
        // Build metadata doesn't make a pre-release.
        assert!(!is_newer("0.1.0+build.5", "0.1.0"));
        assert!(!is_newer("garbage", "0.1.0"));
        assert!(!is_newer("0.2.0", "garbage"));
    }

    #[test]
    fn builds_issue_links() {
        let report = "Org 0.1.0\nSystem: linux\n\npanicked at src/x.rs:1:2:\noops & more\n";
        let url = crash_issue_url(report);
        assert!(url.starts_with(
            "https://github.com/systemicdream/Org/issues/new?title=Crash%3A%20src%2Fx.rs"
        ));
        assert!(url.contains("oops%20%26%20more"));
        assert!(!url.contains(' '));
        let long = "é".repeat(10_000);
        let long_url = crash_issue_url(&long);
        assert!(long_url.len() <= 7500, "len={}", long_url.len());
        let backtrace =
            "  12: org::workspace::Workspace::render (src/workspace.rs:100:5)\n".repeat(200);
        let bt_url = crash_issue_url(&backtrace);
        assert!(bt_url.len() <= 7500, "len={}", bt_url.len());
        assert!(bt_url.contains("shortened"));
    }

    #[test]
    fn tracks_unseen_crashes() {
        let dir = std::env::temp_dir().join(format!("org-crash-{}", std::process::id()));
        let crashes = dir.join("crashes");
        std::fs::create_dir_all(&crashes).unwrap();
        // SAFETY: tests in this module don't otherwise read ORG_STATE_DIR concurrently.
        unsafe { std::env::set_var("ORG_STATE_DIR", &dir) };
        assert_eq!(pending_crash(), None);
        std::fs::write(crashes.join("crash-20260101-000000.txt"), "a").unwrap();
        std::fs::write(crashes.join("crash-20260102-000000.txt"), "b").unwrap();
        let newest = pending_crash().unwrap();
        assert!(newest.ends_with("crash-20260102-000000.txt"));
        mark_crash_seen(&newest);
        assert_eq!(pending_crash(), None);
        std::fs::write(crashes.join("crash-20260103-000000.txt"), "c").unwrap();
        assert!(pending_crash().is_some());
        unsafe { std::env::remove_var("ORG_STATE_DIR") };
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
