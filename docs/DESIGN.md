# Aster design guidelines

Aster's interface follows **Microsoft Fluent Design as it looked in early Windows 11** (WinUI 3,
2021–2022): calm layered surfaces, soft rounded geometry, a single accent color, generous
spacing, and quiet chrome that lets the notes be the focus. We adapt it to macOS and Linux
rather than imitating Windows pixel for pixel: platform fonts, platform window controls, and
materials only where the platform supports them.

All values below live in `aster-ui` as theme tokens. **Never hard-code a color, radius, font size
or shadow in a view** — use the token, so light/dark themes and future accent colors just work.

---

## 1. Principles

1. **Content first.** Notes sit on a clean surface; chrome recedes. Controls are subtle until
   hovered, focused or selected.
2. **Layered, not boxed.** Hierarchy comes from surfaces (base → layer → flyout), not from heavy
   borders. Strokes are hairlines that catch light.
3. **One accent.** Accent color marks the current selection, focus, primary actions and links.
   Everything else is neutral. Semantic colors (success, caution, critical) are for status only.
4. **Soft geometry.** Rounded corners everywhere, sized by role (see §5).
5. **Predictable motion.** Short, eased transitions that explain state changes; never decorative.
6. **Accessible by default.** Text contrast ≥ 4.5:1, visible focus, full keyboard reach.

## 2. Materials and layers

| Layer | Use | Light | Dark |
|---|---|---|---|
| **Base (Mica)** | Window background behind the sidebar and title area | `#F3F3F3` | `#202020` |
| **Layer** | The main content area (editor, agenda), as a card inset into the base with an 8 px top-left radius and a hairline stroke | `#FBFBFB` | `#282828` |
| **Card** | Grouped content inside a layer (settings groups, agenda days) | `#FFFFFF` @ 70 % | `#FFFFFF` @ 5 % |
| **Flyout (Acrylic)** | Menus, pickers, date picker, command palette, capture | `#FCFCFC` | `#2C2C2C` |
| **Smoke** | Dims the app behind modal dialogs | `#000000` @ 30 % | `#000000` @ 30 % |

- **Transparency effects** (on by default, Settings → Transparency effects): the window gets a
  blurred background and the base layer becomes translucent (72 % light, 80 % dark), so the
  desktop shows through the sidebar and title area much like Mica. The content layer stays
  opaque for reading. Supported on macOS and on Wayland compositors that blur behind windows
  (KDE Plasma, Hyprland); elsewhere (e.g. GNOME) the setting is unavailable and the base is
  solid, since an unblurred see-through window hurts legibility.
- Flyouts and dialogs are opaque in Aster (true acrylic blur isn't available in-app).

## 3. Color tokens

Names follow WinUI's theme resources so they're easy to look up.

### Text

| Token | Light | Dark | Use |
|---|---|---|---|
| `text.primary` | `#1A1A1A` | `#FFFFFF` | Body text, titles |
| `text.secondary` | `#5D5D5D` | `#C5C5C5` | Labels, metadata, captions |
| `text.tertiary` | `#8A8A8A` | `#9E9E9E` | Placeholders, hints, disabled-looking chrome |
| `text.disabled` | `#A0A0A0` | `#717171` | Disabled controls |
| `text.on_accent` | `#FFFFFF` | `#000000` | Text on accent fills |

### Accent

Aster uses the **system accent color** (macOS System Settings → Appearance → Accent color;
on Linux the desktop's accent via the XDG appearance portal, KDE's `AccentColor`, or GNOME's
`accent-color`). Like Windows' accent shades, the color is adjusted per theme: darkened on light
themes and lightened on dark themes until it reaches 4.5:1 contrast on every surface, and text on
accent fills is black or white, whichever reads better. With no system preference (macOS
"Multicolor"), the defaults below apply. Settings → Accent color can pick a fixed accent instead
(a Windows 11-style palette of swatches); `ASTER_ACCENT=#rrggbb` stands in for the system accent
when testing.

| Token | Light | Dark | Use |
|---|---|---|---|
| `accent.default` | `#005FB8` | `#60CDFF` | Accent buttons, selection indicator, focus, links |
| `accent.secondary` | `#005FB8` @ 90 % | `#60CDFF` @ 90 % | Accent hover |
| `accent.tertiary` | `#005FB8` @ 80 % | `#60CDFF` @ 80 % | Accent pressed |
| `accent.subtle` | `#005FB8` @ 10 % | `#60CDFF` @ 14 % | Search highlights, selected chips |
| `text.selection` | `#005FB8` @ 25 % | `#60CDFF` @ 30 % | Text selection |

### Fills

| Token | Light | Dark | Use |
|---|---|---|---|
| `control.default` | `#FFFFFF` @ 70 % | `#FFFFFF` @ 6 % | Standard button, text box rest |
| `control.secondary` | `#F9F9F9` @ 50 % | `#FFFFFF` @ 8 % | Control hover |
| `control.tertiary` | `#F9F9F9` @ 30 % | `#FFFFFF` @ 4 % | Control pressed |
| `control.input_active` | `#FFFFFF` | `#1E1E1E` | Focused text box |
| `subtle.secondary` | `#000000` @ 3.7 % | `#FFFFFF` @ 6 % | Hover on list items / subtle buttons |
| `subtle.tertiary` | `#000000` @ 2.4 % | `#FFFFFF` @ 4 % | Pressed, selected list item |

### Strokes

| Token | Light | Dark | Use |
|---|---|---|---|
| `stroke.control` | `#000000` @ 5.8 % | `#FFFFFF` @ 7 % | Control and card borders |
| `stroke.control_strong` | `#000000` @ 16 % | `#FFFFFF` @ 9 % | Bottom edge of controls (elevation) |
| `stroke.divider` | `#000000` @ 8 % | `#FFFFFF` @ 8 % | Dividers between regions |
| `stroke.surface` | `#000000` @ 5.8 % | `#000000` @ 20 % | Flyout and layer outlines |
| `stroke.focus` | `#000000` @ 90 % | `#FFFFFF` | Keyboard focus ring (2 px, outside) |

### Status (semantic)

| Token | Foreground light / dark | Background light / dark |
|---|---|---|
| `status.success` | `#0F7B0F` / `#6CCB5F` | `#DFF6DD` / `#393D1B` |
| `status.caution` | `#9D5D00` / `#FCE100` | `#FFF4CE` / `#433519` |
| `status.critical` | `#C42B1C` / `#FF99A4` | `#FDE7E9` / `#442726` |
| `status.neutral` | `#5D5D5D` / `#C5C5C5` | `#000000` 4 % / `#FFFFFF` 6 % |

### Org-specific colors

Org semantics map onto the palette above so the editor feels part of the same system:

| Element | Token |
|---|---|
| Heading levels 1 / 2 / 3 / 4+ | `text.primary` with weight and size from the type ramp (§4); only the level-1 star color uses `accent.default` |
| TODO-type keyword pill | `status.critical` on its background |
| DONE-type keyword pill | `status.success` on its background; title rendered `text.tertiary` |
| Priority `[#A]`/`[#B]`/`[#C]` | `status.caution` |
| Tags | `text.secondary` on `status.neutral` background |
| Dates | `accent.default` on `accent.subtle` |
| Links | `accent.default`, underline on hover |
| Code / verbatim | `text.primary` on `subtle.secondary`, monospace |
| Code blocks | Card fill with `stroke.control` and 4 px radius |
| Muted lines (drawers, keywords, planning) | `text.tertiary` |

## 4. Typography

Fluent's type ramp, with the platform UI font (SF Pro on macOS, the desktop font on Linux; Segoe
UI Variable is Windows-only). Reading text in the editor is one step larger than UI body text.

| Style | Size / line height | Weight | Use |
|---|---|---|---|
| Caption | 12 / 16 | Regular | Hints, timestamps, shortcut labels, section headers in the sidebar |
| Body | 14 / 20 | Regular | UI text: lists, menus, buttons, inputs |
| Body Strong | 14 / 20 | Semibold | Emphasized UI text, selected tab |
| Body Large | 18 / 24 | Regular | — |
| Subtitle | 20 / 28 | Semibold | Pane titles ("Agenda"), dialog titles |
| Title | 28 / 36 | Semibold | Document title (`#+TITLE`) |
| Title Large | 40 / 52 | Semibold | Empty states, welcome |
| **Editor body** | 16 / 26 | Regular | Note text |
| Editor heading 1 / 2 / 3 / 4+ | 24 / 20 / 17 / 16 | Semibold | Org headings |

- Sentence case everywhere ("Search all files", not "Search All Files"), except menu bar titles
  where the platform convention is title case.
- Monospace for code, tables and fixed-width text: SF Mono / Menlo / Cascadia Code (whichever is
  installed).

## 5. Geometry

| Token | Value | Use |
|---|---|---|
| `radius.control` | 4 px | Buttons, text boxes, list items, chips, pills |
| `radius.overlay` | 8 px | Flyouts, menus, dialogs, cards, the content layer, toasts |
| `radius.none` | 0 | Edges that meet other surfaces |

Spacing is on a **4 px grid**: 4, 8, 12, 16, 24, 32, 48. Defaults:

- Control height 32 px (compact list rows 28–32 px); icon-only buttons 32 × 32.
- Padding inside controls: 12 px horizontal, 6 px vertical. Flyout padding 4 px; menu items 4 px
  apart from the flyout edge.
- Page gutters: 24 px in panes; the editor text column is centered with a comfortable max width
  (about 760 px) and 48 px side margins on wide windows.
- Sidebar ("navigation pane") width 260 px.

## 6. Elevation

| Level | Shadow (light) | Shadow (dark) | Use |
|---|---|---|---|
| 0 | none | none | Base, layer |
| 2 | 0 2 4 @ 4 % | 0 2 4 @ 26 % | Cards, buttons (optional) |
| 8 | 0 4 8 @ 14 % | 0 4 8 @ 26 % | Tooltips, toasts |
| 16 | 0 8 16 @ 14 % | 0 8 16 @ 26 % | Menus, flyouts, pickers |
| 32 | 0 16 32 @ 18 % | 0 16 32 @ 37 % | Dialogs, command palette |

Elevated surfaces also get a 1 px `stroke.surface` outline.

## 7. Controls

**Buttons** — 32 px high, `radius.control`.
- *Accent*: `accent.default` fill, `text.on_accent`; hover `accent.secondary`, pressed `accent.tertiary`.
  One per surface, for the primary action.
- *Standard*: `control.default` fill, `stroke.control` border with `stroke.control_strong` bottom edge.
- *Subtle*: transparent; hover `subtle.secondary`, pressed `subtle.tertiary`. Toolbars, icon buttons.

**Text box** — `control.default` fill, `stroke.control` border; bottom edge `stroke.control_strong`.
Focused: `control.input_active` fill and a **2 px `accent.default` bottom line**. Placeholder in
`text.tertiary`.

**List items / navigation items** — 32 px rows, `radius.control`, inset 4 px from the pane edge.
Hover `subtle.secondary`; selected `subtle.secondary` plus the **selection indicator: a 3 × 16 px
accent pill** centered vertically at the item's left edge. Used by the sidebar, pickers and the
agenda.

**Selector bar (tabs)** — text labels, `text.secondary` at rest; selected label `text.primary`
Body Strong with a **3 × 16 px accent pill centered under it**.

**Menus and flyouts** — Flyout surface, `radius.overlay`, elevation 16, 4 px padding; items as list
items (28 px); optional right-aligned shortcut in Caption `text.tertiary`; check marks in
`accent.default`; separators `stroke.divider` with 4 px vertical margin.

**Dialogs** — Flyout surface on a smoke overlay, `radius.overlay`, elevation 32, 24 px padding,
Subtitle title, buttons right-aligned (accent first).

**InfoBar / toasts** — `radius.overlay`, status background with a matching icon glyph, Body text,
`stroke.surface` outline; informational toasts use the flyout surface.

**Pills and chips** (TODO keywords, tags, dates) — `radius.control`, 4 px horizontal padding,
inline with text.

**Settings cards** — the Windows 11 Settings pattern. A `card` fill with a `stroke.surface`
hairline, `radius.control`, at least 68 px high, 16 px horizontal padding; a 20 px icon, then the
title (Body) over a description (Caption, `text.secondary`), with the control right-aligned. Cards
in a group sit 3 px apart under a Body Strong group header. Changes apply and save immediately:
there is no OK/Cancel. Controls never shift under the pointer as a value changes (reserve space
for buttons that come and go, like "reset").

**Toggle switch** — 40 × 20 px pill. Off: `control.secondary` track with a `text.secondary`
outline and knob. On: `accent.default` track with a `text.on_accent` knob. Disabled: `text.disabled`.

**Scrollbars** — thin (2 px) at rest, widening on hover, `text.tertiary`; no track.

## 8. Iconography

[Fluent UI System Icons](https://github.com/microsoft/fluentui-system-icons) (MIT), *Regular*
style, 16 px in lists and menus, 20 px in toolbars, colored `text.secondary` (accent when
selected). Icons are bundled as SVG; never use emoji as UI icons.

## 9. Motion

| Use | Duration | Easing |
|---|---|---|
| Hover/press color changes | 83 ms | linear |
| Flyouts open / close | 167 ms / 83 ms | decelerate `cubic-bezier(0, 0, 0, 1)` / accelerate |
| Panes, page transitions | 250 ms | decelerate |

Motion is optional polish: the UI must be complete and clear without it.

## 10. Focus and accessibility

- Keyboard focus shows a 2 px `stroke.focus` ring outside the control (1 px inner gap), on
  `radius.control` + 2.
- Every action reachable by mouse is reachable by keyboard and listed in the command palette.
- Check text and pill contrast in both themes (≥ 4.5:1 for text, ≥ 3:1 for UI glyphs).

## 11. Window and layout

- macOS: transparent title bar with the sidebar running under the traffic lights (as in
  Windows 11's extended title bar); the content layer starts below the title area.
- Linux: native window decorations; same layout below them.
- Layout: navigation pane (sidebar) on the base layer at left; the content layer as a rounded
  card filling the rest; flyouts and dialogs float above.
- Themes follow the system light/dark setting by default, with a manual override in settings.
