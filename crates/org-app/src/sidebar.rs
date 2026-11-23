//! The sidebar: the workspace's file tree and its tags.

use std::collections::BTreeSet;
use std::path::PathBuf;

use gpui::{Context, MouseButton, div, prelude::*, px};
use org_ui::components::ButtonKind;
use org_ui::icons::{Icon, icon};
use org_ui::{ActiveTheme, components, ramp};

use crate::agenda_view::Mode;
use crate::file_actions::TreeTarget;

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

/// Flatten files and folders (relative paths with `/`) into visible tree rows: folders first,
/// then files, each sorted case-insensitively; children of closed folders are omitted.
pub fn build_tree(
    files: &[(String, PathBuf, bool)],
    dirs: &[String],
    expanded: &BTreeSet<String>,
) -> Vec<TreeRow> {
    fn walk(
        prefix: &str,
        depth: usize,
        files: &[(String, PathBuf, bool)],
        dirs: &[String],
        expanded: &BTreeSet<String>,
        out: &mut Vec<TreeRow>,
    ) {
        let mut folders: BTreeSet<(String, String)> = BTreeSet::new();
        for dir in dirs {
            if let Some(folder) = dir
                .strip_prefix(prefix)
                .and_then(|rest| rest.split('/').next())
                .filter(|f| !f.is_empty())
            {
                folders.insert((folder.to_lowercase(), folder.to_string()));
            }
        }
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
                walk(&format!("{rel}/"), depth + 1, files, dirs, expanded, out);
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
    walk("", 0, files, dirs, expanded, &mut out);
    out
}

/// The notes folder's inbox: `inbox.org` at its top level, where captures go by default.
pub(crate) fn is_inbox(rel: &str) -> bool {
    rel.eq_ignore_ascii_case("inbox.org")
}

fn view_row(
    id: &'static str,
    glyph: Icon,
    label: &'static str,
    selected: bool,
    cx: &mut Context<Workspace>,
    on_click: fn(&mut Workspace, &mut gpui::Window, &mut Context<Workspace>),
) -> impl IntoElement {
    let c = cx.theme().colors;
    components::list_item(id, selected, cx)
        .gap_3()
        .child(icon(
            glyph,
            px(16.),
            if selected { c.accent } else { c.text_secondary },
        ))
        .child(label)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, _, window, cx| {
                cx.stop_propagation();
                on_click(ws, window, cx);
            }),
        )
}

impl Workspace {
    /// The sidebar before a notes folder is open (e.g. while looking at the sample): what it
    /// will hold, and how to get there.
    pub(crate) fn render_folderless_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = cx.theme().colors;
        div()
            .id("sidebar")
            .w(px(260.))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_3()
            .px_4()
            .text_size(ramp::BODY.size)
            .line_height(ramp::BODY.line_height)
            .child(
                div()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("No notes folder yet"),
            )
            .child(
                div().text_color(c.text_secondary).child(
                    "Choose a folder for your notes to see its files, tags and agenda here.",
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        components::button(
                            "sidebar-create",
                            "Create a notes folder…",
                            ButtonKind::Accent,
                            cx,
                        )
                        .on_click(
                            cx.listener(|ws, _, window, cx| ws.create_notes_folder(window, cx)),
                        ),
                    )
                    .child(
                        components::button(
                            "sidebar-open",
                            "Open an existing folder…",
                            ButtonKind::Standard,
                            cx,
                        )
                        .on_click(|_, window, cx| {
                            window.dispatch_action(Box::new(crate::OpenFolder), cx)
                        }),
                    ),
            )
    }

    /// The navigation pane (docs/DESIGN.md §7, §11): views, the file tree and tags, on the base
    /// layer, with Fluent list rows and the accent selection indicator.
    pub(crate) fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let current_view = self.current_view(cx);
        let views = [
            view_row(
                "view-agenda",
                Icon::Agenda,
                "Agenda",
                matches!(current_view, Some(Mode::Day | Mode::Week | Mode::Search)),
                cx,
                |ws, w, cx| ws.show_agenda_mode(Mode::Day, w, cx),
            )
            .into_any_element(),
            view_row(
                "view-todo",
                Icon::TodoList,
                "To-do list",
                current_view == Some(Mode::Todo),
                cx,
                |ws, w, cx| ws.show_agenda_mode(Mode::Todo, w, cx),
            )
            .into_any_element(),
        ];
        let header = self
            .root
            .as_ref()
            .map(|r| self.render_notebook_header(r, cx));
        let colors = cx.theme().colors;
        let current = self.path.clone().filter(|_| self.editor_visible());
        // The inbox (where captures go) is a view of its own rather than one of the notes.
        let inbox = self
            .index
            .read(cx)
            .files()
            .find(|f| is_inbox(&f.rel))
            .map(|f| {
                let open = f
                    .headings
                    .iter()
                    .filter(|h| h.level == 1 && !h.done)
                    .count();
                (f.path.clone(), open)
            });
        let inbox_row = inbox.map(|(path, open)| {
            let selected = current.as_ref() == Some(&path);
            components::list_item("view-inbox", selected, cx)
                .gap_3()
                .child(icon(
                    Icon::Inbox,
                    px(16.),
                    if selected {
                        colors.accent
                    } else {
                        colors.text_secondary
                    },
                ))
                .child(div().flex_1().child("Inbox"))
                .when(open > 0, |d| {
                    d.child(components::caption(open.to_string(), cx))
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, window, cx| {
                        cx.stop_propagation();
                        ws.open_path(path.clone(), window, cx);
                    }),
                )
                .into_any_element()
        });
        let index = self.index.read(cx);
        let files: Vec<(String, PathBuf, bool)> = index
            .files()
            .filter(|f| !is_inbox(&f.rel))
            .map(|f| (f.rel.clone(), f.path.clone(), f.conflict))
            .collect();
        let rows = build_tree(&files, &index.dirs(), &self.expanded);
        let empty = rows.is_empty();
        let tags = index.tags();
        let root = self.root.clone();

        let file_rows: Vec<_> = rows
            .into_iter()
            .enumerate()
            .map(|(i, row)| {
                let (chevron, kind_icon, selected, color) = match &row.kind {
                    TreeKind::Folder { open, .. } => (
                        Some(if *open {
                            Icon::ChevronDown
                        } else {
                            Icon::ChevronRight
                        }),
                        if *open {
                            Icon::FolderOpen
                        } else {
                            Icon::Folder
                        },
                        false,
                        colors.text_primary,
                    ),
                    TreeKind::File { path, conflict } => (
                        None,
                        if *conflict {
                            Icon::Warning
                        } else {
                            Icon::Document
                        },
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
                let target = match (&row.kind, &root) {
                    (TreeKind::Folder { rel, .. }, Some(root)) => {
                        TreeTarget::Folder(root.join(rel))
                    }
                    (TreeKind::File { path, .. }, _) => TreeTarget::File(path.clone()),
                    _ => TreeTarget::Root,
                };
                components::list_item(("tree-row", i), selected, cx)
                    .pl(px(12. + 18. * row.depth as f32))
                    .text_color(color)
                    .gap_1p5()
                    // Only folders have a chevron; files line up with the rows above (and,
                    // inside a folder, with its icon).
                    .children(chevron.map(|c| {
                        div().w(px(12.)).flex_shrink_0().child(icon(
                            c,
                            px(12.),
                            colors.text_secondary,
                        ))
                    }))
                    .child(icon(
                        kind_icon,
                        px(16.),
                        if conflict {
                            colors.caution
                        } else if selected {
                            colors.accent
                        } else {
                            colors.text_secondary
                        },
                    ))
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
                        cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            match &kind {
                                TreeKind::Folder { rel, .. } => {
                                    if !this.expanded.remove(rel) {
                                        this.expanded.insert(rel.clone());
                                    }
                                    cx.notify();
                                }
                                TreeKind::File { path, .. } => {
                                    this.open_path(path.clone(), window, cx);
                                }
                            }
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, e: &gpui::MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            this.open_tree_menu(e.position, target.clone(), cx);
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
                    .gap_3()
                    .child(icon(Icon::Tag, px(16.), colors.text_secondary))
                    .child(div().flex_1().child(tag.clone()))
                    .child(components::caption(count.to_string(), cx))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.show_tag(&t, window, cx);
                        }),
                    )
            })
            .collect();

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
            .children(header)
            .child(
                div()
                    .id("sidebar-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .px_1()
                    .pb_4()
                    .child(components::section_header("Views", cx))
                    .children(inbox_row)
                    .children(views)
                    .child(
                        div()
                            .flex()
                            .items_end()
                            .pr_1()
                            .child(components::section_header("Files", cx).flex_1())
                            .child(self.files_header_buttons(cx)),
                    )
                    .when(empty, |d| {
                        d.child(div().px_3().text_color(colors.text_tertiary).child(
                            if self.scanning {
                                "Loading…"
                            } else {
                                "No notes yet"
                            },
                        ))
                    })
                    .children(file_rows)
                    .when(has_tags, |d| {
                        d.child(components::section_header("Tags", cx))
                    })
                    .children(tag_rows)
                    // Right-click on empty space: new note / folder at the top level.
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, e: &gpui::MouseDownEvent, _, cx| {
                            this.open_tree_menu(e.position, TreeTarget::Root, cx)
                        }),
                    ),
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
            "Projects/org.org",
            "Projects/sub/deep.org",
            "alpha.org",
        ]);
        let rows = build_tree(&f, &[], &BTreeSet::new());
        let names: Vec<_> = rows.iter().map(|r| (r.depth, r.name.as_str())).collect();
        assert_eq!(names, [(0, "Projects"), (0, "alpha.org"), (0, "zeta.org")]);

        let expanded: BTreeSet<String> = ["Projects".into()].into();
        let rows = build_tree(&f, &["Projects/sub".into(), "Empty".into()], &expanded);
        let names: Vec<_> = rows.iter().map(|r| (r.depth, r.name.as_str())).collect();
        assert_eq!(
            names,
            [
                (0, "Empty"),
                (0, "Projects"),
                (1, "sub"),
                (1, "org.org"),
                (0, "alpha.org"),
                (0, "zeta.org")
            ]
        );
    }
}
