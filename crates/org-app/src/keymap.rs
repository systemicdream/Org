//! Key bindings: the standard keymap (platform conventions), and the optional Emacs keymap
//! layered on top of it. Keymaps are data: switching rebinds everything at runtime.

use gpui::{App, KeyBinding};
use org_editor as e;

use crate::settings::Keymap;
use crate::*;

/// The editor's key context (Emacs bindings apply there, ahead of the standard ones).
const EDITOR: Option<&str> = Some("OrgEditor");

/// Rebind every key for `keymap` and refresh the menus' shortcuts.
pub fn apply(keymap: Keymap, cx: &mut App) {
    cx.clear_key_bindings();
    org_editor::bind_keys(cx);
    agenda_view::bind_keys(cx);
    settings_view::bind_keys(cx);
    cx.bind_keys(standard());
    match keymap {
        Keymap::Standard => {}
        Keymap::Emacs => cx.bind_keys(emacs()),
        Keymap::Vim => {
            cx.bind_keys(org_editor::vim::bindings());
            cx.bind_keys(vim());
        }
    }
    org_editor::vim::set_enabled(keymap == Keymap::Vim, cx);
    cx.set_menus(menus());
}

fn standard() -> Vec<KeyBinding> {
    let mut keys = vec![
        KeyBinding::new("secondary-o", Open, None),
        KeyBinding::new("secondary-shift-o", OpenFolder, None),
        KeyBinding::new("secondary-p", QuickOpen, None),
        KeyBinding::new("secondary-shift-f", SearchAll, None),
        KeyBinding::new("secondary-\\", ToggleSidebar, None),
        KeyBinding::new("secondary-shift-a", ShowAgenda, None),
        KeyBinding::new("secondary-shift-t", ShowTodoList, None),
        KeyBinding::new("secondary-n", Capture, None),
        KeyBinding::new("secondary-shift-r", e::Refile, None),
        KeyBinding::new("secondary-s", Save, None),
        KeyBinding::new("secondary-shift-s", SaveAs, None),
        KeyBinding::new("secondary-q", Quit, None),
        KeyBinding::new("secondary-f", Find, None),
        KeyBinding::new("secondary-g", FindNext, None),
        KeyBinding::new("secondary-shift-p", ShowCommandPalette, None),
        KeyBinding::new("secondary-shift-g", FindPrevious, None),
        KeyBinding::new("secondary-,", ShowSettings, None),
        KeyBinding::new("escape", Dismiss, Some("Workspace")),
    ];
    keys.push(if cfg!(target_os = "macos") {
        KeyBinding::new("cmd-alt-f", FindReplace, None)
    } else {
        KeyBinding::new("ctrl-h", FindReplace, None)
    });
    keys
}

/// Emacs and Org mode bindings. Movement and editing apply in the editor; `C-x`/`C-c`
/// prefixes and `M-x` work everywhere. Standard shortcuts that don't clash keep working.
fn emacs() -> Vec<KeyBinding> {
    vec![
        // Movement and editing
        KeyBinding::new("ctrl-f", e::Right, EDITOR),
        KeyBinding::new("ctrl-b", e::Left, EDITOR),
        KeyBinding::new("ctrl-n", e::Down, EDITOR),
        KeyBinding::new("ctrl-p", e::Up, EDITOR),
        KeyBinding::new("ctrl-a", e::LineStart, EDITOR),
        KeyBinding::new("ctrl-e", e::LineEnd, EDITOR),
        KeyBinding::new("alt-f", e::WordRight, EDITOR),
        KeyBinding::new("alt-b", e::WordLeft, EDITOR),
        KeyBinding::new("alt-<", e::DocStart, EDITOR),
        KeyBinding::new("alt->", e::DocEnd, EDITOR),
        KeyBinding::new("ctrl-d", e::Delete, EDITOR),
        KeyBinding::new("alt-d", e::DeleteWordRight, EDITOR),
        KeyBinding::new("alt-backspace", e::DeleteWordLeft, EDITOR),
        KeyBinding::new("ctrl-k", e::KillLine, EDITOR),
        KeyBinding::new("ctrl-w", e::Cut, EDITOR),
        KeyBinding::new("alt-w", e::Copy, EDITOR),
        KeyBinding::new("ctrl-y", e::Paste, EDITOR),
        KeyBinding::new("ctrl-/", e::Undo, EDITOR),
        KeyBinding::new("ctrl-g", e::Cancel, EDITOR),
        KeyBinding::new("ctrl-s", Find, EDITOR),
        // Org structure editing
        KeyBinding::new("tab", e::OrgCycle, EDITOR),
        KeyBinding::new("shift-tab", e::OrgGlobalCycle, EDITOR),
        KeyBinding::new("alt-enter", e::InsertHeading, EDITOR),
        KeyBinding::new("alt-left", e::PromoteHeading, EDITOR),
        KeyBinding::new("alt-right", e::DemoteHeading, EDITOR),
        KeyBinding::new("alt-shift-left", e::PromoteSubtree, EDITOR),
        KeyBinding::new("alt-shift-right", e::DemoteSubtree, EDITOR),
        KeyBinding::new("alt-up", e::MoveSubtreeUp, EDITOR),
        KeyBinding::new("alt-down", e::MoveSubtreeDown, EDITOR),
        KeyBinding::new("ctrl-c ctrl-t", e::ToggleDone, EDITOR),
        KeyBinding::new("ctrl-c ctrl-c", e::ToggleDone, EDITOR),
        KeyBinding::new("ctrl-c ctrl-w", e::Refile, EDITOR),
        KeyBinding::new("ctrl-c ctrl-x ctrl-a", e::ArchiveSubtree, EDITOR),
        // Files and views
        KeyBinding::new("ctrl-x ctrl-s", Save, None),
        KeyBinding::new("ctrl-x ctrl-w", SaveAs, None),
        KeyBinding::new("ctrl-x ctrl-f", Open, None),
        KeyBinding::new("ctrl-x b", QuickOpen, None),
        KeyBinding::new("ctrl-x ctrl-c", Quit, None),
        KeyBinding::new("ctrl-c a", ShowAgenda, None),
        KeyBinding::new("ctrl-c c", Capture, None),
        KeyBinding::new("alt-x", ShowCommandPalette, None),
    ]
}

/// App commands from Vim's Normal mode: search, and `:` for commands.
fn vim() -> Vec<KeyBinding> {
    let n = Some("OrgEditor && vim_normal");
    vec![
        KeyBinding::new("/", Find, n),
        KeyBinding::new("n", FindNext, n),
        KeyBinding::new("shift-n", FindPrevious, n),
        KeyBinding::new(":", ShowCommandPalette, n),
        KeyBinding::new("shift-;", ShowCommandPalette, n),
    ]
}

/// The main Vim keys, for the settings page: (keys, what they do).
pub const VIM_SUMMARY: &[(&str, &str)] = &[
    ("i a I A o O", "Insert mode (Esc returns to Normal mode)"),
    ("h j k l w b e 0 ^ $ gg G", "Move"),
    ("v V", "Visual mode by character or by line; then d y c"),
    (
        "x dd dw D cc cw C yy p P J",
        "Delete, change, yank (copy), paste, join",
    ),
    ("u Ctrl-r", "Undo, redo"),
    (
        "TAB S-TAB za zM zR",
        "Fold the heading; fold or unfold everything",
    ),
    (">> << t", "Demote, promote, cycle the TODO state"),
    ("/ n N :", "Find, next, previous, all commands"),
];

/// A few Emacs bindings to show on the settings page: (keys, what they do).
pub const EMACS_SUMMARY: &[(&str, &str)] = &[
    (
        "C-f C-b C-n C-p C-a C-e",
        "Move by character, line, line start/end",
    ),
    ("M-f M-b M-< M->", "Move by word, to the start/end"),
    ("C-k C-w M-w C-y C-/", "Kill line, cut, copy, yank, undo"),
    ("TAB S-TAB", "Fold the heading; fold or unfold everything"),
    (
        "M-RET M-← M-→ M-↑ M-↓",
        "New heading, promote, demote, move",
    ),
    ("C-c C-t", "Cycle the TODO state"),
    ("C-c C-w  C-c C-x C-a", "Refile, archive"),
    ("C-x C-s C-x C-f C-x b", "Save, open file, switch file"),
    ("C-c a  C-c c  M-x", "Agenda, capture, all commands"),
];
