//! The window's root view: a notes folder (index, sidebar, watcher), the open file's editor,
//! the find bar, and pickers (command palette, quick switcher, search, tags).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context as _;
use aster_editor::{EditorEvent, FindBar, FindBarEvent, OrgEditor, format_keystrokes};
use aster_index::{Index, fuzzy_score};
use futures::StreamExt;
use gpui::{
    Action, Context, Entity, Focusable, PathPromptOptions, PromptLevel, SharedString, Subscription,
    Task, Window, div, prelude::*, px,
};

use crate::agenda_view::{AgendaEvent, AgendaView, Mode as AgendaMode};
use crate::capture::{CaptureDialog, CaptureEvent, CaptureTime, capture_op};
use crate::file_actions::TreeMenu;
use crate::name_dialog::NameDialog;
use crate::picker::{PickTarget, Picker, PickerEvent, PickerItem};
use crate::session::Session;
use crate::settings::{CaptureTemplate, Settings};
use crate::settings_view::{SettingsEvent, SettingsView};
use crate::watch::{self, FolderWatcher};
use crate::*;
use aster_doc::ops::OpResult;
use aster_doc::{Document, Selection, Transaction, dates, meta};
use aster_ui::components::Tooltip;
use aster_ui::icons::{Icon, icon};
use aster_ui::{ActiveTheme, components, ramp};

pub const SAMPLE: &str = include_str!("../assets/welcome.org");
const AUTOSAVE_DELAY: Duration = Duration::from_millis(1000);
/// Collect file-change events for this long before acting on them.
const WATCH_DEBOUNCE: Duration = Duration::from_millis(150);

/// What fills the content area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MainView {
    Editor,
    Agenda,
    Settings,
}

pub struct Workspace {
    pub(crate) editor: Entity<OrgEditor>,
    pub(crate) path: Option<PathBuf>,
    pub(crate) dirty: bool,
    find_bar: Option<Entity<FindBar>>,
    autosave: Option<Task<()>>,
    _editor_subscription: Subscription,
    _find_subscription: Option<Subscription>,
    picker: Option<(Entity<Picker>, Subscription)>,
    /// Commands offered by the command palette (rebuilt when it opens).
    commands: Vec<(&'static str, Box<dyn Action>)>,

    // ---- notes folder ----
    pub(crate) root: Option<PathBuf>,
    pub(crate) index: Entity<Index>,
    pub(crate) scanning: bool,
    pub(crate) expanded: BTreeSet<String>,
    sidebar_open: bool,
    _watcher: Option<Task<()>>,

    // ---- views and dialogs ----
    agenda: Option<(Entity<AgendaView>, Subscription)>,
    settings: Option<(Entity<SettingsView>, Subscription)>,
    view: MainView,
    capture: Option<(Entity<CaptureDialog>, Subscription)>,
    /// The heading waiting for a refile target: (file, heading start).
    refile_source: Option<(PathBuf, usize)>,
    toast: Option<(SharedString, bool)>,
    toast_task: Option<Task<()>>,
    pub(crate) tree_menu: Option<TreeMenu>,
    pub(crate) name_dialog: Option<(Entity<NameDialog>, Subscription)>,
    /// The welcome sample is showing (unsaved, not a file).
    sample: bool,
    /// Focus for the window itself, so shortcuts work when no note is open.
    focus_handle: gpui::FocusHandle,
}

impl Workspace {
    pub fn new(
        text: String,
        path: Option<PathBuf>,
        root: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (editor, sub) = Self::make_editor(text, window, cx);
        let mut ws = Self {
            editor,
            path,
            dirty: false,
            find_bar: None,
            autosave: None,
            _editor_subscription: sub,
            _find_subscription: None,
            picker: None,
            commands: Vec::new(),
            root: None,
            index: cx.new(|_| Index::default()),
            scanning: false,
            expanded: BTreeSet::new(),
            sidebar_open: true,
            _watcher: None,
            agenda: None,
            settings: None,
            view: MainView::Editor,
            capture: None,
            refile_source: None,
            toast: None,
            toast_task: None,
            tree_menu: None,
            name_dialog: None,
            sample: false,
            focus_handle: cx.focus_handle(),
        };
        window.set_window_title(&ws.title());
        // Follow the system's light/dark setting.
        cx.observe_window_appearance(window, |_, window, cx| {
            let changed = aster_ui::set_appearance(window.appearance(), cx);
            if aster_ui::refresh_accent(cx) | changed {
                window.refresh();
            }
        })
        .detach();
        // Pick up a changed system accent color when the window comes back into focus.
        cx.observe_window_activation(window, |_, window, cx| {
            if window.is_window_active() && aster_ui::refresh_accent(cx) {
                window.refresh();
            }
        })
        .detach();
        ws.sync_base_dir(cx);
        if let Some(root) = root {
            ws.set_root(root, window, cx);
        }
        ws
    }

    /// Give the editor the folder's tags (most used first) for suggestions.
    fn sync_known_tags(&self, cx: &mut Context<Self>) {
        let mut tags: Vec<(String, usize)> = self.index.read(cx).tags().into_iter().collect();
        tags.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let tags = tags.into_iter().map(|(t, _)| t).collect();
        self.editor.update(cx, |e, _| e.set_known_tags(tags));
    }

    /// Tell the editor which folder relative links and images resolve against.
    pub(crate) fn sync_base_dir(&self, cx: &mut Context<Self>) {
        let dir = self
            .path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf);
        self.editor.update(cx, |e, cx| e.set_base_dir(dir, cx));
    }

    fn make_editor(
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<OrgEditor>, Subscription) {
        let editor = cx.new(|cx| OrgEditor::new(text, window, cx));
        let sub = cx.subscribe_in(
            &editor,
            window,
            |this, _, event: &EditorEvent, window, cx| match event {
                EditorEvent::Edited => this.mark_dirty(window, cx),
                EditorEvent::OpenLink(target) => this.open_link(target, window, cx),
                _ => {}
            },
        );
        (editor, sub)
    }

    pub(crate) fn title(&self) -> String {
        let name = match (&self.path, self.sample) {
            (Some(p), _) => p.file_name().map(|n| n.to_string_lossy().into_owned()),
            (None, true) => Some("Welcome (sample)".into()),
            (None, false) => None,
        };
        let folder = self
            .root
            .as_deref()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned());
        let parts: Vec<String> = name.into_iter().chain(folder).collect();
        if parts.is_empty() {
            return "Aster".into();
        }
        format!(
            "{}{} — Aster",
            if self.dirty { "● " } else { "" },
            parts.join(" — ")
        )
    }

    /// Drop a pending autosave (the edits stay in the editor, marked unsaved).
    pub(crate) fn cancel_autosave(&mut self) {
        self.autosave = None;
    }

    /// Whether a document is open in the editor (a file, or the sample).
    pub(crate) fn has_document(&self) -> bool {
        self.path.is_some() || self.sample
    }

    /// Put focus back on whatever fills the content area.
    pub(crate) fn focus_main(&self, window: &mut Window, cx: &mut Context<Self>) {
        match (self.view, &self.agenda, &self.settings) {
            (MainView::Agenda, Some((v, _)), _) => window.focus(&v.focus_handle(cx)),
            (MainView::Settings, _, Some((v, _))) => window.focus(&v.focus_handle(cx)),
            _ if self.has_document() => window.focus(&self.editor.focus_handle(cx)),
            _ => window.focus(&self.focus_handle),
        }
    }

    /// Close the open file (e.g. it was deleted) and show the folder's empty state.
    pub(crate) fn close_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.autosave = None;
        self.dirty = false;
        self.close_find_bar(window, cx);
        let (editor, sub) = Self::make_editor(String::new(), window, cx);
        self.editor = editor;
        self._editor_subscription = sub;
        self.path = None;
        self.sample = false;
        self.sync_base_dir(cx);
        window.set_window_title(&self.title());
        self.save_session();
        self.focus_main(window, cx);
        cx.notify();
    }

    /// Show the welcome sample (unsaved) in the editor.
    pub(crate) fn show_sample(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dirty {
            self.save_now(window, cx);
        }
        let (editor, sub) = Self::make_editor(SAMPLE.to_string(), window, cx);
        self.editor = editor;
        self._editor_subscription = sub;
        self.path = None;
        self.sample = true;
        self.dirty = false;
        self.view = MainView::Editor;
        self.sync_base_dir(cx);
        window.set_window_title(&self.title());
        window.focus(&self.editor.focus_handle(cx));
        cx.notify();
    }

    pub(crate) fn save_session(&self) {
        Session {
            root: self.root.clone(),
            file: self.path.clone(),
        }
        .save();
    }

    // ---- messages -------------------------------------------------------------------------

    /// Show a short message at the bottom of the window (errors stay a little longer).
    pub(crate) fn toast(
        &mut self,
        message: impl Into<SharedString>,
        error: bool,
        cx: &mut Context<Self>,
    ) {
        self.toast = Some((message.into(), error));
        self.toast_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(if error { 6000 } else { 3000 }))
                .await;
            this.update(cx, |ws, cx| {
                ws.toast = None;
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// Apply a document operation to `path`: through the editor when it's the open file (so it
    /// can be undone), otherwise by rewriting the file. Returns whether anything changed.
    fn edit_file(
        &mut self,
        path: &Path,
        op: impl FnOnce(&Document) -> Option<OpResult>,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<bool> {
        if self.path.as_deref() == Some(path) {
            let Some(result) = op(self.editor.read(cx).document()) else {
                return Ok(false);
            };
            self.editor.update(cx, |e, cx| e.apply_result(result, cx));
            let text = self.editor.read(cx).text().to_string();
            self.index.update(cx, |index, cx| {
                if index.covers(path) && index.upsert_text(path, text) {
                    cx.notify();
                }
            });
            return Ok(true);
        }
        let text = if path.exists() {
            read_org(path)?
        } else {
            String::new()
        };
        let doc = Document::new(text);
        let Some(result) = op(&doc) else {
            return Ok(false);
        };
        let mut new_text = doc.text().to_string();
        result.tx.apply(&mut new_text);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        write_atomic(path, &new_text)?;
        self.index.update(cx, |index, cx| {
            if index.covers(path) && index.upsert_text(path, new_text) {
                cx.notify();
            }
        });
        Ok(true)
    }

    // ---- agenda ---------------------------------------------------------------------------

    /// The agenda mode on screen, if the agenda is showing.
    pub(crate) fn current_view(&self, cx: &gpui::App) -> Option<AgendaMode> {
        self.agenda
            .as_ref()
            .filter(|_| self.view == MainView::Agenda)
            .map(|(v, _)| v.read(cx).mode)
    }

    pub(crate) fn show_agenda_mode(
        &mut self,
        mode: AgendaMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.root.is_none() {
            self.toast("Open a notes folder to see its agenda", false, cx);
            return;
        }
        let view = match &self.agenda {
            Some((view, _)) => view.clone(),
            None => {
                let index = self.index.clone();
                let view = cx.new(|cx| AgendaView::new(index, mode, window, cx));
                let sub =
                    cx.subscribe_in(&view, window, |this, _, event: &AgendaEvent, window, cx| {
                        match event {
                            AgendaEvent::Open(path, offset) => {
                                let (path, offset) = (path.clone(), *offset);
                                this.show_editor(window, cx);
                                this.open_at(path, offset..offset, window, cx);
                            }
                            AgendaEvent::ToggleDone(path, offset) => {
                                let (path, offset) = (path.clone(), *offset);
                                this.toggle_done_at(&path, offset, cx);
                            }
                            AgendaEvent::Close => this.show_editor(window, cx),
                        }
                    });
                self.agenda = Some((view.clone(), sub));
                view
            }
        };
        self.view = MainView::Agenda;
        view.update(cx, |v, cx| v.set_mode(mode, window, cx));
        cx.notify();
    }

    /// Whether the open file's editor is on screen (not the agenda or settings).
    pub(crate) fn editor_visible(&self) -> bool {
        self.view == MainView::Editor
    }

    fn show_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.view = MainView::Editor;
        self.focus_main(window, cx);
        cx.notify();
    }

    pub(crate) fn show_agenda(
        &mut self,
        _: &ShowAgenda,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.view == MainView::Agenda {
            self.show_editor(window, cx);
        } else {
            self.show_agenda_mode(AgendaMode::Day, window, cx);
        }
    }

    pub(crate) fn show_settings(
        &mut self,
        _: &ShowSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.view == MainView::Settings {
            self.show_editor(window, cx);
            return;
        }
        let view = match &self.settings {
            Some((view, _)) => view.clone(),
            None => {
                let view = cx.new(|cx| SettingsView::new(window, cx));
                let sub = cx.subscribe_in(
                    &view,
                    window,
                    |this, _, event: &SettingsEvent, window, cx| match event {
                        SettingsEvent::Close => this.show_editor(window, cx),
                        SettingsEvent::Error(msg) => this.toast(msg.clone(), true, cx),
                    },
                );
                self.settings = Some((view.clone(), sub));
                view
            }
        };
        self.close_find_bar(window, cx);
        self.view = MainView::Settings;
        window.focus(&view.focus_handle(cx));
        cx.notify();
    }

    pub(crate) fn show_todo_list(
        &mut self,
        _: &ShowTodoList,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_agenda_mode(AgendaMode::Todo, window, cx);
    }

    /// Mark the heading at `offset` in `path` done (or back to its first open state).
    fn toggle_done_at(&mut self, path: &Path, offset: usize, cx: &mut Context<Self>) {
        let is_task = self
            .index
            .read(cx)
            .file(path)
            .and_then(|f| f.headings.iter().find(|h| h.offset == offset))
            .is_some_and(|h| h.todo.is_some());
        if !is_task {
            self.toast(
                "That's an event, not a task — open it to add a TODO state",
                false,
                cx,
            );
            return;
        }
        let result = self.edit_file(
            path,
            |doc| {
                let h = meta::heading(doc, offset)?;
                let states = meta::todo_states(doc);
                let next = if h.done {
                    states.iter().find(|(_, done)| !done)
                } else {
                    states.iter().find(|(_, done)| *done)
                }?;
                meta::set_todo(doc, offset, Some(&next.0), Selection::default())
            },
            cx,
        );
        if let Err(err) = result {
            self.toast(format!("Couldn't update the task: {err:#}"), true, cx);
        }
    }

    // ---- capture, refile, archive ---------------------------------------------------------

    pub(crate) fn capture(&mut self, _: &Capture, window: &mut Window, cx: &mut Context<Self>) {
        if self.root.is_none() {
            self.toast(
                "Open a notes folder first: captures go to its inbox.org",
                false,
                cx,
            );
            return;
        }
        let dialog = cx.new(|cx| CaptureDialog::new(window, cx));
        let sub = cx.subscribe_in(
            &dialog,
            window,
            |this, _, event: &CaptureEvent, window, cx| match event {
                CaptureEvent::Capture(template, text) => {
                    let (template, text) = (template.clone(), text.clone());
                    this.capture = None;
                    this.focus_main(window, cx);
                    this.save_capture(template, &text, cx);
                }
                CaptureEvent::Dismissed => {
                    this.capture = None;
                    this.focus_main(window, cx);
                    cx.notify();
                }
            },
        );
        self.capture = Some((dialog, sub));
        cx.notify();
    }

    fn save_capture(&mut self, template: CaptureTemplate, text: &str, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let Some(rel) = template.target() else {
            self.toast(
                format!(
                    "The “{}” template has no usable file. Fix it in Settings.",
                    template.name
                ),
                true,
                cx,
            );
            return;
        };
        let at = CaptureTime::now();
        let result = self.edit_file(
            &root.join(&rel),
            |doc| capture_op(doc, &template, text, &at),
            cx,
        );
        match result {
            Ok(_) => self.toast(format!("Captured to {}", rel.display()), false, cx),
            Err(err) => self.toast(format!("Capture failed: {err:#}"), true, cx),
        }
    }

    pub(crate) fn refile(
        &mut self,
        _: &aster_editor::Refile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(src) = self.path.clone() else {
            self.toast("Save this file before refiling from it", false, cx);
            return;
        };
        if self.root.is_none() {
            self.toast("Open a notes folder to refile between files", false, cx);
            return;
        }
        let Some(h) = self.editor.read(cx).heading_start_at_cursor() else {
            self.toast("Put the cursor in a heading to refile it", false, cx);
            return;
        };
        let doc = self.editor.read(cx).document();
        let own = meta::heading(doc, h)
            .map(|x| x.range.clone())
            .unwrap_or(h..h);
        let title = meta::heading(doc, h)
            .map(|x| x.title.clone())
            .unwrap_or_default();
        self.refile_source = Some((src.clone(), h));
        let index = self.index.clone();
        let provider = Box::new(move |q: &str, cx: &gpui::App| {
            let index = index.read(cx);
            let mut out: Vec<(usize, PickerItem, PickTarget)> = Vec::new();
            for f in index.files().filter(|f| !f.conflict) {
                if let Some(score) = fuzzy_score(&format!("{} top level", f.rel), q) {
                    out.push((
                        score,
                        PickerItem {
                            label: format!("{} (top level)", f.display_name()).into(),
                            detail: Some(f.rel.clone().into()),
                            icon: Some(Icon::Document),
                            ..Default::default()
                        },
                        PickTarget::Refile(f.path.clone(), None),
                    ));
                }
                let mut stack: Vec<(usize, String)> = Vec::new();
                for hd in &f.headings {
                    while stack.last().is_some_and(|(l, _)| *l >= hd.level) {
                        stack.pop();
                    }
                    stack.push((hd.level, hd.title.clone()));
                    // Not into itself or its own children.
                    if f.path == src && hd.offset >= own.start && hd.offset < own.end {
                        continue;
                    }
                    let outline = stack
                        .iter()
                        .map(|(_, t)| t.as_str())
                        .collect::<Vec<_>>()
                        .join(" › ");
                    let Some(score) = fuzzy_score(&format!("{outline} {}", f.rel), q) else {
                        continue;
                    };
                    out.push((
                        score,
                        PickerItem {
                            label: outline.into(),
                            detail: Some(f.rel.clone().into()),
                            icon: Some(Icon::ArrowMove),
                            ..Default::default()
                        },
                        PickTarget::Refile(f.path.clone(), Some(hd.offset)),
                    ));
                }
            }
            out.sort_by_key(|(s, ..)| *s);
            out.into_iter().map(|(_, i, t)| (i, t)).collect()
        });
        self.show_picker(&format!("Refile “{title}” to…"), "", provider, window, cx);
    }

    fn do_refile(&mut self, target: PathBuf, parent: Option<usize>, cx: &mut Context<Self>) {
        let Some((src, h)) = self.refile_source.take() else {
            return;
        };
        if self.path.as_ref() != Some(&src) {
            return;
        }
        let target_name = target
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned());
        if target == src {
            let doc = self.editor.read(cx).document();
            let level = parent
                .and_then(|p| meta::heading(doc, p))
                .map_or(1, |p| p.level + 1);
            let before = parent.and_then(|p| {
                let end = meta::heading(doc, p)?.range.end;
                doc.headings()
                    .iter()
                    .find(|x| x.range.start == end)
                    .map(|x| x.range.start)
            });
            let r = meta::move_subtree_to(doc, h, meta::DropTarget { before, level });
            match r {
                Some(r) => {
                    self.editor.update(cx, |e, cx| e.apply_result(r, cx));
                    self.toast(format!("Moved within {target_name}"), false, cx);
                }
                None => self.toast("It's already there", false, cx),
            }
            return;
        }
        // Into another file: add it there first, then remove it here (undoable).
        let level = parent
            .and_then(|p| {
                self.index
                    .read(cx)
                    .file(&target)?
                    .headings
                    .iter()
                    .find(|x| x.offset == p)
                    .map(|x| x.level + 1)
            })
            .unwrap_or(1);
        let Some(chunk) = meta::subtree_text(self.editor.read(cx).document(), h, level) else {
            return;
        };
        match self.edit_file(&target, |doc| meta::insert_subtree(doc, parent, &chunk), cx) {
            Ok(true) => {
                if let Some(r) = meta::remove_subtree(self.editor.read(cx).document(), h) {
                    self.editor.update(cx, |e, cx| e.apply_result(r, cx));
                }
                self.toast(format!("Refiled to {target_name}"), false, cx);
            }
            Ok(false) => self.toast("Couldn't find the target heading", true, cx),
            Err(err) => self.toast(format!("Refile failed: {err:#}"), true, cx),
        }
    }

    pub(crate) fn archive(
        &mut self,
        _: &aster_editor::ArchiveSubtree,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(src) = self.path.clone() else {
            self.toast("Save this file before archiving from it", false, cx);
            return;
        };
        let Some(h) = self.editor.read(cx).heading_start_at_cursor() else {
            self.toast("Put the cursor in a heading to archive it", false, cx);
            return;
        };
        let doc = self.editor.read(cx).document();
        let Some(chunk) = meta::subtree_text(doc, h, 1) else {
            return;
        };
        let mut props: Vec<(&str, String)> = vec![
            ("ARCHIVE_TIME", dates::inactive_now()),
            ("ARCHIVE_FILE", src.display().to_string()),
        ];
        let olpath = meta::outline_path(doc, h).join("/");
        if !olpath.is_empty() {
            props.push(("ARCHIVE_OLPATH", olpath));
        }
        if let Some(todo) = meta::heading(doc, h).and_then(|x| x.todo.clone()) {
            props.push(("ARCHIVE_TODO", todo));
        }
        let chunk = meta::with_properties(&chunk, &props);
        // Org's default archive location: "<file>_archive", top level.
        let archive = PathBuf::from(format!("{}_archive", src.display()));
        let header = format!(
            "#    -*- mode: org -*-\n\n\nArchived entries from file {}\n\n\n",
            src.display()
        );
        let result = self.edit_file(
            &archive,
            |doc| {
                if doc.text().is_empty() {
                    Some(OpResult {
                        tx: Transaction::replace(0..0, format!("{header}{chunk}")),
                        selection: Selection::default(),
                    })
                } else {
                    meta::insert_subtree(doc, None, &chunk)
                }
            },
            cx,
        );
        match result {
            Ok(_) => {
                if let Some(r) = meta::remove_subtree(self.editor.read(cx).document(), h) {
                    self.editor.update(cx, |e, cx| e.apply_result(r, cx));
                }
                let name = archive
                    .file_name()
                    .map_or(String::new(), |n| n.to_string_lossy().into_owned());
                self.toast(format!("Archived to {name}"), false, cx);
            }
            Err(err) => self.toast(format!("Archive failed: {err:#}"), true, cx),
        }
    }

    // ---- saving ---------------------------------------------------------------------------

    fn mark_dirty(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.dirty {
            self.dirty = true;
            window.set_window_title(&self.title());
        }
        // Autosave shortly after typing stops (replacing the task cancels the previous one).
        if self.path.is_some() {
            self.autosave = Some(cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(AUTOSAVE_DELAY).await;
                this.update_in(cx, |ws, window, cx| ws.save_now(window, cx))
                    .ok();
            }));
        }
        cx.notify();
    }

    pub(crate) fn save_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.path.clone() else {
            return;
        };
        self.autosave = None;
        let text = self.editor.read(cx).text().to_owned();
        match write_atomic(&path, &text) {
            Ok(()) => {
                self.dirty = false;
                window.set_window_title(&self.title());
                self.index.update(cx, |index, cx| {
                    if index.covers(&path) && index.upsert_text(&path, text) {
                        cx.notify();
                    }
                });
                cx.notify();
            }
            Err(err) => self.toast(format!("Save failed: {err:#}"), true, cx),
        }
    }

    pub(crate) fn save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        if self.path.is_some() {
            self.save_now(window, cx);
        } else {
            self.save_as(&SaveAs, window, cx);
        }
    }

    pub(crate) fn save_as(&mut self, _: &SaveAs, window: &mut Window, cx: &mut Context<Self>) {
        let dir = self
            .path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .or_else(|| self.root.clone())
            .or_else(std::env::home_dir)
            .unwrap_or_else(|| PathBuf::from("."));
        let name = self
            .path
            .as_deref()
            .and_then(Path::file_name)
            .map_or("notes.org".to_string(), |n| {
                n.to_string_lossy().into_owned()
            });
        let rx = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(path))) = rx.await {
                this.update_in(cx, |ws, window, cx| {
                    ws.path = Some(path);
                    ws.sample = false;
                    ws.sync_base_dir(cx);
                    ws.save_now(window, cx);
                    ws.save_session();
                })
                .ok();
            }
        })
        .detach();
    }

    pub(crate) fn quit(&mut self, _: &Quit, window: &mut Window, cx: &mut Context<Self>) {
        if self.dirty {
            self.save_now(window, cx);
        }
        cx.quit();
    }

    // ---- opening --------------------------------------------------------------------------

    pub(crate) fn open(&mut self, _: &Open, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            this.update_in(cx, |ws, window, cx| ws.open_path(path, window, cx))
                .ok();
        })
        .detach();
    }

    pub(crate) fn open_folder(
        &mut self,
        _: &OpenFolder,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open Folder".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            this.update_in(cx, |ws, window, cx| ws.set_root(path, window, cx))
                .ok();
        })
        .detach();
    }

    pub(crate) fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        // One spelling per file (`a/../b.org` → `b.org`), so it matches the index and tree.
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        if self.path.as_ref() == Some(&path) {
            self.view = MainView::Editor;
            window.focus(&self.editor.focus_handle(cx));
            cx.notify();
            return;
        }
        let text = match read_org(&path) {
            Ok(text) => text,
            Err(err) => {
                self.toast(
                    format!("Couldn't open {}: {err:#}", path.display()),
                    true,
                    cx,
                );
                return;
            }
        };
        // Flush pending edits to the current file before switching.
        if self.dirty {
            self.save_now(window, cx);
        }
        self.close_find_bar(window, cx);
        let (editor, sub) = Self::make_editor(text, window, cx);
        self.editor = editor;
        self._editor_subscription = sub;
        self.path = Some(path.clone());
        self.sample = false;
        self.sync_base_dir(cx);
        self.sync_known_tags(cx);
        self.dirty = false;
        self.reveal_in_tree(&path);
        self.view = MainView::Editor;
        window.set_window_title(&self.title());
        window.focus(&self.editor.focus_handle(cx));
        self.save_session();
        cx.notify();
    }

    /// Open `path` and select `range` in it.
    fn open_at(
        &mut self,
        path: PathBuf,
        range: std::ops::Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_path(path, window, cx);
        self.editor.update(cx, |e, cx| e.select_range(range, cx));
        window.focus(&self.editor.focus_handle(cx));
    }

    /// Expand the folders containing `path` in the file tree.
    pub(crate) fn reveal_in_tree(&mut self, path: &Path) {
        let Some(root) = &self.root else { return };
        let Ok(rel) = path.strip_prefix(root) else {
            return;
        };
        let mut acc = String::new();
        if let Some(parent) = rel.parent() {
            for c in parent.components() {
                if !acc.is_empty() {
                    acc.push('/');
                }
                acc.push_str(&c.as_os_str().to_string_lossy());
                self.expanded.insert(acc.clone());
            }
        }
    }

    /// Resolve a link target to a local path (`file:`, `~/`, relative to the current file).
    fn resolve(&self, target: &str) -> PathBuf {
        let path = target.strip_prefix("file:").unwrap_or(target);
        let path = path.split("::").next().unwrap_or(path);
        let path = match path.strip_prefix("~/") {
            Some(rest) => std::env::home_dir().unwrap_or_default().join(rest),
            None => PathBuf::from(path),
        };
        if path.is_relative() {
            self.path
                .as_deref()
                .and_then(Path::parent)
                .or(self.root.as_deref())
                .map_or(path.clone(), |dir| dir.join(&path))
        } else {
            path
        }
    }

    /// Follow a link the editor couldn't resolve in its own file: web links, files (with an
    /// optional `::*Heading`), and `id:` links found through the workspace index.
    fn open_link(&mut self, target: &str, window: &mut Window, cx: &mut Context<Self>) {
        if target.contains("://") || target.starts_with("mailto:") {
            cx.open_url(target);
            return;
        }
        if let Some(id) = target.strip_prefix("id:") {
            match self.index.read(cx).find_id(id) {
                Some((path, offset)) => self.open_at(path, offset..offset, window, cx),
                None => self.toast(
                    format!("No heading with ID “{id}” in this folder"),
                    true,
                    cx,
                ),
            }
            return;
        }
        let path = self.resolve(target);
        if !path.exists() {
            self.toast(
                format!("Link target not found: {}", path.display()),
                true,
                cx,
            );
            return;
        }
        if !aster_index::is_org_file(&path) {
            cx.open_with_system(&path);
            return;
        }
        self.open_path(path.clone(), window, cx);
        // `file:notes.org::*Heading` jumps to that heading.
        if let Some(search) = target.split_once("::").map(|(_, s)| s.trim())
            && let Some(title) = search.strip_prefix('*')
        {
            let offset = aster_index::index_text(
                &path,
                String::new(),
                self.editor.read(cx).text().to_string(),
            )
            .headings
            .iter()
            .find(|h| h.title == title.trim())
            .map(|h| h.offset);
            if let Some(offset) = offset {
                self.editor.update(cx, |e, cx| e.jump_to(offset, cx));
            }
        }
    }

    // ---- notes folder ---------------------------------------------------------------------

    /// Use `root` as the notes folder: scan it in the background and watch it for changes.
    pub(crate) fn set_root(&mut self, root: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let root = std::fs::canonicalize(&root).unwrap_or(root);
        self.root = Some(root.clone());
        self.scanning = true;
        self.sidebar_open = true;
        if let Some(path) = self.path.clone() {
            self.reveal_in_tree(&path);
        }
        window.set_window_title(&self.title());
        self.save_session();
        cx.notify();

        let scan_root = root.clone();
        let scan = cx.background_spawn(async move { Index::scan(&scan_root) });
        self._watcher = Some(cx.spawn_in(window, async move |this, cx| {
            let index = scan.await;
            let ok = this
                .update_in(cx, |ws, _, cx| {
                    ws.index.update(cx, |i, cx| {
                        *i = index;
                        cx.notify();
                    });
                    ws.scanning = false;
                    ws.sync_known_tags(cx);
                    cx.notify();
                })
                .is_ok();
            if !ok {
                return;
            }
            let Ok(FolderWatcher {
                _watcher: watcher,
                mut changes,
            }) = watch::watch(&root)
            else {
                this.update(cx, |ws, cx| {
                    ws.toast(
                        format!("Can't watch {} for outside changes", root.display()),
                        true,
                        cx,
                    )
                })
                .ok();
                return;
            };
            // Batch events, then update the index and the open file.
            while let Some(first) = changes.next().await {
                cx.background_executor().timer(WATCH_DEBOUNCE).await;
                let mut batch = vec![first];
                while let Ok(p) = changes.try_recv() {
                    batch.push(p);
                }
                let paths = watch::relevant_paths(batch);
                if paths.is_empty() {
                    continue;
                }
                if this
                    .update_in(cx, |ws, window, cx| ws.on_disk_change(paths, window, cx))
                    .is_err()
                {
                    break;
                }
            }
            drop(watcher);
        }));
    }

    /// Files changed on disk (by Emacs, a sync tool, …).
    fn on_disk_change(
        &mut self,
        paths: BTreeSet<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.index.update(cx, |index, cx| {
            let mut changed = false;
            for p in &paths {
                if aster_index::is_org_file(p) {
                    if index.covers(p) {
                        changed |= index.update_file(p);
                    }
                } else {
                    // A folder was created, renamed or deleted.
                    changed |= index.rescan_under(p);
                }
            }
            if changed {
                cx.notify();
            }
        });
        self.sync_known_tags(cx);
        let Some(current) = self.path.clone() else {
            return;
        };
        if !paths.contains(&current) {
            return;
        }
        let Ok(disk) = read_org(&current) else {
            return; // deleted or unreadable: keep what's in the editor
        };
        if disk == self.editor.read(cx).text() {
            return; // our own save, or no real change
        }
        if !self.dirty {
            self.editor.update(cx, |e, cx| e.reload_text(&disk, cx));
            self.dirty = false;
            self.autosave = None;
            window.set_window_title(&self.title());
            return;
        }
        // Unsaved edits here and a different version on disk: ask.
        self.autosave = None;
        let name = current
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned());
        let rx = window.prompt(
            PromptLevel::Warning,
            &format!("“{name}” changed on disk"),
            Some("It was changed outside Aster while you have unsaved edits."),
            &["Keep my version", "Load the version on disk"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let choice = rx.await.unwrap_or(0);
            this.update_in(cx, |ws, window, cx| {
                if choice == 1 {
                    if let Ok(disk) = read_org(&current) {
                        ws.editor.update(cx, |e, cx| e.reload_text(&disk, cx));
                    }
                    ws.dirty = false;
                    ws.autosave = None;
                    window.set_window_title(&ws.title());
                } else {
                    ws.save_now(window, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn toggle_sidebar(
        &mut self,
        _: &ToggleSidebar,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sidebar_open = !self.sidebar_open;
        cx.notify();
    }

    // ---- pickers --------------------------------------------------------------------------

    fn show_picker(
        &mut self,
        placeholder: &str,
        initial: &str,
        provider: crate::picker::Provider,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.picker = None;
        let picker = cx.new(|cx| Picker::new(placeholder, initial, provider, window, cx));
        let sub = cx.subscribe_in(
            &picker,
            window,
            |this, _, event: &PickerEvent, window, cx| match event {
                PickerEvent::Confirm(target) => {
                    let target = target.clone();
                    let refile = this.refile_source.take();
                    this.close_picker(window, cx);
                    this.refile_source = refile;
                    this.run_pick(target, window, cx);
                }
                PickerEvent::Dismissed => this.close_picker(window, cx),
            },
        );
        self.picker = Some((picker, sub));
        cx.notify();
    }

    fn close_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.refile_source = None;
        if self.picker.take().is_some() {
            self.focus_main(window, cx);
            cx.notify();
        }
    }

    fn run_pick(&mut self, target: PickTarget, window: &mut Window, cx: &mut Context<Self>) {
        match target {
            PickTarget::Command(i) => {
                if let Some((_, action)) = self.commands.get(i) {
                    let action = action.boxed_clone();
                    self.focus_main(window, cx);
                    window.dispatch_action(action, cx);
                }
            }
            PickTarget::File(path) => self.open_path(path, window, cx),
            PickTarget::Location(path, range) => self.open_at(path, range, window, cx),
            PickTarget::Refile(path, parent) => self.do_refile(path, parent, cx),
        }
    }

    fn commands() -> Vec<(&'static str, Box<dyn Action>)> {
        let mut commands: Vec<(&'static str, Box<dyn Action>)> = vec![
            ("Open file…", Box::new(Open)),
            ("Open folder…", Box::new(OpenFolder)),
            ("Go to file…", Box::new(QuickOpen)),
            ("Search in all files…", Box::new(SearchAll)),
            ("Agenda", Box::new(ShowAgenda)),
            ("To-do list", Box::new(ShowTodoList)),
            ("Capture…", Box::new(Capture)),
            ("Settings…", Box::new(ShowSettings)),
            ("Toggle sidebar", Box::new(ToggleSidebar)),
            ("Save", Box::new(Save)),
            ("Save as…", Box::new(SaveAs)),
            ("Find…", Box::new(Find)),
            ("Find and replace…", Box::new(FindReplace)),
            ("Find next", Box::new(FindNext)),
            ("Find previous", Box::new(FindPrevious)),
        ];
        commands.extend(aster_editor::commands());
        commands.push(("Quit Aster", Box::new(Quit)));
        commands
    }

    pub(crate) fn show_command_palette(
        &mut self,
        _: &ShowCommandPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commands = Self::commands();
        // Shortcuts as the document editor would resolve them.
        let target = self.editor.focus_handle(cx);
        let entries: Vec<(&'static str, Option<SharedString>)> = self
            .commands
            .iter()
            .map(|(name, action)| {
                // As the editor would resolve it; if it isn't on screen (agenda showing), look
                // in its key context, then globally.
                let hint = window
                    .highest_precedence_binding_for_action_in(action.as_ref(), &target)
                    .or_else(|| {
                        window.highest_precedence_binding_for_action_in_context(
                            action.as_ref(),
                            gpui::KeyContext::parse("OrgEditor").unwrap_or_default(),
                        )
                    })
                    .or_else(|| window.highest_precedence_binding_for_action(action.as_ref()))
                    .map(|b| format_keystrokes(&b).into());
                (*name, hint)
            })
            .collect();
        let provider = Box::new(move |q: &str, _: &gpui::App| {
            let mut scored: Vec<(usize, usize)> = entries
                .iter()
                .enumerate()
                .filter_map(|(i, (name, _))| fuzzy_score(name, q).map(|s| (s, i)))
                .collect();
            scored.sort();
            scored
                .into_iter()
                .map(|(_, i)| {
                    let (name, hint) = &entries[i];
                    (
                        PickerItem {
                            label: (*name).into(),
                            hint: hint.clone(),
                            ..Default::default()
                        },
                        PickTarget::Command(i),
                    )
                })
                .collect()
        });
        self.show_picker("Type a command…", "", provider, window, cx);
    }

    pub(crate) fn quick_open(
        &mut self,
        _: &QuickOpen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.root.is_none() {
            self.open_folder(&OpenFolder, window, cx);
            return;
        }
        let index = self.index.clone();
        let provider = Box::new(move |q: &str, cx: &gpui::App| {
            index
                .read(cx)
                .match_files(q)
                .into_iter()
                .take(200)
                .map(|f| {
                    (
                        PickerItem {
                            label: f.display_name().into(),
                            detail: Some(f.rel.clone().into()),
                            hint: f.conflict.then(|| "sync conflict".into()),
                            icon: Some(if f.conflict {
                                Icon::Warning
                            } else {
                                Icon::Document
                            }),
                            ..Default::default()
                        },
                        PickTarget::File(f.path.clone()),
                    )
                })
                .collect()
        });
        self.show_picker("Go to file…", "", provider, window, cx);
    }

    pub(crate) fn search_all(
        &mut self,
        _: &SearchAll,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.root.is_none() {
            self.open_folder(&OpenFolder, window, cx);
            return;
        }
        let seed = self.editor.read(cx).selected_text().to_string();
        let seed = if seed.contains('\n') {
            String::new()
        } else {
            seed
        };
        let index = self.index.clone();
        let provider = Box::new(move |q: &str, cx: &gpui::App| {
            if q.trim().chars().count() < 2 {
                return Vec::new();
            }
            index
                .read(cx)
                .search(q, 500)
                .into_iter()
                .map(|hit| {
                    let len = hit.snippet_match.len();
                    (
                        PickerItem {
                            label: hit.snippet.clone().into(),
                            highlight: Some(hit.snippet_match.clone()),
                            detail: Some(format!("{}:{}", hit.rel, hit.line).into()),
                            hint: None,
                            icon: Some(Icon::Document),
                        },
                        PickTarget::Location(hit.path.clone(), hit.offset..hit.offset + len),
                    )
                })
                .collect()
        });
        self.show_picker("Search all files…", &seed, provider, window, cx);
    }

    /// List headings tagged `tag` across the folder.
    pub(crate) fn show_tag(&mut self, tag: &str, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.index.clone();
        let tag = tag.to_string();
        let placeholder = format!("Headings tagged #{tag}…");
        let provider = Box::new(move |q: &str, cx: &gpui::App| {
            let index = index.read(cx);
            let mut scored: Vec<(usize, PickerItem, PickTarget)> = index
                .headings_with_tag(&tag)
                .into_iter()
                .filter_map(|(f, h)| {
                    let score = fuzzy_score(&h.title, q)?;
                    let label = match &h.todo {
                        Some(t) => format!("{t} {}", h.title),
                        None => h.title.clone(),
                    };
                    Some((
                        score,
                        PickerItem {
                            label: label.into(),
                            detail: Some(f.rel.clone().into()),
                            icon: Some(Icon::Tag),
                            ..Default::default()
                        },
                        PickTarget::Location(f.path.clone(), h.offset..h.offset),
                    ))
                })
                .collect();
            scored.sort_by_key(|(s, ..)| *s);
            scored.into_iter().map(|(_, i, t)| (i, t)).collect()
        });
        self.show_picker(&placeholder, "", provider, window, cx);
    }

    // ---- find -----------------------------------------------------------------------------

    fn show_find(&mut self, with_replace: bool, window: &mut Window, cx: &mut Context<Self>) {
        let seed = Some(self.editor.read(cx).selected_text().to_string());
        let bar = match &self.find_bar {
            Some(bar) => bar.clone(),
            None => {
                let editor = self.editor.clone();
                let bar = cx.new(|cx| FindBar::new(editor, window, cx));
                self._find_subscription = Some(cx.subscribe_in(
                    &bar,
                    window,
                    |this, _, event: &FindBarEvent, window, cx| match event {
                        FindBarEvent::Dismissed => this.close_find_bar(window, cx),
                    },
                ));
                self.find_bar = Some(bar.clone());
                bar
            }
        };
        bar.update(cx, |b, cx| b.open(with_replace, seed, window, cx));
        cx.notify();
    }

    fn close_find_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.find_bar.take().is_some() {
            self._find_subscription = None;
            self.editor.update(cx, |e, cx| e.clear_search(cx));
            window.focus(&self.editor.focus_handle(cx));
            cx.notify();
        }
    }

    pub(crate) fn find(&mut self, _: &Find, window: &mut Window, cx: &mut Context<Self>) {
        self.show_find(false, window, cx);
    }

    pub(crate) fn find_replace(
        &mut self,
        _: &FindReplace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_find(true, window, cx);
    }

    pub(crate) fn find_next(&mut self, _: &FindNext, window: &mut Window, cx: &mut Context<Self>) {
        match &self.find_bar {
            Some(bar) => bar.update(cx, |b, cx| b.next(false, cx)),
            None => self.show_find(false, window, cx),
        }
    }

    pub(crate) fn find_previous(
        &mut self,
        _: &FindPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match &self.find_bar {
            Some(bar) => bar.update(cx, |b, cx| b.next(true, cx)),
            None => self.show_find(false, window, cx),
        }
    }
}

pub fn read_org(path: &Path) -> anyhow::Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    String::from_utf8(bytes).context("Aster currently only opens UTF-8 files")
}

/// Write via a temp file + rename so sync tools never observe a half-written file.
pub(crate) fn write_atomic(path: &Path, text: &str) -> anyhow::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let tmp = dir.join(format!(
        ".{}.aster-tmp",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));
    std::fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

/// Height of the custom title area on macOS (the traffic lights sit in it).
const TITLE_BAR_HEIGHT: f32 = 38.;

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors;
        let sidebar = if self.sidebar_open && self.root.is_some() {
            Some(self.render_sidebar(cx).into_any_element())
        } else {
            None
        };
        let has_sidebar = sidebar.is_some();
        // The command bar (docs/DESIGN.md §11). On macOS it's the transparent title bar (traffic
        // lights on the left, drag to move); elsewhere it sits under the native title bar and
        // gives access to commands that live in the menu bar on macOS.
        let mac = cfg!(target_os = "macos");
        let bar_button = |id: &'static str,
                          glyph: Icon,
                          tip: &'static str,
                          action: fn() -> Box<dyn Action>,
                          cx: &mut Context<Self>| {
            components::icon_button(id, glyph, cx)
                .tooltip(move |window, cx| Tooltip::text(tip, window, cx))
                .on_click(cx.listener(move |_, _, window, cx| {
                    window.dispatch_action(action(), cx);
                }))
        };
        let title_bar = div()
            .id("title-bar")
            .h(px(TITLE_BAR_HEIGHT))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .when(mac, |d| d.pl(px(78.)))
            .child(bar_button(
                "bar-sidebar",
                Icon::PanelLeft,
                "Toggle sidebar",
                || Box::new(ToggleSidebar),
                cx,
            ))
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(ramp::BODY.size)
                    .text_color(colors.text_secondary)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(window.window_title())
                    .when(mac, |d| {
                        d.on_mouse_down(gpui::MouseButton::Left, |e, window, _| {
                            if e.click_count == 2 {
                                window.zoom_window();
                            } else {
                                window.start_window_move();
                            }
                        })
                    }),
            )
            .child(bar_button(
                "bar-search",
                Icon::Search,
                "Search all files",
                || Box::new(SearchAll),
                cx,
            ))
            .child(bar_button(
                "bar-capture",
                Icon::NoteAdd,
                "Capture",
                || Box::new(Capture),
                cx,
            ))
            .child(bar_button(
                "bar-agenda",
                Icon::Agenda,
                "Agenda",
                || Box::new(ShowAgenda),
                cx,
            ))
            .child(bar_button(
                "bar-settings",
                Icon::Settings,
                "Settings",
                || Box::new(ShowSettings),
                cx,
            ))
            .child(bar_button(
                "bar-commands",
                Icon::More,
                "All commands",
                || Box::new(ShowCommandPalette),
                cx,
            ));
        let main = match (self.view, &self.agenda, &self.settings) {
            (MainView::Agenda, Some((agenda, _)), _) => agenda.clone().into_any_element(),
            (MainView::Settings, _, Some((settings, _))) => settings.clone().into_any_element(),
            _ if self.has_document() => self.editor.clone().into_any_element(),
            _ if self.root.is_some() => self.render_empty_state(cx),
            _ => self.render_first_run(cx),
        };
        let vim_mode = (self.view == MainView::Editor && self.has_document())
            .then(|| self.editor.read(cx).vim_mode(cx))
            .flatten();
        let text_size = cx.global::<Settings>().font_size;
        let line_height = cx.global::<Settings>().line_height();

        div()
            .size_full()
            .flex()
            .flex_col()
            .track_focus(&self.focus_handle)
            .bg(colors.base)
            .text_color(colors.text_primary)
            .text_size(px(text_size))
            .line_height(px(line_height))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::save_as))
            .on_action(cx.listener(Self::open))
            .on_action(cx.listener(Self::open_folder))
            .on_action(cx.listener(Self::quit))
            .on_action(cx.listener(Self::find))
            .on_action(cx.listener(Self::find_replace))
            .on_action(cx.listener(Self::find_next))
            .on_action(cx.listener(Self::find_previous))
            .on_action(cx.listener(Self::show_command_palette))
            .on_action(cx.listener(Self::quick_open))
            .on_action(cx.listener(Self::search_all))
            .on_action(cx.listener(Self::toggle_sidebar))
            .on_action(cx.listener(Self::show_agenda))
            .on_action(cx.listener(Self::show_todo_list))
            .on_action(cx.listener(Self::capture))
            .on_action(cx.listener(Self::show_settings))
            .on_action(cx.listener(Self::refile))
            .on_action(cx.listener(Self::archive))
            .child(title_bar)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .children(sidebar)
                    // The content layer: a card inset into the base, rounded where it meets
                    // the sidebar and title area.
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .flex()
                            .flex_col()
                            .relative()
                            .overflow_hidden()
                            .bg(colors.layer)
                            .border_t_1()
                            .border_color(colors.stroke_surface)
                            .when(has_sidebar, |d| {
                                d.border_l_1().rounded_tl(aster_ui::RADIUS_OVERLAY)
                            })
                            .children(
                                self.find_bar
                                    .clone()
                                    .filter(|_| self.view == MainView::Editor),
                            )
                            .child(div().flex_1().min_h_0().child(main))
                            .children(vim_mode.map(|mode| {
                                // Vim's mode, as a status pill in the corner.
                                div().absolute().bottom(px(12.)).right(px(16.)).child(
                                    components::pill(
                                        if mode == aster_editor::vim::VimMode::Normal {
                                            colors.text_secondary
                                        } else {
                                            colors.text_on_accent
                                        },
                                        if mode == aster_editor::vim::VimMode::Normal {
                                            colors.subtle_secondary
                                        } else {
                                            colors.accent
                                        },
                                    )
                                    .px_2()
                                    .text_size(ramp::CAPTION.size)
                                    .line_height(ramp::CAPTION.line_height)
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(mode.label()),
                                )
                            })),
                    ),
            )
            .children(self.capture.as_ref().map(|(c, _)| {
                gpui::deferred(
                    div()
                        .absolute()
                        .top(px(80.))
                        .left_0()
                        .right_0()
                        .flex()
                        .justify_center()
                        .child(c.clone()),
                )
                .with_priority(2)
            }))
            .children(self.name_dialog.as_ref().map(|(d, _)| {
                gpui::deferred(
                    div()
                        .absolute()
                        .top(px(120.))
                        .left_0()
                        .right_0()
                        .flex()
                        .justify_center()
                        .child(d.clone()),
                )
                .with_priority(2)
            }))
            .children(self.render_tree_menu(cx))
            .children(self.toast.clone().map(|(msg, error)| {
                // InfoBar-style toast (docs/DESIGN.md §7).
                let theme = cx.theme();
                let surface = div()
                    .absolute()
                    .bottom(px(16.))
                    .right(px(16.))
                    .max_w(px(420.))
                    .px_3()
                    .py_2()
                    .flex()
                    .gap_2()
                    .rounded(aster_ui::RADIUS_OVERLAY)
                    .shadow(theme.shadow(aster_ui::Elevation::Tooltip))
                    .border_1()
                    .border_color(colors.stroke_surface)
                    .text_size(ramp::BODY.size)
                    .line_height(ramp::BODY.line_height);
                let surface = if error {
                    surface
                        .bg(colors.critical_bg)
                        .child(icon(Icon::ErrorCircle, px(16.), colors.critical).mt(px(2.)))
                        .child(div().text_color(colors.text_primary).child(msg))
                } else {
                    surface
                        .bg(colors.flyout)
                        .child(icon(Icon::CheckmarkCircle, px(16.), colors.success).mt(px(2.)))
                        .child(div().text_color(colors.text_primary).child(msg))
                };
                gpui::deferred(surface).with_priority(3)
            }))
            .children(self.picker.as_ref().map(|(p, _)| {
                gpui::deferred(
                    div()
                        .absolute()
                        .top(px(60.))
                        .left_0()
                        .right_0()
                        .flex()
                        .justify_center()
                        .child(p.clone()),
                )
                .with_priority(2)
            }))
    }
}
