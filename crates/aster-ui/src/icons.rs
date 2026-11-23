//! Icons: Fluent UI System Icons (MIT), Regular style, bundled as SVG (docs/DESIGN.md §8).
//!
//! The SVGs are embedded in the binary and served to GPUI through [`Assets`]. Draw them with
//! [`icon`], which colors them with the given color (they're monochrome masks).

use std::borrow::Cow;

use gpui::{AssetSource, Hsla, Pixels, SharedString, Svg, prelude::*, svg};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    Agenda,
    TodoList,
    Document,
    Folder,
    FolderOpen,
    ChevronRight,
    ChevronDown,
    ChevronLeft,
    ChevronUp,
    Tag,
    Search,
    Dismiss,
    ArrowUp,
    ArrowDown,
    Checkmark,
    CheckmarkCircle,
    ErrorCircle,
    Info,
    Warning,
    Add,
    Settings,
    PanelLeft,
    CalendarToday,
    Archive,
    ArrowMove,
    BulletList,
    NumberList,
    Table,
    Code,
    Quote,
    Link,
    Divider,
    Heading1,
    Heading2,
    Checkbox,
    Schedule,
    Deadline,
    Notebook,
    Inbox,
    DocumentAdd,
    FolderAdd,
    Rename,
    Delete,
    Edit,
    NoteAdd,
    Flag,
    More,
    Open,
    Sync,
    Theme,
    Color,
    TextSize,
    Keyboard,
    Reset,
    Transparency,
}

impl Icon {
    pub fn path(self) -> &'static str {
        match self {
            Icon::Agenda => "icons/calendar_ltr.svg",
            Icon::TodoList => "icons/task_list_ltr.svg",
            Icon::Document => "icons/document.svg",
            Icon::Folder => "icons/folder.svg",
            Icon::FolderOpen => "icons/folder_open.svg",
            Icon::ChevronRight => "icons/chevron_right.svg",
            Icon::ChevronDown => "icons/chevron_down.svg",
            Icon::ChevronLeft => "icons/chevron_left.svg",
            Icon::ChevronUp => "icons/chevron_up.svg",
            Icon::Tag => "icons/tag.svg",
            Icon::Search => "icons/search.svg",
            Icon::Dismiss => "icons/dismiss.svg",
            Icon::ArrowUp => "icons/arrow_up.svg",
            Icon::ArrowDown => "icons/arrow_down.svg",
            Icon::Checkmark => "icons/checkmark.svg",
            Icon::CheckmarkCircle => "icons/checkmark_circle.svg",
            Icon::ErrorCircle => "icons/error_circle.svg",
            Icon::Info => "icons/info.svg",
            Icon::Warning => "icons/warning.svg",
            Icon::Add => "icons/add.svg",
            Icon::Settings => "icons/settings.svg",
            Icon::PanelLeft => "icons/panel_left.svg",
            Icon::CalendarToday => "icons/calendar_today.svg",
            Icon::Archive => "icons/archive.svg",
            Icon::ArrowMove => "icons/arrow_move.svg",
            Icon::BulletList => "icons/text_bullet_list_ltr.svg",
            Icon::NumberList => "icons/text_number_list_ltr.svg",
            Icon::Table => "icons/table.svg",
            Icon::Code => "icons/code.svg",
            Icon::Quote => "icons/text_quote.svg",
            Icon::Link => "icons/link.svg",
            Icon::Divider => "icons/line_horizontal_1.svg",
            Icon::Heading1 => "icons/text_header_1.svg",
            Icon::Heading2 => "icons/text_header_2.svg",
            Icon::Checkbox => "icons/checkbox_checked.svg",
            Icon::Schedule => "icons/calendar_clock.svg",
            Icon::Deadline => "icons/clock_alarm.svg",
            Icon::Notebook => "icons/notebook.svg",
            Icon::Inbox => "icons/mail_inbox.svg",
            Icon::DocumentAdd => "icons/document_add.svg",
            Icon::FolderAdd => "icons/folder_add.svg",
            Icon::Rename => "icons/rename.svg",
            Icon::Delete => "icons/delete.svg",
            Icon::Edit => "icons/edit.svg",
            Icon::NoteAdd => "icons/note_add.svg",
            Icon::Flag => "icons/flag.svg",
            Icon::More => "icons/more_horizontal.svg",
            Icon::Open => "icons/open.svg",
            Icon::Sync => "icons/arrow_sync.svg",
            Icon::Theme => "icons/dark_theme.svg",
            Icon::Color => "icons/color.svg",
            Icon::TextSize => "icons/text_font_size.svg",
            Icon::Keyboard => "icons/keyboard.svg",
            Icon::Reset => "icons/arrow_reset.svg",
            Icon::Transparency => "icons/blur.svg",
        }
    }
}

const ICONS: &[(&str, &[u8])] = &[
    (
        "icons/calendar_ltr.svg",
        include_bytes!("../assets/icons/calendar_ltr.svg"),
    ),
    (
        "icons/task_list_ltr.svg",
        include_bytes!("../assets/icons/task_list_ltr.svg"),
    ),
    (
        "icons/document.svg",
        include_bytes!("../assets/icons/document.svg"),
    ),
    (
        "icons/folder.svg",
        include_bytes!("../assets/icons/folder.svg"),
    ),
    (
        "icons/folder_open.svg",
        include_bytes!("../assets/icons/folder_open.svg"),
    ),
    (
        "icons/chevron_right.svg",
        include_bytes!("../assets/icons/chevron_right.svg"),
    ),
    (
        "icons/chevron_down.svg",
        include_bytes!("../assets/icons/chevron_down.svg"),
    ),
    (
        "icons/chevron_left.svg",
        include_bytes!("../assets/icons/chevron_left.svg"),
    ),
    (
        "icons/chevron_up.svg",
        include_bytes!("../assets/icons/chevron_up.svg"),
    ),
    ("icons/tag.svg", include_bytes!("../assets/icons/tag.svg")),
    (
        "icons/search.svg",
        include_bytes!("../assets/icons/search.svg"),
    ),
    (
        "icons/dismiss.svg",
        include_bytes!("../assets/icons/dismiss.svg"),
    ),
    (
        "icons/arrow_up.svg",
        include_bytes!("../assets/icons/arrow_up.svg"),
    ),
    (
        "icons/arrow_down.svg",
        include_bytes!("../assets/icons/arrow_down.svg"),
    ),
    (
        "icons/checkmark.svg",
        include_bytes!("../assets/icons/checkmark.svg"),
    ),
    (
        "icons/checkmark_circle.svg",
        include_bytes!("../assets/icons/checkmark_circle.svg"),
    ),
    (
        "icons/error_circle.svg",
        include_bytes!("../assets/icons/error_circle.svg"),
    ),
    ("icons/info.svg", include_bytes!("../assets/icons/info.svg")),
    (
        "icons/warning.svg",
        include_bytes!("../assets/icons/warning.svg"),
    ),
    ("icons/add.svg", include_bytes!("../assets/icons/add.svg")),
    (
        "icons/settings.svg",
        include_bytes!("../assets/icons/settings.svg"),
    ),
    (
        "icons/panel_left.svg",
        include_bytes!("../assets/icons/panel_left.svg"),
    ),
    (
        "icons/calendar_today.svg",
        include_bytes!("../assets/icons/calendar_today.svg"),
    ),
    (
        "icons/archive.svg",
        include_bytes!("../assets/icons/archive.svg"),
    ),
    (
        "icons/arrow_move.svg",
        include_bytes!("../assets/icons/arrow_move.svg"),
    ),
    (
        "icons/text_bullet_list_ltr.svg",
        include_bytes!("../assets/icons/text_bullet_list_ltr.svg"),
    ),
    (
        "icons/text_number_list_ltr.svg",
        include_bytes!("../assets/icons/text_number_list_ltr.svg"),
    ),
    (
        "icons/table.svg",
        include_bytes!("../assets/icons/table.svg"),
    ),
    ("icons/code.svg", include_bytes!("../assets/icons/code.svg")),
    (
        "icons/text_quote.svg",
        include_bytes!("../assets/icons/text_quote.svg"),
    ),
    ("icons/link.svg", include_bytes!("../assets/icons/link.svg")),
    (
        "icons/line_horizontal_1.svg",
        include_bytes!("../assets/icons/line_horizontal_1.svg"),
    ),
    (
        "icons/text_header_1.svg",
        include_bytes!("../assets/icons/text_header_1.svg"),
    ),
    (
        "icons/text_header_2.svg",
        include_bytes!("../assets/icons/text_header_2.svg"),
    ),
    (
        "icons/checkbox_checked.svg",
        include_bytes!("../assets/icons/checkbox_checked.svg"),
    ),
    (
        "icons/calendar_clock.svg",
        include_bytes!("../assets/icons/calendar_clock.svg"),
    ),
    (
        "icons/clock_alarm.svg",
        include_bytes!("../assets/icons/clock_alarm.svg"),
    ),
    (
        "icons/notebook.svg",
        include_bytes!("../assets/icons/notebook.svg"),
    ),
    (
        "icons/mail_inbox.svg",
        include_bytes!("../assets/icons/mail_inbox.svg"),
    ),
    (
        "icons/document_add.svg",
        include_bytes!("../assets/icons/document_add.svg"),
    ),
    (
        "icons/folder_add.svg",
        include_bytes!("../assets/icons/folder_add.svg"),
    ),
    (
        "icons/rename.svg",
        include_bytes!("../assets/icons/rename.svg"),
    ),
    (
        "icons/delete.svg",
        include_bytes!("../assets/icons/delete.svg"),
    ),
    ("icons/edit.svg", include_bytes!("../assets/icons/edit.svg")),
    (
        "icons/note_add.svg",
        include_bytes!("../assets/icons/note_add.svg"),
    ),
    ("icons/flag.svg", include_bytes!("../assets/icons/flag.svg")),
    (
        "icons/more_horizontal.svg",
        include_bytes!("../assets/icons/more_horizontal.svg"),
    ),
    ("icons/open.svg", include_bytes!("../assets/icons/open.svg")),
    (
        "icons/arrow_sync.svg",
        include_bytes!("../assets/icons/arrow_sync.svg"),
    ),
    (
        "icons/dark_theme.svg",
        include_bytes!("../assets/icons/dark_theme.svg"),
    ),
    (
        "icons/color.svg",
        include_bytes!("../assets/icons/color.svg"),
    ),
    (
        "icons/text_font_size.svg",
        include_bytes!("../assets/icons/text_font_size.svg"),
    ),
    (
        "icons/keyboard.svg",
        include_bytes!("../assets/icons/keyboard.svg"),
    ),
    (
        "icons/arrow_reset.svg",
        include_bytes!("../assets/icons/arrow_reset.svg"),
    ),
    ("icons/blur.svg", include_bytes!("../assets/icons/blur.svg")),
];

/// GPUI asset source for Aster's bundled assets (icons).
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS
            .iter()
            .find(|(p, _)| *p == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .filter(|(p, _)| p.starts_with(path))
            .map(|(p, _)| SharedString::from(*p))
            .collect())
    }
}

/// An icon at `size` (16 px in lists and menus, 20 px in toolbars) in `color`.
pub fn icon(icon: Icon, size: Pixels, color: Hsla) -> Svg {
    svg()
        .path(icon.path())
        .size(size)
        .flex_shrink_0()
        .text_color(color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_is_bundled_and_looks_like_svg() {
        for (path, bytes) in ICONS {
            let text = std::str::from_utf8(bytes).unwrap();
            assert!(text.contains("<svg") && text.contains("<path"), "{path}");
        }
        let a = Assets;
        assert!(a.load(Icon::Agenda.path()).unwrap().is_some());
        assert!(a.load("icons/nope.svg").unwrap().is_none());
    }
}
