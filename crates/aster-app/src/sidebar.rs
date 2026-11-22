//! The sidebar: the workspace's file tree and its tags.

use std::collections::BTreeSet;
use std::path::PathBuf;

use gpui::{Context, MouseButton, SharedString, div, prelude::*, px, rgb};

use crate::workspace::Workspace;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeKind {
    Folder { rel: String, open: bool },
    File { path: PathBuf, conflict: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeRow {
    pub depth: usize,
    pub name: String,
    pub kind: TreeKind,
}

/// Flatten files (relative paths with `/`) into visible tree rows: folders first, then files,
/// each sorted case-insensitively; children of closed folders are omitted.
pub fn build_tree(files: &[(String, PathBuf, bool)], expanded: &BTreeSet<String>) -> Vec<TreeRow> {
    fn walk(
        prefix: &str,
        depth: usize,
        files: &[(String, PathBuf, bool)],
        expanded: &BTreeSet<String>,
        out: &mut Vec<TreeRow>,
    ) {
        let mut folders: BTreeSet<(String, String)> = BTreeSet::new();
        let mut leaves: Vec<(String, PathBuf, bool)> = Vec::new();
        for (rel, path, conflict) in files {
            let Some(rest) = rel.strip_prefix(prefix) else {
                continue;
            };
            match rest.split_once('/') {
                Some((folder, _)) => {
                    folders.insert((folder.to_lowercase(), folder.to_string()));
                }
                None => leaves.push((rest.to_string(), path.clone(), *conflict)),
            }
        }
        for (_, folder) in folders {
            let rel = format!("{prefix}{folder}");
            let open = expanded.contains(&rel);
            out.push(TreeRow {
                depth,
                name: folder.clone(),
                kind: TreeKind::Folder {
                    rel: rel.clone(),
                    open,
                },
            });
            if open {
                walk(&format!("{rel}/"), depth + 1, files, expanded, out);
            }
        }
        leaves.sort_by_key(|(name, ..)| name.to_lowercase());
        for (name, path, conflict) in leaves {
            out.push(TreeRow {
                depth,
                name,
                kind: TreeKind::File { path, conflict },
            });
        }
    }
    let mut out = Vec::new();
    walk("", 0, files, expanded, &mut out);
    out
}

fn section_label(text: &'static str) -> impl IntoElement {
    div()
        .px_3()
        .pt_3()
        .pb_1()
        .text_size(px(11.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(rgb(0x9ca3af))
        .child(text)
}

impl Workspace {
    pub(crate) fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let index = self.index.read(cx);
        let files: Vec<(String, PathBuf, bool)> = index
            .files()
            .map(|f| (f.rel.clone(), f.path.clone(), f.conflict))
            .collect();
        let rows = build_tree(&files, &self.expanded);
        let tags = index.tags();
        let root_name = self
            .root
            .as_ref()
            .and_then(|r| r.file_name())
            .map_or("Notes".to_string(), |n| n.to_string_lossy().into_owned());
        let current = self.path.clone();

        let file_rows = rows.into_iter().enumerate().map(|(i, row)| {
            let indent = px(12. + 14. * row.depth as f32);
            let (icon, color, selected) = match &row.kind {
                TreeKind::Folder { open, .. } => {
                    (if *open { "▾" } else { "▸" }, rgb(0x6b7280), false)
                }
                TreeKind::File { path, conflict } => (
                    if *conflict { "⚠" } else { " " },
                    if *conflict {
                        rgb(0xd97706)
                    } else {
                        rgb(0x374151)
                    },
                    current.as_ref() == Some(path),
                ),
            };
            let name = match &row.kind {
                TreeKind::File { .. } => row.name.trim_end_matches(".org").to_string(),
                TreeKind::Folder { .. } => row.name.clone(),
            };
            let kind = row.kind.clone();
            div()
                .id(("tree-row", i))
                .pl(indent)
                .pr_2()
                .mx_1()
                .rounded_md()
                .flex()
                .gap_1()
                .cursor_pointer()
                .text_color(color)
                .when(selected, |d| d.bg(rgb(0xe0e7ff)).text_color(rgb(0x1e3a8a)))
                .when(!selected, |d| d.hover(|d| d.bg(rgb(0xe5e7eb))))
                .child(div().w(px(12.)).text_color(rgb(0x9ca3af)).child(icon))
                .child(div().overflow_hidden().whitespace_nowrap().child(name))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| match &kind {
                        TreeKind::Folder { rel, .. } => {
                            if !this.expanded.remove(rel) {
                                this.expanded.insert(rel.clone());
                            }
                            cx.notify();
                        }
                        TreeKind::File { path, .. } => {
                            this.open_path(path.clone(), window, cx);
                        }
                    }),
                )
        });

        let tag_rows = tags.into_iter().enumerate().map(|(i, (tag, count))| {
            let t = tag.clone();
            div()
                .id(("tag-row", i))
                .px_3()
                .mx_1()
                .rounded_md()
                .flex()
                .justify_between()
                .cursor_pointer()
                .hover(|d| d.bg(rgb(0xe5e7eb)))
                .child(div().text_color(rgb(0x4b5563)).child(format!("#{tag}")))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(0x9ca3af))
                        .child(count.to_string()),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| this.show_tag(&t, window, cx)),
                )
        });

        let empty = index.is_empty();
        div()
            .id("sidebar")
            .w(px(240.))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .bg(rgb(0xf9fafb))
            .border_r_1()
            .border_color(rgb(0xe5e7eb))
            .text_size(px(14.))
            .line_height(px(24.))
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(0xe5e7eb))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(SharedString::from(root_name)),
            )
            .child(
                div()
                    .id("sidebar-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .pb_4()
                    .child(section_label("FILES"))
                    .when(empty, |d| {
                        d.child(
                            div()
                                .px_3()
                                .text_color(rgb(0x9ca3af))
                                .child(if self.scanning {
                                    "Loading…"
                                } else {
                                    "No .org files"
                                }),
                        )
                    })
                    .children(file_rows)
                    .when(!index.tags().is_empty(), |d| d.child(section_label("TAGS")))
                    .children(tag_rows),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(rels: &[&str]) -> Vec<(String, PathBuf, bool)> {
        rels.iter()
            .map(|r| (r.to_string(), PathBuf::from(format!("/w/{r}")), false))
            .collect()
    }

    #[test]
    fn tree_folders_first_and_collapsed() {
        let f = files(&[
            "zeta.org",
            "Projects/aster.org",
            "Projects/sub/deep.org",
            "alpha.org",
        ]);
        let rows = build_tree(&f, &BTreeSet::new());
        let names: Vec<_> = rows.iter().map(|r| (r.depth, r.name.as_str())).collect();
        assert_eq!(names, [(0, "Projects"), (0, "alpha.org"), (0, "zeta.org")]);

        let expanded: BTreeSet<String> = ["Projects".into()].into();
        let rows = build_tree(&f, &expanded);
        let names: Vec<_> = rows.iter().map(|r| (r.depth, r.name.as_str())).collect();
        assert_eq!(
            names,
            [
                (0, "Projects"),
                (1, "sub"),
                (1, "aster.org"),
                (0, "alpha.org"),
                (0, "zeta.org")
            ]
        );
    }
}
