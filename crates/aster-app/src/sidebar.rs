//! The sidebar: the workspace's file tree and its tags.

use std::collections::BTreeSet;
use std::path::PathBuf;

use aster_ui::{ActiveTheme, components, ramp};
use gpui::{Context, MouseButton, SharedString, div, prelude::*, px};

use crate::agenda_view::Mode;

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

fn view_row(
    id: &'static str,
    label: &'static str,
    selected: bool,
    cx: &mut Context<Workspace>,
    on_click: fn(&mut Workspace, &mut gpui::Window, &mut Context<Workspace>),
) -> impl IntoElement {
    components::list_item(id, selected, cx)
        .child(label)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, _, window, cx| on_click(ws, window, cx)),
        )
}

impl Workspace {
    /// The navigation pane (docs/DESIGN.md §7, §11): views, the file tree and tags, on the base
    /// layer, with Fluent list rows and the accent selection indicator.
    pub(crate) fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let current_view = self.current_view(cx);
        let views = [
            view_row(
                "view-agenda",
                "Agenda",
                matches!(current_view, Some(Mode::Day | Mode::Week | Mode::Search)),
                cx,
                |ws, w, cx| ws.show_agenda_mode(Mode::Day, w, cx),
            )
            .into_any_element(),
            view_row(
                "view-todo",
                "To-do list",
                current_view == Some(Mode::Todo),
                cx,
                |ws, w, cx| ws.show_agenda_mode(Mode::Todo, w, cx),
            )
            .into_any_element(),
        ];
        let colors = cx.theme().colors;
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
        let current = self.path.clone().filter(|_| current_view.is_none());

        let file_rows: Vec<_> = rows
            .into_iter()
            .enumerate()
            .map(|(i, row)| {
                let (chevron, selected, color) = match &row.kind {
                    TreeKind::Folder { open, .. } => {
                        (if *open { "⌄" } else { "›" }, false, colors.text_primary)
                    }
                    TreeKind::File { path, conflict } => (
                        "",
                        current.as_ref() == Some(path),
                        if *conflict {
                            colors.caution
                        } else {
                            colors.text_primary
                        },
                    ),
                };
                let conflict = matches!(row.kind, TreeKind::File { conflict: true, .. });
                let name = match &row.kind {
                    TreeKind::File { .. } => row.name.trim_end_matches(".org").to_string(),
                    TreeKind::Folder { .. } => row.name.clone(),
                };
                let kind = row.kind.clone();
                components::list_item(("tree-row", i), selected, cx)
                    .pl(px(12. + 16. * row.depth as f32))
                    .text_color(color)
                    .child(
                        div()
                            .w(px(12.))
                            .flex_shrink_0()
                            .text_color(colors.text_secondary)
                            .child(chevron),
                    )
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(name),
                    )
                    .when(conflict, |d| {
                        d.child(components::caption("conflict", cx).text_color(colors.caution))
                    })
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
            })
            .collect();

        let tag_rows: Vec<_> = tags
            .iter()
            .enumerate()
            .map(|(i, (tag, count))| {
                let t = tag.clone();
                components::list_item(("tag-row", i), false, cx)
                    .justify_between()
                    .child(
                        div()
                            .text_color(colors.text_secondary)
                            .child(format!("#{tag}")),
                    )
                    .child(components::caption(count.to_string(), cx))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| this.show_tag(&t, window, cx)),
                    )
            })
            .collect();

        let empty = index.is_empty();
        let has_tags = !tags.is_empty();
        div()
            .id("sidebar")
            .w(px(260.))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .text_size(ramp::BODY.size)
            .line_height(ramp::BODY.line_height)
            .child(
                div()
                    .px_4()
                    .pb_1()
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
                    .px_1()
                    .pb_4()
                    .child(components::section_header("Views", cx))
                    .children(views)
                    .child(components::section_header("Files", cx))
                    .when(empty, |d| {
                        d.child(div().px_3().text_color(colors.text_tertiary).child(
                            if self.scanning {
                                "Loading…"
                            } else {
                                "No .org files"
                            },
                        ))
                    })
                    .children(file_rows)
                    .when(has_tags, |d| {
                        d.child(components::section_header("Tags", cx))
                    })
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
