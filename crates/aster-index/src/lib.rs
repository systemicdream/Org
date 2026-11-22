//! The workspace index: every `.org` file under a folder, with titles, headings, tags and
//! IDs, plus full-text search and fuzzy file matching.
//!
//! The index lives in memory and is rebuilt from the files (see ADR 0004); nothing is written
//! into the user's notes folder.

use std::collections::BTreeMap;
use std::ops::Range;
use std::path::{Path, PathBuf};

use aster_org::OrgTree;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadingEntry {
    pub title: String,
    pub level: usize,
    pub todo: Option<String>,
    pub done: bool,
    pub tags: Vec<String>,
    /// The `:ID:` property, if any.
    pub id: Option<String>,
    /// Byte offset of the heading line.
    pub offset: usize,
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    /// Path relative to the workspace root, with `/` separators.
    pub rel: String,
    /// `#+TITLE:` if present.
    pub title: Option<String>,
    /// `#+FILETAGS:` tags.
    pub file_tags: Vec<String>,
    pub headings: Vec<HeadingEntry>,
    pub text: String,
    /// A sync tool's conflict copy (Dropbox, Syncthing, Nextcloud…).
    pub conflict: bool,
}

impl FileEntry {
    /// Name to show: the title, or the file name without extension.
    pub fn display_name(&self) -> String {
        self.title.clone().unwrap_or_else(|| {
            Path::new(&self.rel)
                .file_stem()
                .map_or(self.rel.clone(), |s| s.to_string_lossy().into_owned())
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub path: PathBuf,
    pub rel: String,
    /// Byte offset of the match in the file.
    pub offset: usize,
    /// 1-based line number.
    pub line: usize,
    /// The line's text (trimmed) and the match range within it.
    pub snippet: String,
    pub snippet_match: Range<usize>,
}

/// Whether a file name looks like a sync tool's conflict copy.
pub fn is_conflict_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("conflicted copy")
        || lower.contains(".sync-conflict-")
        || lower.contains("(conflict")
        || lower.contains("-conflict-")
}

pub fn is_org_file(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("org"))
        && !path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'))
}

/// Folders never scanned.
fn skip_dir(name: &str) -> bool {
    name.starts_with('.') || name == "node_modules" || name == "target"
}

/// Subsequence fuzzy score of `text` against `query`; lower is better, `None` if no match.
/// Prefix and contiguous matches score best.
pub fn fuzzy_score(text: &str, query: &str) -> Option<usize> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Some(0);
    }
    let lower = text.to_lowercase();
    if lower.starts_with(&query) {
        return Some(0);
    }
    if let Some(i) = lower.find(&query) {
        return Some(1 + i.min(50));
    }
    let chars: Vec<char> = lower.chars().collect();
    let mut pos = 0;
    let mut penalty = 60;
    for q in query.chars().filter(|c| !c.is_whitespace()) {
        let found = chars[pos..].iter().position(|&c| c == q)?;
        let at = pos + found;
        let word_start = at == 0 || !chars[at - 1].is_alphanumeric();
        penalty += found + if word_start { 0 } else { 2 };
        pos = at + 1;
    }
    Some(penalty)
}

fn parse_keyword<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|l| {
        let l = l.trim_start();
        let (k, v) = l.split_once(':')?;
        k.eq_ignore_ascii_case(&format!("#+{key}"))
            .then(|| v.trim())
    })
}

/// Parse a file's text into its index entry.
pub fn index_text(path: &Path, rel: String, text: String) -> FileEntry {
    let tree = OrgTree::parse(&text);
    let headings = tree
        .headings()
        .into_iter()
        .map(|h| {
            // The :ID: property lives in the heading's own section, before any child heading.
            let section_end = text[h.line_end..h.range.end]
                .find("\n*")
                .map_or(h.range.end, |i| h.line_end + i + 1);
            let id = text[h.line_end..section_end].lines().find_map(|l| {
                let l = l.trim();
                l.get(..4)
                    .filter(|p| p.eq_ignore_ascii_case(":id:"))
                    .map(|_| l[4..].trim().to_string())
                    .filter(|v| !v.is_empty())
            });
            HeadingEntry {
                title: h.title,
                level: h.level,
                todo: h.todo,
                done: h.done,
                tags: h.tags,
                id,
                offset: h.range.start,
            }
        })
        .collect();
    let title = parse_keyword(&text, "title").map(str::to_string);
    let file_tags = parse_keyword(&text, "filetags")
        .map(|v| {
            v.split([':', ' '])
                .filter(|t| !t.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let conflict = path
        .file_name()
        .is_some_and(|n| is_conflict_file(&n.to_string_lossy()));
    FileEntry {
        path: path.to_path_buf(),
        rel,
        title,
        file_tags,
        headings,
        text,
        conflict,
    }
}

#[derive(Debug, Default)]
pub struct Index {
    root: PathBuf,
    files: BTreeMap<PathBuf, FileEntry>,
}

impl Index {
    /// Scan every `.org` file under `root`.
    pub fn scan(root: &Path) -> Self {
        let mut index = Self {
            root: root.to_path_buf(),
            files: BTreeMap::new(),
        };
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if kind.is_dir() {
                    if !skip_dir(&entry.file_name().to_string_lossy()) {
                        stack.push(path);
                    }
                } else if is_org_file(&path) {
                    index.update_file(&path);
                }
            }
        }
        index
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn rel(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/")
    }

    /// Whether `path` belongs in this index (an `.org` file under the root, not in a skipped
    /// folder).
    pub fn covers(&self, path: &Path) -> bool {
        is_org_file(path)
            && path.starts_with(&self.root)
            && path
                .strip_prefix(&self.root)
                .map(|rel| {
                    rel.parent().is_none_or(|p| {
                        p.components()
                            .all(|c| !skip_dir(&c.as_os_str().to_string_lossy()))
                    })
                })
                .unwrap_or(false)
    }

    /// Re-read a file from disk (removing it if it's gone). Returns whether the index changed.
    pub fn update_file(&mut self, path: &Path) -> bool {
        match std::fs::read(path) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => self.upsert_text(path, text),
                Err(_) => false,
            },
            Err(_) => self.files.remove(path).is_some(),
        }
    }

    /// Index `text` as the contents of `path` (e.g. unsaved text from the editor).
    pub fn upsert_text(&mut self, path: &Path, text: String) -> bool {
        if self.files.get(path).is_some_and(|f| f.text == text) {
            return false;
        }
        let entry = index_text(path, self.rel(path), text);
        self.files.insert(path.to_path_buf(), entry);
        true
    }

    pub fn remove_file(&mut self, path: &Path) -> bool {
        self.files.remove(path).is_some()
    }

    pub fn files(&self) -> impl Iterator<Item = &FileEntry> {
        self.files.values()
    }

    pub fn file(&self, path: &Path) -> Option<&FileEntry> {
        self.files.get(path)
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// The file and heading offset carrying `:ID: id`.
    pub fn find_id(&self, id: &str) -> Option<(PathBuf, usize)> {
        let id = id.trim();
        self.files.values().find_map(|f| {
            f.headings
                .iter()
                .find(|h| h.id.as_deref() == Some(id))
                .map(|h| (f.path.clone(), h.offset))
        })
    }

    /// Offset of the first heading in `path` titled `title`.
    pub fn find_heading(&self, path: &Path, title: &str) -> Option<usize> {
        self.files
            .get(path)?
            .headings
            .iter()
            .find(|h| h.title == title.trim())
            .map(|h| h.offset)
    }

    /// Every tag with how many headings (or files, for file tags) carry it.
    pub fn tags(&self) -> BTreeMap<String, usize> {
        let mut out = BTreeMap::new();
        for f in self.files.values() {
            for t in &f.file_tags {
                *out.entry(t.clone()).or_default() += 1;
            }
            for h in &f.headings {
                for t in &h.tags {
                    *out.entry(t.clone()).or_default() += 1;
                }
            }
        }
        out
    }

    /// Headings tagged `tag` (directly or through `#+FILETAGS`).
    pub fn headings_with_tag(&self, tag: &str) -> Vec<(&FileEntry, &HeadingEntry)> {
        let mut out = Vec::new();
        for f in self.files.values() {
            let file_tagged = f.file_tags.iter().any(|t| t == tag);
            for h in &f.headings {
                if file_tagged || h.tags.iter().any(|t| t == tag) {
                    out.push((f, h));
                }
            }
        }
        out
    }

    /// Files ranked by fuzzy match on their path and title.
    pub fn match_files(&self, query: &str) -> Vec<&FileEntry> {
        let mut scored: Vec<(usize, &FileEntry)> = self
            .files
            .values()
            .filter_map(|f| {
                let by_title = f.title.as_deref().and_then(|t| fuzzy_score(t, query));
                let by_path = fuzzy_score(&f.rel, query);
                let by_name = Path::new(&f.rel)
                    .file_name()
                    .and_then(|n| fuzzy_score(&n.to_string_lossy(), query));
                [by_title, by_path, by_name]
                    .into_iter()
                    .flatten()
                    .min()
                    .map(|s| (s, f))
            })
            .collect();
        scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.rel.cmp(&b.1.rel)));
        scored.into_iter().map(|(_, f)| f).collect()
    }

    /// Lines containing `query` across all files (smart case: case-insensitive unless the
    /// query has uppercase), at most `limit`.
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchHit> {
        let query = query.trim();
        if query.is_empty() {
            return Vec::new();
        }
        let case_sensitive = query.chars().any(char::is_uppercase);
        let needle = if case_sensitive {
            query.to_string()
        } else {
            query.to_lowercase()
        };
        let mut hits = Vec::new();
        for f in self.files.values() {
            let mut offset = 0;
            for (n, line) in f.text.split_inclusive('\n').enumerate() {
                let hay = if case_sensitive {
                    line.to_string()
                } else {
                    line.to_lowercase()
                };
                // Lowercasing can change byte lengths for some scripts; only map positions
                // back when they line up.
                if let Some(i) = hay.find(&needle)
                    && hay.len() == line.len()
                {
                    let trimmed_start = line.len() - line.trim_start().len();
                    let snippet = line.trim().to_string();
                    let start = i.saturating_sub(trimmed_start).min(snippet.len());
                    let end = (start + needle.len()).min(snippet.len());
                    hits.push(SearchHit {
                        path: f.path.clone(),
                        rel: f.rel.clone(),
                        offset: offset + i,
                        line: n + 1,
                        snippet,
                        snippet_match: start..end,
                    });
                    if hits.len() >= limit {
                        return hits;
                    }
                }
                offset += line.len();
            }
        }
        hits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflict_names() {
        assert!(is_conflict_file(
            "notes (Ann's conflicted copy 2026-11-22).org"
        ));
        assert!(is_conflict_file(
            "notes.sync-conflict-20261122-101010-ABCDEF.org"
        ));
        assert!(is_conflict_file("notes (conflict 2026-11-22).org"));
        assert!(!is_conflict_file("conflicts-of-interest.org"));
    }

    #[test]
    fn fuzzy() {
        assert_eq!(fuzzy_score("projects/aster.org", ""), Some(0));
        assert!(
            fuzzy_score("projects/aster.org", "aster") < fuzzy_score("projects/aster.org", "pa")
        );
        assert!(fuzzy_score("projects/aster.org", "pjas").is_some());
        assert!(fuzzy_score("projects/aster.org", "zz").is_none());
    }

    #[test]
    fn indexes_text() {
        let text = "#+TITLE: My Notes\n#+FILETAGS: :home:\n* TODO Task :work:\n:PROPERTIES:\n:ID: abc-1\n:END:\n** Child\n:PROPERTIES:\n:ID: child-2\n:END:\n";
        let f = index_text(Path::new("/x/n.org"), "n.org".into(), text.into());
        assert_eq!(f.title.as_deref(), Some("My Notes"));
        assert_eq!(f.file_tags, ["home"]);
        assert_eq!(f.headings.len(), 2);
        assert_eq!(f.headings[0].id.as_deref(), Some("abc-1"));
        assert_eq!(f.headings[0].tags, ["work"]);
        assert_eq!(f.headings[1].id.as_deref(), Some("child-2"));
        assert_eq!(f.display_name(), "My Notes");
    }
}
