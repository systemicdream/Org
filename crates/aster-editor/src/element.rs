//! Layout and painting for [`OrgEditor`].
//!
//! Every visible (unfolded) row gets a vertical position. Rows near the viewport are shaped
//! with soft wrapping; rows far away use a cached visual-line count (or one line) so the
//! document height stays stable without shaping everything.

use aster_doc::display::RowDisplay;
use aster_doc::folds::foldable_range;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use gpui::{
    App, Bounds, Corners, ElementId, ElementInputHandler, Entity, GlobalElementId,
    ImgResourceLoader, LayoutId, PaintQuad, Pixels, Point, RenderImage, Resource, ShapedLine,
    Style, TextAlign, TextRun, Window, fill, point, prelude::*, px, relative, size,
};

use aster_ui::icons::Icon;

use crate::spans::Decor;
use crate::{EditorMode, Layout, Metrics, OrgEditor, RowLayout};

/// Width of the fold-arrow gutter left of the text.
fn gutter() -> Pixels {
    px(22.)
}

/// Space above and below an inline image.
const IMAGE_PADDING: Pixels = px(6.);

/// Extra rows shaped above and below the viewport, so scrolling a little needs no reshaping.
const OVERSCAN_LINES: f32 = 8.;

/// Widest text column, in multiples of the text size (about 80 characters of body text): long
/// lines are hard to read, so wide windows get margins instead.
const MAX_COLUMN_EMS: f32 = 46.;

pub struct EditorElement {
    editor: Entity<OrgEditor>,
}

impl EditorElement {
    pub fn new(editor: Entity<OrgEditor>) -> Self {
        Self { editor }
    }
}

pub struct PrepaintState {
    rows: Vec<RowLayout>,
    text_left: Pixels,
    /// Visible vertical range, relative to the element top.
    paint_range: (Pixels, Pixels),
    backgrounds: Vec<PaintQuad>,
    overlays: Vec<PaintQuad>,
    images: Vec<(Bounds<Pixels>, Arc<RenderImage>)>,
    selections: Vec<PaintQuad>,
    glyphs: Vec<(ShapedLine, Point<Pixels>, Pixels)>,
    icons: Vec<(Bounds<Pixels>, Icon, gpui::Hsla)>,
    tag_hits: Vec<(Bounds<Pixels>, usize)>,
    cursor: Option<PaintQuad>,
    wrap_updates: Vec<(u64, usize)>,
    /// The layout matches the measured rows (nothing moved this frame).
    settled: bool,
}

impl IntoElement for EditorElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let editor = self.editor.read(cx);
        let line_height = window.line_height();
        let height = match editor.mode {
            EditorMode::SingleLine => line_height,
            EditorMode::Full => {
                let wrap_width = editor.metrics.as_ref().and_then(|m| m.wrap_width);
                editor
                    .visible_rows
                    .iter()
                    .enumerate()
                    .map(|(i, &row)| {
                        let scale = editor.row_scale(row);
                        let key =
                            editor.wrap_key(row, scale, editor.row_wrap_width(row, wrap_width));
                        let lines = editor.wrap_cache.get(&key).copied().unwrap_or(1);
                        let image = editor
                            .row_wrap_width(row, wrap_width)
                            .and_then(|w| editor.image_display_size(row, w))
                            .map_or(px(0.), |s| s.height + IMAGE_PADDING * 2.);
                        editor.space_above(i, line_height)
                            + line_height * scale * lines as f32
                            + image
                    })
                    .fold(px(0.), |a, b| a + b)
            }
        };
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = height.into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> PrepaintState {
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();
        let full = self.editor.read(cx).mode == EditorMode::Full;
        let gutter = if full { gutter() } else { px(0.) };
        let available = (bounds.size.width - gutter).max(px(80.));
        let column = if full {
            available.min(font_size * MAX_COLUMN_EMS)
        } else {
            available
        };
        // Centered in the editor when the window is wider than the column.
        let text_left = bounds.left() + gutter + ((available - column) / 2.).max(px(0.));
        let text_right = text_left + column;
        let mono_family = self.editor.read(cx).mono_family.clone().unwrap_or_else(|| {
            let family = pick_monospace(window);
            self.editor
                .update(cx, |e, _| e.mono_family = Some(family.clone()));
            family
        });
        let metrics = Metrics {
            font: style.font(),
            mono_family,
            font_size,
            color: style.color,
            wrap_width: full.then_some(column),
        };
        self.editor
            .update(cx, |e, _| e.metrics = Some(metrics.clone()));

        // Load inline images (asynchronously; the view is notified when one arrives).
        let image_paths: Vec<PathBuf> = self.editor.read(cx).row_images.values().cloned().collect();
        let mut images: HashMap<PathBuf, Arc<RenderImage>> = HashMap::new();
        for path in image_paths {
            let resource = Resource::Path(path.clone().into());
            if let Some(Ok(img)) = window.use_asset::<ImgResourceLoader>(&resource, cx) {
                images.insert(path, img);
            }
        }
        let scale_factor = window.scale_factor();
        self.editor.update(cx, |e, _| {
            for (path, img) in &images {
                let s = img.size(0);
                e.image_sizes.insert(
                    path.clone(),
                    gpui::size(
                        px(s.width.0 as f32 / scale_factor),
                        px(s.height.0 as f32 / scale_factor),
                    ),
                );
            }
        });
        let editor = self.editor.read(cx);
        let mut image_paints = Vec::new();

        // Which part of the element is on screen.
        let viewport = if full {
            let v = editor.scroll_handle.bounds();
            if v.size.height > px(0.) { v } else { bounds }
        } else {
            bounds
        };
        let paint_top = viewport.top() - bounds.top();
        let paint_bottom = viewport.bottom() - bounds.top();
        let overscan = line_height * OVERSCAN_LINES;

        let cursor = editor.cursor_offset();
        let cursor_row = editor.row_for_offset(cursor);
        let block_cursor = editor.vim_mode(cx) == Some(crate::vim::VimMode::Normal);

        let mut rows = Vec::with_capacity(editor.visible_rows.len());
        let mut wrap_updates = Vec::new();
        let mut y = px(0.);
        for (i, &row) in editor.visible_rows.iter().enumerate() {
            y += editor.space_above(i, line_height);
            let scale = editor.row_scale(row);
            let lh = line_height * scale;
            let indent = editor.row_indent(row);
            let key = editor.wrap_key(row, scale, editor.row_wrap_width(row, metrics.wrap_width));
            let estimate = editor.wrap_cache.get(&key).copied().unwrap_or(1);
            let near = (y + lh * estimate as f32 >= paint_top - overscan
                && y <= paint_bottom + overscan)
                || row == cursor_row;
            let (wrapped, display, visual_lines, decor, tags) = if near {
                let shaped = editor.shape_row(row, &metrics, window);
                let n = shaped.wrapped.wrap_boundaries().len() + 1;
                if n != estimate || !editor.wrap_cache.contains_key(&key) {
                    wrap_updates.push((key, n));
                }
                (
                    Some(shaped.wrapped),
                    shaped.display,
                    n,
                    shaped.decor,
                    shaped.tags,
                )
            } else {
                let display = RowDisplay::plain(editor.doc.text(), editor.line_range(row));
                (None, display, estimate, Vec::new(), Vec::new())
            };
            let extra = editor
                .row_wrap_width(row, metrics.wrap_width)
                .and_then(|w| editor.image_display_size(row, w))
                .map_or(px(0.), |size| {
                    let top = bounds.top() + y + lh * visual_lines as f32 + IMAGE_PADDING;
                    if let Some(img) = editor.row_images.get(&row).and_then(|p| images.get(p)) {
                        image_paints.push((
                            Bounds::new(point(text_left + indent, top), size),
                            img.clone(),
                        ));
                    }
                    size.height + IMAGE_PADDING * 2.
                });
            rows.push(RowLayout {
                row,
                top: y,
                indent,
                display,
                extra,
                line_height: lh,
                wrapped,
                visual_lines,
                decor,
                tags,
            });
            y += lh * visual_lines as f32 + extra;
        }
        let total_height = y;

        let wrap_width = metrics.wrap_width;
        let run = |color| TextRun {
            len: 0,
            font: metrics.font.clone(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let shape_glyph = |window: &mut Window, s: &'static str, color, font_size| {
            let mut r = run(color);
            r.len = s.len();
            window
                .text_system()
                .shape_line(s.into(), font_size, &[r], None)
        };

        let colors = editor.colors;
        let selection_color = colors.selection;
        let sel = editor.selected_range.clone();
        let mut selections = Vec::new();
        let mut backgrounds = Vec::new();
        let mut overlays = Vec::new();
        let mut glyphs = Vec::new();
        let mut icons = Vec::new();
        let mut tag_hits = Vec::new();
        let text = editor.doc.text();
        // Column positions of the last table row drawn, for the rule rows below it.
        let mut table_columns: Vec<Pixels> = Vec::new();

        for (j, r) in rows.iter().enumerate() {
            let Some(w) = r.wrapped.as_ref() else {
                continue;
            };
            if r.top + r.height() < paint_top || r.top > paint_bottom {
                continue;
            }
            let lh = r.line_height;
            let origin = point(text_left + r.indent, bounds.top() + r.top);
            let range = editor.line_range(r.row);
            let line_width = editor
                .row_wrap_width(r.row, wrap_width)
                .unwrap_or(w.width());

            // Tinted background behind code blocks.
            if full && editor.row_kind(r.row).block_background() {
                backgrounds.push(fill(
                    Bounds::from_corners(
                        point(origin.x - px(8.), origin.y),
                        point(text_right + px(8.), origin.y + r.text_height()),
                    ),
                    colors.subtle_secondary,
                ));
            }

            // Tables not being edited: a grid in place of the `|` and `-+-` characters.
            let kind = editor.row_kind(r.row);
            if full && kind.is_table() && !editor.table_revealed(r.row) {
                let (top, bottom) = (origin.y, origin.y + r.text_height());
                if kind != crate::preview::RowKind::TableRule {
                    table_columns = r
                        .display
                        .text
                        .match_indices('|')
                        .filter_map(|(i, _)| {
                            let a = w.position_for_index(i, lh)?;
                            let b = w.position_for_index(i + 1, lh)?;
                            (a.y == b.y).then(|| origin.x + (a.x + b.x) / 2.)
                        })
                        .collect();
                }
                let joined = |k: Option<&crate::RowLayout>| {
                    k.is_some_and(|k| {
                        k.row.abs_diff(r.row) == 1 && editor.row_kind(k.row).is_table()
                    })
                };
                if let (Some(&x0), Some(&x1)) = (table_columns.first(), table_columns.last()) {
                    let rule = colors.stroke_control_strong;
                    let hline = |y: Pixels| {
                        fill(
                            Bounds::from_corners(point(x0, y), point(x1 + px(1.), y + px(1.))),
                            rule,
                        )
                    };
                    if kind == (crate::preview::RowKind::Table { header: true }) {
                        backgrounds.push(fill(
                            Bounds::from_corners(point(x0, top), point(x1, bottom)),
                            colors.subtle_secondary,
                        ));
                    }
                    if kind == crate::preview::RowKind::TableRule {
                        backgrounds.push(hline(((top + bottom) / 2.).floor()));
                    }
                    if !joined(j.checked_sub(1).and_then(|p| rows.get(p))) {
                        backgrounds.push(hline(top));
                    }
                    if !joined(rows.get(j + 1)) {
                        backgrounds.push(hline(bottom - px(1.)));
                    }
                    for &x in &table_columns {
                        backgrounds.push(fill(
                            Bounds::from_corners(point(x, top), point(x + px(1.), bottom)),
                            rule,
                        ));
                    }
                }
            }

            // Labels, checkboxes and icons drawn around the text.
            let text_size = font_size * (lh / line_height);
            for (range, style) in &r.decor {
                let Some(p0) = w.position_for_index(range.start, lh) else {
                    continue;
                };
                let x1 = w
                    .position_for_index(range.end, lh)
                    .filter(|p| p.y == p0.y)
                    .map_or(line_width, |p| p.x);
                let center_y = origin.y + p0.y + lh / 2.;
                match style.decor {
                    Some(Decor::Pill(color)) => {
                        let h = (text_size * 1.3).min(lh);
                        // Narrow, so neighbouring labels (TODO [#A]) keep a gap.
                        let pad = text_size * 0.15;
                        backgrounds.push(
                            fill(
                                Bounds::from_corners(
                                    point(origin.x + p0.x - pad, center_y - h / 2.),
                                    point(origin.x + x1 + pad, center_y + h / 2.),
                                ),
                                color,
                            )
                            .corner_radii(px(4.)),
                        );
                    }
                    Some(Decor::Icon(icon)) => {
                        let size = text_size * 0.95;
                        let x = origin.x + p0.x + (x1 - p0.x - size) / 2.;
                        icons.push((
                            Bounds::new(point(x, center_y - size / 2.), gpui::size(size, size)),
                            icon,
                            style.color.unwrap_or(colors.text_secondary),
                        ));
                    }
                    Some(Decor::Checkbox { checked, partial }) => {
                        let size = (text_size * 0.9).round();
                        let x = (origin.x + (p0.x + x1 - size) / 2.).round();
                        let b = Bounds::new(
                            point(x, (center_y - size / 2.).round()),
                            gpui::size(size, size),
                        );
                        if checked || partial {
                            backgrounds.push(fill(b, colors.accent).corner_radii(px(3.)));
                            if checked {
                                let inner = size * 0.75;
                                icons.push((
                                    Bounds::new(
                                        b.origin + point((size - inner) / 2., (size - inner) / 2.),
                                        gpui::size(inner, inner),
                                    ),
                                    Icon::Checkmark,
                                    colors.text_on_accent,
                                ));
                            } else {
                                overlays.push(fill(
                                    Bounds::from_corners(
                                        b.origin + point(size * 0.25, size * 0.5 - px(1.)),
                                        b.origin + point(size * 0.75, size * 0.5 + px(1.)),
                                    ),
                                    colors.text_on_accent,
                                ));
                            }
                        } else {
                            backgrounds.push(
                                fill(b, gpui::transparent_black())
                                    .corner_radii(px(3.))
                                    .border_widths(px(1.))
                                    .border_color(colors.text_secondary),
                            );
                        }
                    }
                    None => {}
                }
            }
            if full && editor.row_kind(r.row).left_bar() {
                backgrounds.push(fill(
                    Bounds::from_corners(
                        point(origin.x - px(10.), origin.y),
                        point(origin.x - px(7.), origin.y + r.height()),
                    ),
                    colors.stroke_control_strong,
                ));
            }

            // Selection rectangles, one per visual line touched.
            let row_end = editor
                .line_starts
                .get(r.row + 1)
                .copied()
                .unwrap_or(text.len());
            if !sel.is_empty() && sel.start < row_end.max(range.end) && sel.end > range.start {
                let s = r.display.to_display(sel.start.max(range.start));
                let e = r
                    .display
                    .to_display(sel.end.min(range.end).max(range.start));
                let newline_selected = sel.end > range.end && range.end < text.len();
                let p0 = w.position_for_index(s, lh).unwrap_or_default();
                let p1 = w.position_for_index(e, lh).unwrap_or(p0);
                let extra = if newline_selected { px(6.) } else { px(0.) };
                let mut push = |x0: Pixels, x1: Pixels, vy: Pixels| {
                    selections.push(fill(
                        Bounds::from_corners(
                            origin + point(x0, vy),
                            origin + point(x1.max(x0 + px(1.)), vy + lh),
                        ),
                        selection_color,
                    ));
                };
                if p0.y == p1.y {
                    push(p0.x, p1.x + extra, p0.y);
                } else {
                    push(p0.x, line_width, p0.y);
                    let mut vy = p0.y + lh;
                    while vy < p1.y {
                        push(px(0.), line_width, vy);
                        vy += lh;
                    }
                    push(px(0.), p1.x + extra, p1.y);
                }
            }

            // Fold arrows and the "…" after folded headings; tags go after both.
            let mut tail_x = px(0.);
            if full && let Some(_level) = editor.heading_level(r.row) {
                let start = editor.line_starts[r.row];
                if let Some(h) = editor
                    .doc
                    .headings()
                    .iter()
                    .find(|h| h.range.start == start)
                    && !foldable_range(h, text).is_empty()
                {
                    let folded = editor.folds.is_folded(start);
                    let (glyph, color) = if folded {
                        (Icon::ChevronRight, colors.text_secondary)
                    } else {
                        (Icon::ChevronDown, colors.text_disabled)
                    };
                    // A 12 px chevron centered on the heading's first visual line.
                    let size = px(12.);
                    let gy = bounds.top() + r.top + (lh - size) / 2.;
                    let gx = (origin.x - px(18.)).max(bounds.left() + px(4.));
                    icons.push((
                        Bounds::new(point(gx, gy), gpui::size(size, size)),
                        glyph,
                        color,
                    ));
                    if folded {
                        let shaped = shape_glyph(
                            window,
                            " …",
                            colors.text_tertiary,
                            font_size * (lh / line_height),
                        );
                        let end = w
                            .position_for_index(r.display.text.len(), lh)
                            .unwrap_or_default();
                        tail_x = tail_x.max(end.x + px(2.) + shaped.width);
                        glyphs.push((shaped, origin + point(end.x + px(2.), end.y), lh));
                    }
                }
            }

            // A heading's tags, as small labels after its text.
            if !r.tags.is_empty() {
                let end = w
                    .position_for_index(r.display.text.len(), lh)
                    .unwrap_or_default();
                let size = font_size * 0.8;
                let h = size * 1.6;
                let pad = size * 0.5;
                let y = origin.y + end.y + (lh - h) / 2.;
                let mut x = origin.x + tail_x.max(end.x) + px(10.);
                let first = x;
                for tag in &r.tags {
                    let mut tag_run = run(colors.text_secondary);
                    tag_run.len = tag.len();
                    let shaped =
                        window
                            .text_system()
                            .shape_line(tag.clone().into(), size, &[tag_run], None);
                    let width = shaped.width + pad * 2.;
                    backgrounds.push(
                        fill(
                            Bounds::new(point(x, y), gpui::size(width, h)),
                            colors.neutral_bg,
                        )
                        .corner_radii(px(4.)),
                    );
                    glyphs.push((shaped, point(x + pad, y), h));
                    x += width + px(4.);
                }
                tag_hits.push((
                    Bounds::from_corners(point(first, y), point(x, y + h)),
                    editor.line_starts[r.row],
                ));
            }
        }

        // Drop indicator while dragging a heading.
        if let Some((_, y, x)) = editor
            .heading_drag
            .as_ref()
            .filter(|d| d.active)
            .and_then(|d| d.target)
        {
            overlays.push(fill(
                Bounds::from_corners(point(x, y - px(1.)), point(text_right, y + px(1.))),
                colors.accent,
            ));
            overlays.push(fill(
                Bounds::new(point(x - px(3.), y - px(3.)), size(px(6.), px(6.))),
                colors.accent,
            ));
        }

        // Placeholder for empty single-line inputs.
        if !full
            && text.is_empty()
            && let Some(placeholder) = editor.placeholder.clone()
        {
            let mut r = run(colors.text_tertiary);
            r.len = placeholder.len();
            let shaped = window
                .text_system()
                .shape_line(placeholder, font_size, &[r], None);
            glyphs.push((shaped, point(text_left, bounds.top()), line_height));
        }

        // Cursor.
        let cursor_rect = rows
            .binary_search_by_key(&cursor_row, |r| r.row)
            .ok()
            .and_then(|i| {
                let r = &rows[i];
                let col = r.display.to_display(cursor);
                let wrapped = r.wrapped.as_ref()?;
                let p = wrapped.position_for_index(col, r.line_height)?;
                // Vim Normal mode: a block over the character under the cursor.
                let width = if block_cursor {
                    let next = r
                        .display
                        .to_display(aster_doc::text::next_grapheme(editor.doc.text(), cursor));
                    wrapped
                        .position_for_index(next, r.line_height)
                        .filter(|q| q.y == p.y && q.x > p.x)
                        .map_or(font_size * 0.6, |q| q.x - p.x)
                } else {
                    px(2.)
                };
                Some(Bounds::new(
                    point(text_left + r.indent + p.x, bounds.top() + r.top + p.y),
                    size(width, r.line_height),
                ))
            });
        let cursor_quad = cursor_rect
            .filter(|_| sel.is_empty() && (editor.cursor_visible || block_cursor))
            .map(|b| {
                let mut color = colors.accent;
                if block_cursor {
                    color.a = 0.45;
                }
                fill(b, color)
            });

        let mut redraw = false;
        // Keep the cursor visible after keyboard navigation or edits.
        if full
            && editor.autoscroll
            && let Some(c) = cursor_rect
        {
            let mut offset = editor.scroll_handle.offset();
            let margin = line_height * 2.;
            if c.top() - margin < viewport.top() {
                offset.y += viewport.top() - (c.top() - margin);
            } else if c.bottom() + margin > viewport.bottom() {
                offset.y -= c.bottom() + margin - viewport.bottom();
            }
            offset.y = offset.y.min(px(0.));
            if offset != editor.scroll_handle.offset() {
                editor.scroll_handle.set_offset(offset);
                redraw = true;
            }
        }

        // Newly measured rows changed the document height: lay out again next frame.
        // A scroll brings new rows into view, whose measured heights can move the cursor again.
        let height_changed = full && (total_height - bounds.size.height).abs() > px(0.5);
        let settled = !redraw && !height_changed;
        redraw |= !settled;
        // `refresh` is ignored while drawing, so ask for the next frame instead (otherwise the
        // change would only show on the next event, e.g. the cursor blinking).
        if redraw {
            window.on_next_frame(|window, _| window.refresh());
        }

        PrepaintState {
            rows,
            text_left,
            paint_range: (paint_top, paint_bottom),
            backgrounds,
            overlays,
            images: image_paints,
            selections,
            glyphs,
            icons,
            tag_hits,
            cursor: cursor_quad,
            wrap_updates,
            settled,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepaint: &mut PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.editor.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );
        for quad in prepaint.backgrounds.drain(..) {
            window.paint_quad(quad);
        }
        for quad in prepaint.selections.drain(..) {
            window.paint_quad(quad);
        }
        let (top, bottom) = prepaint.paint_range;
        for r in &prepaint.rows {
            let Some(w) = r.wrapped.as_ref() else {
                continue;
            };
            if r.top + r.height() < top || r.top > bottom {
                continue;
            }
            let origin = point(prepaint.text_left + r.indent, bounds.top() + r.top);
            w.paint_background(origin, r.line_height, TextAlign::Left, None, window, cx)
                .ok();
            w.paint(origin, r.line_height, TextAlign::Left, None, window, cx)
                .ok();
        }
        for (b, img) in prepaint.images.drain(..) {
            window
                .paint_image(b, Corners::all(px(6.)), img, 0, false)
                .ok();
        }
        for (b, glyph, color) in prepaint.icons.drain(..) {
            window
                .paint_svg(
                    b,
                    glyph.path().into(),
                    gpui::TransformationMatrix::unit(),
                    color,
                    cx,
                )
                .ok();
        }
        for (shaped, origin, lh) in &prepaint.glyphs {
            shaped.paint(*origin, *lh, window, cx).ok();
        }
        for quad in prepaint.overlays.drain(..) {
            window.paint_quad(quad);
        }
        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        let rows = std::mem::take(&mut prepaint.rows);
        let updates = std::mem::take(&mut prepaint.wrap_updates);
        let text_left = prepaint.text_left;
        let settled = prepaint.settled;
        let tag_hits = std::mem::take(&mut prepaint.tag_hits);
        self.editor.update(cx, |editor, _| {
            if editor.wrap_cache.len() > 100_000 {
                editor.wrap_cache.clear();
            }
            editor.wrap_cache.extend(updates);
            // Until the layout settles, row positions (from estimated heights) can still move;
            // keep scrolling the cursor into view until they stop.
            if settled {
                editor.autoscroll = false;
            }
            editor.layout = Some(Layout {
                bounds,
                text_left,
                rows,
                tag_hits,
            });
        });
    }
}

/// First installed monospace font from a list of common ones.
fn pick_monospace(window: &mut Window) -> gpui::SharedString {
    const CANDIDATES: &[&str] = &[
        "SF Mono",
        "Menlo",
        "JetBrains Mono",
        "Cascadia Code",
        "DejaVu Sans Mono",
        "Noto Sans Mono",
        "Liberation Mono",
        "Ubuntu Mono",
        "Courier New",
    ];
    let installed = window.text_system().all_font_names();
    CANDIDATES
        .iter()
        .find(|c| installed.iter().any(|f| f == *c))
        .map_or_else(|| "monospace".into(), |c| (*c).into())
}
