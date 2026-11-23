//! The settings page (docs/DESIGN.md §7): Windows 11-style setting cards for appearance, the
//! editor and capture templates. Changes apply and save immediately; there's no "OK" button.

use aster_editor::{EditorEvent, OrgEditor};
use aster_ui::components::{self, ButtonKind};
use aster_ui::icons::{Icon, icon};
use aster_ui::{ActiveTheme, RADIUS_CONTROL, ramp};
use gpui::{
    AnyElement, App, Context, Div, Entity, EventEmitter, FocusHandle, Focusable, KeyBinding,
    MouseButton, Rgba, Subscription, Window, actions, div, prelude::*, px,
};

use crate::keymap::{EMACS_SUMMARY, VIM_SUMMARY};
use crate::settings::{
    self, AppearancePref, CaptureKind, CaptureTemplate, FONT_SIZE_DEFAULT, FONT_SIZE_MAX,
    FONT_SIZE_MIN, Keymap, Settings,
};

actions!(settings_page, [CloseSettings]);

const CONTEXT: &str = "Settings";

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("escape", CloseSettings, Some(CONTEXT))]);
}

pub enum SettingsEvent {
    Close,
    /// Saving the settings file failed.
    Error(String),
}

/// One capture template's editable fields.
struct TemplateRow {
    kind: CaptureKind,
    name: Entity<OrgEditor>,
    file: Entity<OrgEditor>,
    heading: Entity<OrgEditor>,
}

pub struct SettingsView {
    focus_handle: FocusHandle,
    templates: Vec<TemplateRow>,
    _subscriptions: Vec<Subscription>,
    /// The custom accent's hex field, and whether it's showing.
    custom_hex: Entity<OrgEditor>,
    show_custom: bool,
    _custom_subscription: Subscription,
}

/// Whether `accent` (a saved `#rrggbb`) is one of the preset swatches.
fn is_preset(accent: &str) -> bool {
    aster_ui::accent::parse_hex(accent).is_some_and(|a| {
        let hex = aster_ui::accent::to_hex(a);
        aster_ui::accent::SWATCHES
            .iter()
            .any(|(_, c)| aster_ui::accent::to_hex(gpui::rgb(*c)) == hex)
    })
}

impl EventEmitter<SettingsEvent> for SettingsView {}

impl SettingsView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let accent = cx.global::<Settings>().accent.clone();
        let custom_hex = cx.new(|cx| {
            let mut e = OrgEditor::single_line("#0078d4", window, cx);
            e.set_text(accent.as_deref().unwrap_or(""), cx);
            e
        });
        let custom_sub = cx.subscribe_in(
            &custom_hex,
            window,
            |this, input, event: &EditorEvent, window, cx| match event {
                EditorEvent::Edited => {
                    // Apply as soon as it's a valid color.
                    let text = input.read(cx).text().trim().to_string();
                    if let Some(c) = aster_ui::accent::parse_hex(&text) {
                        let hex = aster_ui::accent::to_hex(c);
                        this.change(cx, move |s| s.accent = Some(hex));
                    }
                    cx.notify();
                }
                EditorEvent::Cancel | EditorEvent::Confirm { .. } => {
                    window.focus(&this.focus_handle);
                    cx.notify();
                }
                _ => {}
            },
        );
        let mut view = Self {
            focus_handle: cx.focus_handle(),
            templates: Vec::new(),
            _subscriptions: Vec::new(),
            show_custom: accent.as_deref().is_some_and(|a| !is_preset(a)),
            custom_hex,
            _custom_subscription: custom_sub,
        };
        view.load_templates(window, cx);
        view
    }

    /// (Re)build the template fields from the saved settings.
    fn load_templates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let templates = cx.global::<Settings>().capture_templates.clone();
        self.templates.clear();
        self._subscriptions.clear();
        for t in templates {
            let mut field = |placeholder: &'static str, text: &str, cx: &mut Context<Self>| {
                let editor = cx.new(|cx| {
                    let mut e = OrgEditor::single_line(placeholder, window, cx);
                    e.set_text(text, cx);
                    e
                });
                let sub = cx.subscribe_in(
                    &editor,
                    window,
                    |this, _, event: &EditorEvent, window, cx| match event {
                        EditorEvent::Edited => this.save_templates(cx),
                        // Esc or Enter leaves the field (Esc again closes the page).
                        EditorEvent::Cancel | EditorEvent::Confirm { .. } => {
                            window.focus(&this.focus_handle);
                            cx.notify();
                        }
                        _ => {}
                    },
                );
                self._subscriptions.push(sub);
                editor
            };
            let name = field("Name", &t.name, cx);
            let file = field("inbox.org", &t.file, cx);
            let heading = field("Top level", &t.heading, cx);
            self.templates.push(TemplateRow {
                kind: t.kind,
                name,
                file,
                heading,
            });
        }
        cx.notify();
    }

    fn save_templates(&mut self, cx: &mut Context<Self>) {
        let templates: Vec<CaptureTemplate> = self
            .templates
            .iter()
            .map(|r| CaptureTemplate {
                name: r.name.read(cx).text().trim().to_string(),
                kind: r.kind,
                file: r.file.read(cx).text().trim().to_string(),
                heading: r.heading.read(cx).text().trim().to_string(),
            })
            .collect();
        self.change(cx, move |s| s.capture_templates = templates);
    }

    fn change(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Settings)) {
        if let Err(err) = settings::update(cx, f) {
            cx.emit(SettingsEvent::Error(format!(
                "Couldn't save settings: {err:#}"
            )));
        }
        cx.notify();
    }

    fn close(&mut self, _: &CloseSettings, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(SettingsEvent::Close);
    }

    // ---- building blocks --------------------------------------------------------------

    /// A settings card: icon, title and description on the left, the control on the right.
    fn card(
        glyph: Icon,
        title: &'static str,
        description: &'static str,
        control: impl IntoElement,
        cx: &App,
    ) -> Div {
        let c = cx.theme().colors;
        card_frame(cx).child(
            div()
                .min_h(px(68.))
                .px_4()
                .py_3()
                .flex()
                .items_center()
                .gap_4()
                .child(icon(glyph, px(20.), c.text_primary))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(title)
                        .child(components::caption(description, cx).text_color(c.text_secondary)),
                )
                .child(control),
        )
    }

    /// A row of selector tabs; `on_pick` gets the chosen index.
    fn choice<T: Copy + PartialEq + 'static>(
        id: &'static str,
        options: &[(&'static str, T)],
        current: T,
        cx: &mut Context<Self>,
        on_pick: fn(&mut Settings, T),
    ) -> Div {
        div()
            .flex()
            .gap_1()
            .children(options.iter().enumerate().map(|(i, &(label, value))| {
                components::selector_tab((id, i), label, value == current, cx).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| this.change(cx, |s| on_pick(s, value))),
                )
            }))
    }

    fn swatch(
        &self,
        i: usize,
        color: Option<Rgba>,
        label: &'static str,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = cx.theme().colors;
        // "System" shows the system's own accent (or Aster's blue when it has none).
        let fill =
            color
                .or(cx.theme().system_accent())
                .map_or(aster_ui::Colors::light().accent, |rgba| {
                    let mut h: gpui::Hsla = rgba.into();
                    h.a = 1.;
                    h
                });
        let value = color.map(aster_ui::accent::to_hex);
        div()
            .id(("swatch", i))
            .size(px(32.))
            .rounded(RADIUS_CONTROL)
            .border_2()
            .border_color(if selected {
                c.text_primary
            } else {
                gpui::transparent_black()
            })
            .p(px(2.))
            .cursor_pointer()
            .tooltip(move |window, cx| components::Tooltip::text(label, window, cx))
            .child(
                div()
                    .size_full()
                    .rounded(px(2.))
                    .bg(fill)
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(selected, |d| {
                        d.child(icon(
                            Icon::Checkmark,
                            px(16.),
                            aster_ui::accent::on_accent(fill),
                        ))
                    }),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                let value = value.clone();
                this.show_custom = false;
                this.change(cx, |s| s.accent = value);
            }))
            .into_any_element()
    }

    fn template_card(
        &self,
        i: usize,
        row: &TemplateRow,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let c = cx.theme().colors;
        let field = |label: &'static str, editor: &Entity<OrgEditor>, cx: &App| {
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_1()
                .child(components::caption(label, cx).text_color(c.text_secondary))
                .child(
                    components::text_box(editor.focus_handle(cx).is_focused(window), cx)
                        .child(editor.clone()),
                )
        };
        let kinds = div()
            .flex()
            .gap_1()
            .children(CaptureKind::ALL.iter().map(|&k| {
                components::selector_tab(
                    ("template-kind", i * 8 + k as usize),
                    k.label(),
                    k == row.kind,
                    cx,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.templates[i].kind = k;
                        this.save_templates(cx);
                    }),
                )
            }));
        let journal = row.kind == CaptureKind::Journal;
        let can_delete = self.templates.len() > 1;
        card_frame(cx).child(
            div()
                .p_4()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .items_end()
                        .gap_4()
                        .child(field("Name", &row.name, cx))
                        .child(kinds)
                        .when(can_delete, |d| {
                            d.child(
                                components::icon_button(("template-delete", i), Icon::Delete, cx)
                                    .tooltip(|window, cx| {
                                        components::Tooltip::text("Delete template", window, cx)
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.templates.remove(i);
                                        this.save_templates(cx);
                                        this.load_templates(window, cx);
                                    })),
                            )
                        }),
                )
                .child(
                    div()
                        .flex()
                        .gap_4()
                        .child(field("File (in your notes folder)", &row.file, cx))
                        .child(if journal {
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .justify_end()
                                .child(components::caption(
                                    "Entries go under today's date heading",
                                    cx,
                                ))
                        } else {
                            field("Under heading", &row.heading, cx)
                        }),
                ),
        )
    }
}

/// The card surface: card fill, hairline border, control radius.
fn card_frame(cx: &App) -> Div {
    let c = cx.theme().colors;
    div()
        .bg(c.card)
        .border_1()
        .border_color(c.stroke_surface)
        .rounded(RADIUS_CONTROL)
}

fn group_header(label: &'static str) -> Div {
    div()
        .pt_6()
        .pb_2()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child(label)
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = cx.global::<Settings>().clone();
        let c = cx.theme().colors;

        let theme = Self::choice(
            "theme",
            &[
                ("System", AppearancePref::System),
                ("Light", AppearancePref::Light),
                ("Dark", AppearancePref::Dark),
            ],
            settings.appearance,
            cx,
            |s, v| s.appearance = v,
        );

        let current_accent = settings
            .accent
            .as_deref()
            .and_then(aster_ui::accent::parse_hex);
        let mut swatches = vec![self.swatch(0, None, "System", current_accent.is_none(), cx)];
        for (i, (name, hex)) in aster_ui::accent::SWATCHES.iter().enumerate() {
            let color: Rgba = gpui::rgb(*hex);
            let selected = current_accent
                .is_some_and(|a| aster_ui::accent::to_hex(a) == aster_ui::accent::to_hex(color));
            swatches.push(self.swatch(i + 1, Some(color), name, selected, cx));
        }
        // "Custom": shows the custom color when one is in use, otherwise a "+".
        let custom = settings.accent.as_deref().filter(|a| !is_preset(a));
        let custom_fill: Option<gpui::Hsla> =
            custom.and_then(aster_ui::accent::parse_hex).map(Into::into);
        swatches.push(
            div()
                .id("swatch-custom")
                .size(px(32.))
                .rounded(RADIUS_CONTROL)
                .border_2()
                .border_color(if custom.is_some() {
                    c.text_primary
                } else {
                    c.stroke_control_strong
                })
                .p(px(2.))
                .cursor_pointer()
                .tooltip(|window, cx| components::Tooltip::text("Custom color", window, cx))
                .child(
                    div()
                        .size_full()
                        .rounded(px(2.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .when_some(custom_fill, |d, fill| {
                            d.bg(fill).child(icon(
                                Icon::Edit,
                                px(14.),
                                aster_ui::accent::on_accent(fill),
                            ))
                        })
                        .when(custom_fill.is_none(), |d| {
                            d.child(icon(Icon::Add, px(16.), c.text_secondary))
                        }),
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.show_custom = true;
                    let current = cx.global::<Settings>().accent.clone().unwrap_or_default();
                    this.custom_hex.update(cx, |e, cx| {
                        e.set_text(&current, cx);
                        e.select_range(0..usize::MAX, cx);
                    });
                    window.focus(&this.custom_hex.focus_handle(cx));
                    cx.notify();
                }))
                .into_any_element(),
        );
        let hex_text = self.custom_hex.read(cx).text().trim().to_string();
        let hex_invalid = !hex_text.is_empty() && aster_ui::accent::parse_hex(&hex_text).is_none();
        let custom_row = self.show_custom.then(|| {
            div()
                .pl(px(36.))
                .flex()
                .items_center()
                .gap_3()
                .child(components::caption("Custom color", cx).text_color(c.text_secondary))
                .child(
                    components::text_box(self.custom_hex.focus_handle(cx).is_focused(window), cx)
                        .w(px(120.))
                        .child(self.custom_hex.clone()),
                )
                .child(
                    components::caption(
                        if hex_invalid {
                            "Use a hex color like #0078d4"
                        } else {
                            "Any color works: Aster adjusts it so text stays readable"
                        },
                        cx,
                    )
                    .text_color(if hex_invalid {
                        c.critical
                    } else {
                        c.text_tertiary
                    }),
                )
        });
        let accent = card_frame(cx).child(
            div()
                .px_4()
                .py_3()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_4()
                        .child(icon(Icon::Color, px(20.), c.text_primary))
                        .child(
                            div().flex().flex_col().child("Accent color").child(
                                components::caption(
                                    "Highlights selections, the cursor and buttons. \
                                     “System” follows your desktop's accent.",
                                    cx,
                                )
                                .text_color(c.text_secondary),
                            ),
                        ),
                )
                .child(
                    div()
                        .pl(px(36.))
                        .flex()
                        .flex_wrap()
                        .gap_1()
                        .children(swatches),
                )
                .children(custom_row),
        );

        let supported = settings::transparency_supported();
        let transparency = div()
            .flex()
            .items_center()
            .gap_3()
            .child(if settings.translucent() { "On" } else { "Off" })
            .child(
                components::toggle_switch("transparency", settings.translucent(), supported, cx)
                    .when(supported, |d| {
                        d.on_click(cx.listener(|this, _, _, cx| {
                            this.change(cx, |s| s.transparency = !s.transparency)
                        }))
                    }),
            );

        let size = settings.font_size;
        let font_size = div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                components::button("font-smaller", "A−", ButtonKind::Standard, cx)
                    .w(px(40.))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.change(cx, |s| s.font_size = (s.font_size - 1.).max(FONT_SIZE_MIN))
                    })),
            )
            .child(
                div()
                    .w(px(48.))
                    .flex()
                    .justify_center()
                    .child(format!("{size:.0}")),
            )
            .child(
                components::button("font-larger", "A+", ButtonKind::Standard, cx)
                    .w(px(40.))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.change(cx, |s| s.font_size = (s.font_size + 1.).min(FONT_SIZE_MAX))
                    })),
            )
            // Reset sits left of the stepper, and its space is kept, so A−/A+ never move.
            ;
        let reset = div().size(px(32.)).when(size != FONT_SIZE_DEFAULT, |d| {
            d.child(
                components::icon_button("font-reset", Icon::Reset, cx)
                    .tooltip(|window, cx| components::Tooltip::text("Reset to default", window, cx))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.change(cx, |s| s.font_size = FONT_SIZE_DEFAULT)
                    })),
            )
        });
        let font_size = div()
            .flex()
            .items_center()
            .gap_2()
            .child(reset)
            .child(font_size);

        let keymap = Self::choice(
            "keymap",
            &[
                ("Standard", Keymap::Standard),
                ("Emacs", Keymap::Emacs),
                ("Vim", Keymap::Vim),
            ],
            settings.keymap,
            cx,
            |s, v| s.keymap = v,
        );
        let summary = match settings.keymap {
            Keymap::Standard => None,
            Keymap::Emacs => Some(EMACS_SUMMARY),
            Keymap::Vim => Some(VIM_SUMMARY),
        };
        let emacs_help = summary.map(|summary| {
            card_frame(cx).mt(px(-3.)).px_4().py_3().child(
                div()
                    .pl(px(36.))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .text_size(ramp::CAPTION.size)
                    .line_height(ramp::CAPTION.line_height)
                    .children(summary.iter().map(|(keys, what)| {
                        div()
                            .flex()
                            .gap_4()
                            .child(
                                div()
                                    .w(px(200.))
                                    .flex_shrink_0()
                                    .text_color(c.text_primary)
                                    .child(*keys),
                            )
                            .child(div().text_color(c.text_secondary).child(*what))
                    })),
            )
        });

        let templates: Vec<_> = self
            .templates
            .iter()
            .enumerate()
            .map(|(i, row)| self.template_card(i, row, window, cx))
            .collect();
        let template_buttons = div()
            .pt_2()
            .flex()
            .gap_2()
            .child(
                components::button("template-add", "Add template", ButtonKind::Standard, cx)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.change(cx, |s| {
                            s.capture_templates.push(CaptureTemplate {
                                name: "New template".into(),
                                kind: CaptureKind::Note,
                                file: "inbox.org".into(),
                                heading: String::new(),
                            })
                        });
                        this.load_templates(window, cx);
                        if let Some(row) = this.templates.last() {
                            window.focus(&row.name.focus_handle(cx));
                        }
                    })),
            )
            .child(
                components::button("template-reset", "Restore defaults", ButtonKind::Subtle, cx)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.change(cx, |s| s.capture_templates = settings::default_templates());
                        this.load_templates(window, cx);
                    })),
            );

        div()
            .id("settings")
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::close))
            .size_full()
            .overflow_y_scroll()
            .text_size(ramp::BODY.size)
            .line_height(ramp::BODY.line_height)
            .child(
                div()
                    .max_w(px(860.))
                    .px_8()
                    .pt_6()
                    .pb_12()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                components::icon_button("settings-back", Icon::ChevronLeft, cx)
                                    .tooltip(|window, cx| {
                                        components::Tooltip::text("Back", window, cx)
                                    })
                                    .on_click(
                                        cx.listener(|_, _, _, cx| cx.emit(SettingsEvent::Close)),
                                    ),
                            )
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_size(ramp::TITLE.size)
                                    .line_height(ramp::TITLE.line_height)
                                    .child("Settings"),
                            ),
                    )
                    .child(group_header("Appearance"))
                    .child(Self::card(
                        Icon::Theme,
                        "Theme",
                        "Light, dark, or match your system",
                        theme,
                        cx,
                    ))
                    .child(accent)
                    .child(Self::card(
                        Icon::Transparency,
                        "Transparency effects",
                        if settings::transparency_supported() {
                            "Let the desktop show through the window's background"
                        } else {
                            "Not available on this desktop (it doesn't blur behind windows)"
                        },
                        transparency,
                        cx,
                    ))
                    .child(group_header("Editor"))
                    .child(Self::card(
                        Icon::TextSize,
                        "Text size",
                        "The size of text in your notes",
                        font_size,
                        cx,
                    ))
                    .child(Self::card(
                        Icon::Keyboard,
                        "Keyboard shortcuts",
                        "Emacs or Vim keys, on top of the standard ones",
                        keymap,
                        cx,
                    ))
                    .children(emacs_help)
                    .child(group_header("Capture templates"))
                    .child(
                        components::caption(
                            "What the Capture dialog (the + button) can create, and where it \
                             goes. Files are created when first used.",
                            cx,
                        )
                        .text_color(c.text_secondary)
                        .pb_2(),
                    )
                    .children(templates)
                    .child(template_buttons),
            )
            .into_any_element()
    }
}

impl Focusable for SettingsView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
