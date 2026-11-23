//! Scanning a real folder: nested files, skipped folders, conflict copies, search, IDs, tags.

use std::fs;
use std::path::PathBuf;

use org_index::Index;

fn temp_workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("org-index-test-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("projects")).unwrap();
    fs::create_dir_all(dir.join(".git")).unwrap();
    fs::write(
        dir.join("inbox.org"),
        "#+TITLE: Inbox\n* TODO Buy milk :errand:\n* Call Ann about the Garden plan\n",
    )
    .unwrap();
    fs::write(
        dir.join("projects/garden.org"),
        "#+FILETAGS: :work:\n* Garden\n:PROPERTIES:\n:ID: garden-id\n:END:\n** Live preview :ui:\nHide markup.\n",
    )
    .unwrap();
    fs::write(dir.join("projects/notes.txt"), "not org").unwrap();
    fs::write(dir.join(".git/ignored.org"), "* hidden").unwrap();
    fs::write(
        dir.join("inbox.sync-conflict-20261122-101010-ABCDEF.org"),
        "* conflict\n",
    )
    .unwrap();
    dir
}

#[test]
fn scan_search_ids_tags() {
    let dir = temp_workspace("scan");
    let mut index = Index::scan(&dir);
    let mut rels: Vec<_> = index.files().map(|f| f.rel.clone()).collect();
    rels.sort();
    assert_eq!(
        rels,
        [
            "inbox.org",
            "inbox.sync-conflict-20261122-101010-ABCDEF.org",
            "projects/garden.org"
        ]
    );
    assert!(index.files().any(|f| f.conflict));

    // Search is case-insensitive for lowercase queries and reports line and offset.
    let hits = index.search("garden", 10);
    assert_eq!(hits.len(), 3, "{hits:?}");
    let ann = hits.iter().find(|h| h.rel == "inbox.org").unwrap();
    assert_eq!(ann.line, 3);
    assert_eq!(&ann.snippet[ann.snippet_match.clone()], "Garden");
    let text = &index.file(&ann.path).unwrap().text;
    assert_eq!(&text[ann.offset..ann.offset + 6], "Garden");
    assert_eq!(index.search("Garden plan", 10).len(), 1);

    // IDs and tags (FILETAGS apply to every heading in the file).
    let (path, offset) = index.find_id("garden-id").unwrap();
    assert!(path.ends_with("projects/garden.org"));
    assert_eq!(offset, "#+FILETAGS: :work:\n".len());
    let tags = index.tags();
    assert_eq!(tags.get("errand"), Some(&1));
    assert_eq!(index.headings_with_tag("work").len(), 2);

    // Fuzzy file matching uses titles and paths.
    assert_eq!(index.match_files("inbox")[0].rel, "inbox.org");
    assert_eq!(index.match_files("pgd")[0].rel, "projects/garden.org");

    // Updates and removals.
    let inbox = dir.join("inbox.org");
    fs::write(&inbox, "* Nothing left\n").unwrap();
    assert!(index.update_file(&inbox));
    assert!(index.search("milk", 10).is_empty());
    fs::remove_file(&inbox).unwrap();
    assert!(index.update_file(&inbox));
    assert!(index.file(&inbox).is_none());
    assert!(index.covers(&dir.join("projects/new.org")));
    assert!(!index.covers(&dir.join(".git/x.org")));
    assert!(!index.covers(&dir.join("projects/notes.txt")));

    let _ = fs::remove_dir_all(&dir);
}
