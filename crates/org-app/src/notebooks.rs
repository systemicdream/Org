//! The notebook switcher: the notes folder's name at the top of the sidebar opens a menu of
//! recently used notebooks, plus opening or creating another.

use std::path::{Path, PathBuf};

use gpui::{
    AnyElement, Context, MouseButton, SharedString, Window, anchored, deferred, div, point,
    prelude::*, px,
};
use org_ui::icons::{Icon, icon};
use org_ui::{ActiveTheme, components};

use crate::session::{RecentNotebook, Session};
use crate::workspace::Workspace;

/// The folder's name as the notebook's name.
pub fn notebook_name(root: &Path) -> String {
    root.file_name().map_or_else(
        || root.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// Where a notebook lives, shortened with `~` for the home folder.
fn location(root: &Path) -> String {
    let parent = root.parent().unwrap_or(root);
    match std::env::home_dir().and_then(|h| parent.strip_prefix(&h).ok().map(Path::to_path_buf)) {
        Some(rel) if rel.as_os_str().is_empty() => "~".into(),
        Some(rel) => format!("~/{}", rel.display()),
        None => parent.display().to_string(),
    }
}

impl Workspace {
    pub(crate) fn toggle_notebook_menu(&mut self, cx: &mut Context<Self>) {
        self.notebook_menu = match self.notebook_menu {
            Some(_) => None,
            None => Some(
                Session::load()
                    .recent
                    .into_iter()
                    .filter(|n| n.root.is_dir())
                    .collect(),
            ),
        };
        cx.notify();
    }

    /// Make `root` the notes folder, reopening the note last open in it.
    pub(crate) fn switch_notebook(
        &mut self,
        root: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.notebook_menu = None;
        let root = std::fs::canonicalize(&root).unwrap_or(root);
        if self.root.as_ref() == Some(&root) {
            cx.notify();
            return;
        }
        let last = Session::load()
            .recent
            .into_iter()
            .find(|n| n.root == root)
            .and_then(|n| n.file)
            .filter(|f| f.is_file());
        if self.dirty {
            self.save_now(window, cx);
        }
        self.set_root(root.clone(), window, cx);
        match last {
            Some(file) => self.open_path(file, window, cx),
            None if !self.path.as_ref().is_some_and(|p| p.starts_with(&root)) => {
                self.close_file(window, cx)
            }
            None => {}
        }
    }

    /// The notes folder's name, which opens the notebook switcher.
    pub(crate) fn render_notebook_header(&self, root: &Path, cx: &mut Context<Self>) -> AnyElement {
        let c = cx.theme().colors;
        let open = self.notebook_menu.is_some();
        div()
            .id("notebook-switcher")
            .mx_2()
            .mb_1()
            .px_2()
            .py_1()
            .flex()
            .items_center()
            .gap_1p5()
            .rounded(org_ui::RADIUS_CONTROL)
            .cursor_pointer()
            .when(open, |d| d.bg(c.subtle_secondary))
            .hover(|d| d.bg(c.subtle_secondary))
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(SharedString::from(notebook_name(root))),
            )
            .child(icon(Icon::ChevronDown, px(12.), c.text_secondary).flex_shrink_0())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|ws, _, _, cx| ws.toggle_notebook_menu(cx)))
            .into_any_element()
    }

    /// The switcher's menu, under the sidebar header.
    pub(crate) fn render_notebook_menu(
        &self,
        top: f32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let recent = self.notebook_menu.as_ref()?;
        let c = cx.theme().colors;
        let notebook_rows = recent.iter().enumerate().map(|(i, n)| {
            let RecentNotebook { root, .. } = n.clone();
            let current = self.root.as_ref() == Some(&root);
            components::list_item(("notebook", i), false, cx)
                .min_h(px(40.))
                .gap_3()
                .child(icon(
                    Icon::Notebook,
                    px(16.),
                    if current { c.accent } else { c.text_secondary },
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(notebook_name(&root)),
                        )
                        .child(
                            components::caption(location(&root), cx)
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis(),
                        ),
                )
                .when(current, |d| {
                    d.child(icon(Icon::Checkmark, px(14.), c.accent))
                })
                .on_click(cx.listener(move |ws, _, window, cx| {
                    ws.switch_notebook(root.clone(), window, cx)
                }))
        });
        let action_row =
            |id: &'static str, glyph: Icon, label: &'static str, cx: &Context<Self>| {
                components::list_item(id, false, cx)
                    .min_h(px(28.))
                    .gap_3()
                    .child(icon(glyph, px(16.), c.text_secondary))
                    .child(label)
            };
        let has_recent = !recent.is_empty();
        Some(
            deferred(
                anchored().position(point(px(8.), px(top))).child(
                    components::flyout(cx)
                        .w(px(260.))
                        .on_mouse_down_out(cx.listener(|ws, _, _, cx| {
                            ws.notebook_menu = None;
                            cx.notify();
                        }))
                        .children(notebook_rows)
                        .when(has_recent, |d| {
                            d.child(div().my_1().h(px(1.)).bg(c.stroke_divider))
                        })
                        .child(
                            action_row("notebook-open", Icon::FolderOpen, "Open folder…", cx)
                                .on_click(cx.listener(|ws, _, window, cx| {
                                    ws.notebook_menu = None;
                                    window.dispatch_action(Box::new(crate::OpenFolder), cx);
                                })),
                        )
                        .child(
                            action_row("notebook-new", Icon::FolderAdd, "New notes folder…", cx)
                                .on_click(cx.listener(|ws, _, window, cx| {
                                    ws.notebook_menu = None;
                                    ws.create_notes_folder(window, cx);
                                })),
                        ),
                ),
            )
            .with_priority(3)
            .into_any_element(),
        )
    }
}
