//! Sidebar file management: the right-click menu, and new note / new folder / rename / delete.

use std::path::{Path, PathBuf};

use aster_ui::icons::{Icon, icon};
use aster_ui::{ActiveTheme, components};
use gpui::{
    AnyElement, Context, MouseButton, Pixels, Point, PromptLevel, Window, anchored, deferred, div,
    prelude::*, px,
};

use crate::files;
use crate::name_dialog::{NameDialog, NameEvent};
use crate::workspace::Workspace;

/// What a file action applies to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeTarget {
    /// The notes folder itself (the "Files" header, empty space).
    Root,
    Folder(PathBuf),
    File(PathBuf),
}

pub struct TreeMenu {
    pub position: Point<Pixels>,
    pub target: TreeTarget,
}

#[derive(Debug, Clone)]
pub enum FileAction {
    NewNote(PathBuf),
    NewFolder(PathBuf),
    Rename(PathBuf),
    Delete(PathBuf),
    Reveal(PathBuf),
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map_or(String::new(), |n| n.to_string_lossy().into_owned())
}

impl Workspace {
    /// The folder new notes go in for a target.
    fn target_dir(&self, target: &TreeTarget) -> Option<PathBuf> {
        match target {
            TreeTarget::Root => self.root.clone(),
            TreeTarget::Folder(dir) => Some(dir.clone()),
            TreeTarget::File(path) => path.parent().map(Path::to_path_buf),
        }
    }

    pub(crate) fn open_tree_menu(
        &mut self,
        position: Point<Pixels>,
        target: TreeTarget,
        cx: &mut Context<Self>,
    ) {
        self.tree_menu = Some(TreeMenu { position, target });
        cx.notify();
    }

    pub(crate) fn run_file_action(
        &mut self,
        action: FileAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tree_menu = None;
        cx.notify();
        match action {
            FileAction::NewNote(dir) => self.ask_name(
                "New note",
                "Create",
                "",
                window,
                cx,
                move |ws, name, w, cx| {
                    let path = files::new_note(&dir, name)?;
                    ws.rescan(&path, cx);
                    ws.open_path(path, w, cx);
                    // Start typing below the title.
                    ws.editor
                        .update(cx, |e, cx| e.select_range(usize::MAX..usize::MAX, cx));
                    Ok(())
                },
            ),
            FileAction::NewFolder(dir) => self.ask_name(
                "New folder",
                "Create",
                "",
                window,
                cx,
                move |ws, name, w, cx| {
                    let path = files::new_folder(&dir, name)?;
                    ws.rescan(&path, cx);
                    ws.reveal_in_tree(&path.join("_"));
                    ws.focus_main(w, cx);
                    Ok(())
                },
            ),
            FileAction::Rename(path) => {
                let name = display_name(&path);
                // Don't lose unsaved edits to the file being moved.
                if self.path.as_ref().is_some_and(|p| p.starts_with(&path)) && self.dirty {
                    self.save_now(window, cx);
                }
                self.ask_name(
                    format!("Rename “{}”", name.trim_end_matches(".org")),
                    "Rename",
                    &name,
                    window,
                    cx,
                    move |ws, name, w, cx| {
                        let to = files::rename(&path, name)?;
                        ws.after_move(&path, &to, w, cx);
                        ws.focus_main(w, cx);
                        Ok(())
                    },
                );
            }
            FileAction::Delete(path) => {
                let name = display_name(&path);
                let what = if path.is_dir() {
                    format!("the folder “{name}” and everything in it")
                } else {
                    format!("“{}”", name.trim_end_matches(".org"))
                };
                let rx = window.prompt(
                    PromptLevel::Warning,
                    &format!("Move {what} to the trash?"),
                    Some("You can restore it from the trash."),
                    &["Move to Trash", "Cancel"],
                    cx,
                );
                cx.spawn_in(window, async move |this, cx| {
                    if rx.await.ok() != Some(0) {
                        return;
                    }
                    // A pending autosave mustn't recreate the file after it's gone.
                    this.update(cx, |ws, _| {
                        if ws.path.as_ref().is_some_and(|p| p.starts_with(&path)) {
                            ws.cancel_autosave();
                        }
                    })
                    .ok();
                    // Trashing can take a moment (Finder on macOS); keep the UI responsive.
                    let result = {
                        let path = path.clone();
                        cx.background_spawn(async move { files::delete(&path) })
                            .await
                    };
                    this.update_in(cx, |ws, window, cx| match result {
                        Ok(()) => ws.after_delete(&path, window, cx),
                        Err(err) => ws.toast(format!("Couldn't delete {name}: {err:#}"), true, cx),
                    })
                    .ok();
                })
                .detach();
            }
            FileAction::Reveal(path) => cx.reveal_path(&path),
        }
    }

    fn ask_name(
        &mut self,
        title: impl Into<gpui::SharedString>,
        action: &'static str,
        initial: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
        on_submit: impl Fn(
            &mut Workspace,
            &str,
            &mut Window,
            &mut Context<Workspace>,
        ) -> anyhow::Result<()>
        + 'static,
    ) {
        let title = title.into();
        let dialog = cx.new(|cx| NameDialog::new(title, action, initial, window, cx));
        let sub = cx.subscribe_in(
            &dialog,
            window,
            move |this, dialog, event: &NameEvent, window, cx| match event {
                NameEvent::Submit(name) => match on_submit(this, name, window, cx) {
                    Ok(()) => {
                        this.name_dialog = None;
                        cx.notify();
                    }
                    Err(err) => dialog.update(cx, |d, cx| d.set_error(format!("{err:#}"), cx)),
                },
                NameEvent::Dismissed => {
                    this.name_dialog = None;
                    this.focus_main(window, cx);
                    cx.notify();
                }
            },
        );
        self.name_dialog = Some((dialog, sub));
        cx.notify();
    }

    /// Update the index for a created, renamed or deleted path right away (the watcher would
    /// catch up too, a moment later).
    fn rescan(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.index.update(cx, |index, cx| {
            if index.rescan_under(path) {
                cx.notify();
            }
        });
    }

    fn after_move(&mut self, from: &Path, to: &Path, window: &mut Window, cx: &mut Context<Self>) {
        self.rescan(from, cx);
        self.rescan(to, cx);
        if let (Some(root), true) = (self.root.clone(), to.is_dir()) {
            let rel = |p: &Path| {
                p.strip_prefix(&root)
                    .map(|r| r.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default()
            };
            let (from_rel, to_rel) = (rel(from), rel(to));
            self.expanded = std::mem::take(&mut self.expanded)
                .into_iter()
                .map(|e| match e.strip_prefix(&from_rel) {
                    Some(rest) if rest.is_empty() || rest.starts_with('/') => {
                        format!("{to_rel}{rest}")
                    }
                    _ => e,
                })
                .collect();
        }
        if let Some(moved) = self.path.as_deref().and_then(|p| files::remap(p, from, to)) {
            self.path = Some(moved);
            self.sync_base_dir(cx);
            window.set_window_title(&self.title());
            self.save_session();
        }
        cx.notify();
    }

    fn after_delete(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        self.rescan(path, cx);
        if self.path.as_ref().is_some_and(|p| p.starts_with(path)) {
            self.close_file(window, cx);
        }
        self.toast(
            format!("Moved {} to the trash", display_name(path)),
            false,
            cx,
        );
    }

    /// The right-click menu for the file tree.
    pub(crate) fn render_tree_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.tree_menu.as_ref()?;
        let dir = self.target_dir(&menu.target)?;
        let c = cx.theme().colors;
        let mut items: Vec<(Icon, &'static str, FileAction)> = vec![
            (
                Icon::DocumentAdd,
                "New note",
                FileAction::NewNote(dir.clone()),
            ),
            (
                Icon::FolderAdd,
                "New folder",
                FileAction::NewFolder(dir.clone()),
            ),
        ];
        let item_path = match &menu.target {
            TreeTarget::Root => None,
            TreeTarget::Folder(p) | TreeTarget::File(p) => Some(p.clone()),
        };
        if let Some(p) = &item_path {
            items.push((Icon::Rename, "Rename…", FileAction::Rename(p.clone())));
            items.push((Icon::Delete, "Move to Trash", FileAction::Delete(p.clone())));
        }
        let reveal = if cfg!(target_os = "macos") {
            "Show in Finder"
        } else {
            "Show in file manager"
        };
        items.push((
            Icon::Open,
            reveal,
            FileAction::Reveal(item_path.unwrap_or(dir)),
        ));
        let rows = items
            .into_iter()
            .enumerate()
            .map(|(i, (glyph, label, action))| {
                let danger = matches!(action, FileAction::Delete(_));
                components::list_item(("tree-menu", i), false, cx)
                    .min_h(px(28.))
                    .gap_3()
                    .child(icon(
                        glyph,
                        px(16.),
                        if danger { c.critical } else { c.text_secondary },
                    ))
                    .child(label)
                    .on_click(cx.listener(move |ws, _, window, cx| {
                        ws.run_file_action(action.clone(), window, cx)
                    }))
            });
        Some(
            deferred(
                anchored().position(menu.position).child(
                    components::flyout(cx)
                        .min_w(px(200.))
                        .on_mouse_down_out(cx.listener(|ws, _, _, cx| {
                            ws.tree_menu = None;
                            cx.notify();
                        }))
                        .children(rows),
                ),
            )
            .with_priority(3)
            .into_any_element(),
        )
    }

    /// Small "new note / new folder" buttons for the Files header.
    pub(crate) fn files_header_buttons(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let button = |id: &'static str,
                      glyph: Icon,
                      tip: &'static str,
                      folder: bool,
                      cx: &mut Context<Self>| {
            components::icon_button(id, glyph, cx)
                .size(px(24.))
                .tooltip(move |window, cx| components::Tooltip::text(tip, window, cx))
                .on_click(cx.listener(move |ws, _, window, cx| {
                    if let Some(root) = ws.root.clone() {
                        let action = if folder {
                            FileAction::NewFolder(root)
                        } else {
                            FileAction::NewNote(root)
                        };
                        ws.run_file_action(action, window, cx);
                    }
                }))
        };
        div()
            .flex()
            .gap_0p5()
            .child(button(
                "files-new-note",
                Icon::DocumentAdd,
                "New note",
                false,
                cx,
            ))
            .child(button(
                "files-new-folder",
                Icon::FolderAdd,
                "New folder",
                true,
                cx,
            ))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    }
}
