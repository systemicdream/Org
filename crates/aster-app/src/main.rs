//! Aster app shell: one window editing one Org file.
//!
//! Usage: `aster [path/to/file.org]` — without a path, opens a built-in sample.

mod palette;

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context as _;
use aster_editor::{EditorEvent, FindBar, FindBarEvent, OrgEditor};
use gpui::{
    App, Application, Bounds, Context, Entity, Focusable, KeyBinding, Menu, MenuItem,
    PathPromptOptions, Subscription, SystemMenuType, Task, Window, WindowBounds, WindowOptions,
    actions, div, prelude::*, px, rgb, size,
};
use palette::{CommandPalette, PaletteEvent};

actions!(
    aster,
    [
        Quit,
        Open,
        Save,
        SaveAs,
        Find,
        FindReplace,
        FindNext,
        FindPrevious,
        ShowCommandPalette
    ]
);

const SAMPLE: &str = include_str!("../assets/welcome.org");
const AUTOSAVE_DELAY: Duration = Duration::from_millis(1000);

struct Workspace {
    editor: Entity<OrgEditor>,
    path: Option<PathBuf>,
    dirty: bool,
    find_bar: Option<Entity<FindBar>>,
    autosave: Option<Task<()>>,
    _editor_subscription: Subscription,
    _find_subscription: Option<Subscription>,
    palette: Option<(Entity<CommandPalette>, Subscription)>,
}

impl Workspace {
    fn new(
        text: String,
        path: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (editor, sub) = Self::make_editor(text, window, cx);
        let ws = Self {
            editor,
            path,
            dirty: false,
            find_bar: None,
            autosave: None,
            _editor_subscription: sub,
            _find_subscription: None,
            palette: None,
        };
        window.set_window_title(&ws.title());
        ws.sync_base_dir(cx);
        ws
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
        format!("{}{name} — Aster", if self.dirty { "● " } else { "" })
    }

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
                cx.notify();
            }
            Err(err) => eprintln!("Save failed: {err:#}"),
        }
    }

    fn save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        if self.path.is_some() {
            self.save_now(window, cx);
        } else {
            self.save_as(&SaveAs, window, cx);
        }
    }

    fn save_as(&mut self, _: &SaveAs, window: &mut Window, cx: &mut Context<Self>) {
        let dir = self
            .path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
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
                })
                .ok();
            }
        })
        .detach();
    }

    fn open(&mut self, _: &Open, window: &mut Window, cx: &mut Context<Self>) {
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

    fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
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
        self.path = Some(path);
        self.sync_base_dir(cx);
        self.dirty = false;
        window.set_window_title(&self.title());
        window.focus(&self.editor.focus_handle(cx));
        cx.notify();
    }

    /// Open a link target: web/mail links in the browser, `.org` files here, other files with
    /// the system's default app.
    fn open_link(&mut self, target: &str, window: &mut Window, cx: &mut Context<Self>) {
        if target.contains("://") || target.starts_with("mailto:") {
            cx.open_url(target);
            return;
        }
        let path = target.strip_prefix("file:").unwrap_or(target);
        // Org allows `file:notes.org::*Heading` search options; ignore them for now.
        let path = path.split("::").next().unwrap_or(path);
        let path = match path.strip_prefix("~/") {
            Some(rest) => std::env::home_dir().unwrap_or_default().join(rest),
            None => PathBuf::from(path),
        };
        let path = if path.is_relative() {
            self.path
                .as_deref()
                .and_then(Path::parent)
                .map_or(path.clone(), |dir| dir.join(&path))
        } else {
            path
        };
        if !path.exists() {
            eprintln!("Link target not found: {}", path.display());
            return;
        }
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("org"))
        {
            self.open_path(path, window, cx);
        } else {
            cx.open_with_system(&path);
        }
    }

    fn quit(&mut self, _: &Quit, window: &mut Window, cx: &mut Context<Self>) {
        if self.dirty {
            self.save_now(window, cx);
        }
        cx.quit();
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

    // ---- command palette ------------------------------------------------------------------

    fn commands() -> Vec<(&'static str, Box<dyn gpui::Action>)> {
        let mut commands: Vec<(&'static str, Box<dyn gpui::Action>)> = vec![
            ("Open file…", Box::new(Open)),
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

    fn show_command_palette(
        &mut self,
        _: &ShowCommandPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.palette.is_some() {
            self.close_palette(window, cx);
            return;
        }
        let target = self.editor.focus_handle(cx);
        let palette = cx.new(|cx| CommandPalette::new(Self::commands(), target, window, cx));
        let sub = cx.subscribe_in(
            &palette,
            window,
            |this, _, event: &PaletteEvent, window, cx| match event {
                PaletteEvent::Run(action) => {
                    let action = action.boxed_clone();
                    this.close_palette(window, cx);
                    // Run against the editor (focused again by close_palette).
                    window.dispatch_action(action, cx);
                }
                PaletteEvent::Dismissed => this.close_palette(window, cx),
            },
        );
        self.palette = Some((palette, sub));
        cx.notify();
    }

    fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.take().is_some() {
            window.focus(&self.editor.focus_handle(cx));
            cx.notify();
        }
    }

    fn find(&mut self, _: &Find, window: &mut Window, cx: &mut Context<Self>) {
        self.show_find(false, window, cx);
    }

    fn find_replace(&mut self, _: &FindReplace, window: &mut Window, cx: &mut Context<Self>) {
        self.show_find(true, window, cx);
    }

    fn find_next(&mut self, _: &FindNext, window: &mut Window, cx: &mut Context<Self>) {
        match &self.find_bar {
            Some(bar) => bar.update(cx, |b, cx| b.next(false, cx)),
            None => self.show_find(false, window, cx),
        }
    }

    fn find_previous(&mut self, _: &FindPrevious, window: &mut Window, cx: &mut Context<Self>) {
        match &self.find_bar {
            Some(bar) => bar.update(cx, |b, cx| b.next(true, cx)),
            None => self.show_find(false, window, cx),
        }
    }
}

fn read_org(path: &Path) -> anyhow::Result<String> {
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
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0xffffff))
            .text_color(rgb(0x1f2937))
            .text_size(px(16.))
            .line_height(px(26.))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::save_as))
            .on_action(cx.listener(Self::open))
            .on_action(cx.listener(Self::quit))
            .on_action(cx.listener(Self::find))
            .on_action(cx.listener(Self::find_replace))
            .on_action(cx.listener(Self::find_next))
            .on_action(cx.listener(Self::find_previous))
            .on_action(cx.listener(Self::show_command_palette))
            .children(self.find_bar.clone())
            .child(div().flex_1().min_h_0().child(self.editor.clone()))
            .children(self.palette.as_ref().map(|(p, _)| {
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

fn menus() -> Vec<Menu> {
    use aster_editor as e;
    vec![
        Menu {
            name: "Aster".into(),
            items: vec![
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Quit Aster", Quit),
            ],
        },
        Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("Open…", Open),
                MenuItem::action("Save", Save),
                MenuItem::action("Save As…", SaveAs),
            ],
        },
        Menu {
            name: "Edit".into(),
            items: vec![
                MenuItem::action("Undo", e::Undo),
                MenuItem::action("Redo", e::Redo),
                MenuItem::separator(),
                MenuItem::action("Cut", e::Cut),
                MenuItem::action("Copy", e::Copy),
                MenuItem::action("Paste", e::Paste),
                MenuItem::action("Select All", e::SelectAll),
                MenuItem::separator(),
                MenuItem::action("Find…", Find),
                MenuItem::action("Find and Replace…", FindReplace),
                MenuItem::action("Find Next", FindNext),
                MenuItem::action("Find Previous", FindPrevious),
            ],
        },
        Menu {
            name: "View".into(),
            items: vec![MenuItem::action("Command Palette…", ShowCommandPalette)],
        },
        Menu {
            name: "Outline".into(),
            items: vec![
                MenuItem::action("Toggle Done", e::ToggleDone),
                MenuItem::action("New Heading", e::InsertHeading),
                MenuItem::action("Move Up", e::MoveSubtreeUp),
                MenuItem::action("Move Down", e::MoveSubtreeDown),
                MenuItem::action("Indent", e::DemoteSubtree),
                MenuItem::action("Outdent", e::PromoteSubtree),
                MenuItem::separator(),
                MenuItem::action("Fold", e::Fold),
                MenuItem::action("Unfold", e::Unfold),
                MenuItem::action("Fold All", e::FoldAll),
                MenuItem::action("Unfold All", e::UnfoldAll),
            ],
        },
    ]
}

fn main() -> anyhow::Result<()> {
    // Aster's Linux build only includes GPUI's Wayland backend. Without a Wayland session GPUI
    // would fall back to a headless platform and fail obscurely, so stop with a clear message.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WAYLAND_DISPLAY").is_none_or(|d| d.is_empty()) {
        anyhow::bail!(
            "Aster requires a Wayland session on Linux (WAYLAND_DISPLAY is not set). \
             X11 is not supported."
        );
    }

    let path = std::env::args_os().nth(1).map(PathBuf::from);
    let text = match &path {
        Some(p) if p.exists() => read_org(p)?,
        Some(_) => String::new(),
        None => SAMPLE.to_owned(),
    };

    Application::new().run(move |cx: &mut App| {
        aster_editor::bind_keys(cx);
        let mut keys = vec![
            KeyBinding::new("secondary-o", Open, None),
            KeyBinding::new("secondary-s", Save, None),
            KeyBinding::new("secondary-shift-s", SaveAs, None),
            KeyBinding::new("secondary-q", Quit, None),
            KeyBinding::new("secondary-f", Find, None),
            KeyBinding::new("secondary-g", FindNext, None),
            KeyBinding::new("secondary-shift-p", ShowCommandPalette, None),
            KeyBinding::new("secondary-shift-g", FindPrevious, None),
        ];
        keys.push(if cfg!(target_os = "macos") {
            KeyBinding::new("cmd-alt-f", FindReplace, None)
        } else {
            KeyBinding::new("ctrl-h", FindReplace, None)
        });
        cx.bind_keys(keys);
        cx.set_menus(menus());

        let bounds = Bounds::centered(None, size(px(900.), px(700.)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| cx.new(|cx| Workspace::new(text, path, window, cx)),
            )
            .expect("failed to open window");

        window
            .update(cx, |ws, window, cx| {
                window.focus(&ws.editor.focus_handle(cx));
                cx.activate(true);
            })
            .ok();
    });
    Ok(())
}
