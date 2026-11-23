//! Aster app shell: a notes folder with a sidebar, and an Org editor for the open file.
//!
//! Usage: `aster [folder | file.org]`. A folder becomes the workspace; a file opens with its
//! folder as the workspace. Without arguments Aster reopens the last folder and file, or shows
//! the first-run screen.

mod agenda_view;
mod app_icon;
mod capture;
mod file_actions;
mod files;
mod keymap;
mod name_dialog;
mod notebooks;
mod notices;
mod picker;
mod release;
mod session;
mod settings;
mod settings_view;
mod sidebar;
mod watch;
mod welcome;
mod window_blur;
mod workspace;

use std::path::PathBuf;

use gpui::{
    App, Application, Bounds, Menu, MenuItem, SystemMenuType, TitlebarOptions, WindowBounds,
    WindowOptions, actions, point, prelude::*, px, size,
};

use session::Session;
use settings::Settings;
use workspace::{Workspace, read_org};

actions!(
    aster,
    [
        Quit,
        Open,
        OpenFolder,
        QuickOpen,
        SearchAll,
        ToggleSidebar,
        Save,
        SaveAs,
        Find,
        FindReplace,
        FindNext,
        FindPrevious,
        ShowCommandPalette,
        ShowAgenda,
        ShowTodoList,
        ShowSettings,
        Capture
    ]
);

pub(crate) fn menus() -> Vec<Menu> {
    use aster_editor as e;
    vec![
        Menu {
            name: "Aster".into(),
            items: vec![
                MenuItem::action("Settings…", ShowSettings),
                MenuItem::separator(),
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Quit Aster", Quit),
            ],
        },
        Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("Capture…", Capture),
                MenuItem::separator(),
                MenuItem::action("Open…", Open),
                MenuItem::action("Open Folder…", OpenFolder),
                MenuItem::action("Go to File…", QuickOpen),
                MenuItem::separator(),
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
                MenuItem::action("Search All Files…", SearchAll),
            ],
        },
        Menu {
            name: "View".into(),
            items: vec![
                MenuItem::action("Command Palette…", ShowCommandPalette),
                MenuItem::action("Agenda", ShowAgenda),
                MenuItem::action("To-do List", ShowTodoList),
                MenuItem::action("Toggle Sidebar", ToggleSidebar),
            ],
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
                MenuItem::action("Refile…", e::Refile),
                MenuItem::action("Archive", e::ArchiveSubtree),
                MenuItem::separator(),
                MenuItem::action("Fold", e::Fold),
                MenuItem::action("Unfold", e::Unfold),
                MenuItem::action("Fold All", e::FoldAll),
                MenuItem::action("Unfold All", e::UnfoldAll),
            ],
        },
    ]
}

/// What to open at startup: (file to edit, workspace folder).
fn startup_target() -> (Option<PathBuf>, Option<PathBuf>) {
    let canonical = |p: PathBuf| std::fs::canonicalize(&p).unwrap_or(p);
    match std::env::args_os().nth(1).map(PathBuf::from) {
        Some(p) if p.is_dir() => (None, Some(canonical(p))),
        Some(p) => {
            let p = if p.exists() { canonical(p) } else { p };
            // Keep the remembered notes folder if it contains this file.
            let root = Session::load()
                .root
                .filter(|r| r.is_dir() && p.starts_with(r))
                .or_else(|| p.parent().map(PathBuf::from));
            (Some(p), root)
        }
        None => {
            let s = Session::load();
            let root = s.root.filter(|r| r.is_dir());
            let file = s.file.filter(|f| f.is_file());
            (file, root)
        }
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

    release::install_panic_hook();
    let (file, root) = startup_target();
    let text = match &file {
        Some(p) if p.exists() => read_org(p)?,
        Some(_) => String::new(),
        None => String::new(),
    };

    Application::new()
        .with_assets(app_icon::Assets)
        .run(move |cx: &mut App| {
            app_icon::set_dock_icon();
            let settings = Settings::load();
            let window_background = settings.window_background();
            aster_ui::init(cx.window_appearance(), settings.theme_prefs(), cx);
            keymap::apply(settings.keymap, cx);
            cx.set_global(settings);

            let bounds = Bounds::centered(None, size(px(1100.), px(720.)), cx);
            let window = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        window_background,
                        // Wayland: ties the window to the desktop entry, and so to its icon.
                        app_id: Some(app_icon::APP_ID.into()),
                        // macOS: draw our own title area (docs/DESIGN.md §11).
                        titlebar: Some(TitlebarOptions {
                            title: Some("Aster".into()),
                            appears_transparent: cfg!(target_os = "macos"),
                            traffic_light_position: Some(point(px(14.), px(13.))),
                        }),
                        ..Default::default()
                    },
                    |window, cx| cx.new(|cx| Workspace::new(text, file, root, window, cx)),
                )
                .expect("failed to open window");

            window
                .update(cx, |ws, window, cx| {
                    ws.focus_main(window, cx);
                    ws.startup_notices(cx);
                    cx.activate(true);
                })
                .ok();
            window_blur::sync(window_background == gpui::WindowBackgroundAppearance::Blurred);
        });
    Ok(())
}
