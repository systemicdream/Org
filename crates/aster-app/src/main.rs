//! Aster app shell (Phase 0): one window editing one Org file.
//!
//! Usage: `aster [path/to/file.org]` — without a path, opens a built-in sample.

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use aster_editor::{Edited, OrgEditor};
use gpui::{
    App, Application, Bounds, Context, Entity, Focusable, KeyBinding, Menu, MenuItem,
    SystemMenuType, Window, WindowBounds, WindowOptions, actions, div, prelude::*, px, rgb, size,
};

actions!(aster, [Quit, Save]);

const SAMPLE: &str = include_str!("../assets/welcome.org");

struct Workspace {
    editor: Entity<OrgEditor>,
    path: Option<PathBuf>,
    dirty: bool,
}

impl Workspace {
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

    fn save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.path.clone() else {
            eprintln!("Save: the sample has no file path; launch with `aster <file.org>`");
            return;
        };
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
            .bg(rgb(0xffffff))
            .text_color(rgb(0x1f2937))
            .text_size(px(16.))
            .line_height(px(26.))
            .on_action(cx.listener(Self::save))
            .child(self.editor.clone())
    }
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
        Some(p) if p.exists() => {
            let bytes = std::fs::read(p).with_context(|| format!("reading {}", p.display()))?;
            String::from_utf8(bytes).context("Aster currently only opens UTF-8 files")?
        }
        Some(_) => String::new(),
        None => SAMPLE.to_owned(),
    };

    Application::new().run(move |cx: &mut App| {
        aster_editor::bind_keys(cx);
        cx.bind_keys([
            KeyBinding::new("secondary-s", Save, None),
            KeyBinding::new("secondary-q", Quit, None),
        ]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.set_menus(vec![
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
                items: vec![MenuItem::action("Save", Save)],
            },
        ]);

        let bounds = Bounds::centered(None, size(px(900.), px(700.)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| {
                    let editor = cx.new(|cx| OrgEditor::new(text, cx));
                    cx.new(|cx| {
                        cx.subscribe_in(
                            &editor,
                            window,
                            |this: &mut Workspace, _, _: &Edited, window, cx| {
                                if !this.dirty {
                                    this.dirty = true;
                                    window.set_window_title(&this.title());
                                }
                                cx.notify();
                            },
                        )
                        .detach();
                        let ws = Workspace {
                            editor,
                            path,
                            dirty: false,
                        };
                        window.set_window_title(&ws.title());
                        ws
                    })
                },
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
