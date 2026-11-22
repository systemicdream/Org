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
    Task, Window, div, prelude::*, px, rgb,
};

use crate::picker::{PickTarget, Picker, PickerEvent, PickerItem};
use crate::session::Session;
use crate::watch::{self, FolderWatcher};
use crate::*;

pub const SAMPLE: &str = include_str!("../assets/welcome.org");
const AUTOSAVE_DELAY: Duration = Duration::from_millis(1000);
/// Collect file-change events for this long before acting on them.
const WATCH_DEBOUNCE: Duration = Duration::from_millis(150);

pub struct Workspace {
    pub(crate) editor: Entity<OrgEditor>,
    pub(crate) path: Option<PathBuf>,
    dirty: bool,
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
        };
        window.set_window_title(&ws.title());
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
    fn sync_base_dir(&self, cx: &mut Context<Self>) {
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

    fn title(&self) -> String {
        let name = self
            .path
            .as_deref()
            .and_then(Path::file_name)
            .map_or("Welcome (unsaved sample)".into(), |n| {
                n.to_string_lossy().into_owned()
            });
        let folder = self
            .root
            .as_deref()
            .and_then(Path::file_name)
            .map(|n| format!(" — {}", n.to_string_lossy()))
            .unwrap_or_default();
        format!(
            "{}{name}{folder} — Aster",
            if self.dirty { "● " } else { "" }
        )
    }

    fn save_session(&self) {
        Session {
            root: self.root.clone(),
            file: self.path.clone(),
        }
        .save();
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

    fn save_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
            Err(err) => eprintln!("Save failed: {err:#}"),
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
            window.focus(&self.editor.focus_handle(cx));
            return;
        }
        let text = match read_org(&path) {
            Ok(text) => text,
            Err(err) => {
                eprintln!("Open failed: {err:#}");
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
        self.sync_base_dir(cx);
        self.sync_known_tags(cx);
        self.dirty = false;
        self.reveal_in_tree(&path);
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
    fn reveal_in_tree(&mut self, path: &Path) {
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
                None => eprintln!("No heading with ID {id} in this folder"),
            }
            return;
        }
        let path = self.resolve(target);
        if !path.exists() {
            eprintln!("Link target not found: {}", path.display());
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
                eprintln!("Could not watch {} for changes", root.display());
                return;
            };
            // Batch events, then update the index and the open file.
            while let Some(first) = changes.next().await {
                cx.background_executor().timer(WATCH_DEBOUNCE).await;
                let mut batch = vec![first];
                while let Ok(p) = changes.try_recv() {
                    batch.push(p);
                }
                let paths = watch::org_paths(batch);
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
                if index.covers(p) {
                    changed |= index.update_file(p);
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
                    this.close_picker(window, cx);
                    this.run_pick(target, window, cx);
                }
                PickerEvent::Dismissed => this.close_picker(window, cx),
            },
        );
        self.picker = Some((picker, sub));
        cx.notify();
    }

    fn close_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker.take().is_some() {
            window.focus(&self.editor.focus_handle(cx));
            cx.notify();
        }
    }

    fn run_pick(&mut self, target: PickTarget, window: &mut Window, cx: &mut Context<Self>) {
        match target {
            PickTarget::Command(i) => {
                if let Some((_, action)) = self.commands.get(i) {
                    let action = action.boxed_clone();
                    window.focus(&self.editor.focus_handle(cx));
                    window.dispatch_action(action, cx);
                }
            }
            PickTarget::File(path) => self.open_path(path, window, cx),
            PickTarget::Location(path, range) => self.open_at(path, range, window, cx),
        }
    }

    fn commands() -> Vec<(&'static str, Box<dyn Action>)> {
        let mut commands: Vec<(&'static str, Box<dyn Action>)> = vec![
            ("Open file…", Box::new(Open)),
            ("Open folder…", Box::new(OpenFolder)),
            ("Go to file…", Box::new(QuickOpen)),
            ("Search in all files…", Box::new(SearchAll)),
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
                let hint = window
                    .highest_precedence_binding_for_action_in(action.as_ref(), &target)
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
fn write_atomic(path: &Path, text: &str) -> anyhow::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let tmp = dir.join(format!(
        ".{}.aster-tmp",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));
    std::fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

impl Render for Workspace {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sidebar = if self.sidebar_open && self.root.is_some() {
            Some(self.render_sidebar(cx).into_any_element())
        } else {
            None
        };
        div()
            .size_full()
            .flex()
            .bg(rgb(0xffffff))
            .text_color(rgb(0x1f2937))
            .text_size(px(16.))
            .line_height(px(26.))
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
            .children(sidebar)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .children(self.find_bar.clone())
                    .child(div().flex_1().min_h_0().child(self.editor.clone())),
            )
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
