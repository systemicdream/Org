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

use crate::{EditorMode, Layout, Metrics, OrgEditor, RowLayout};

/// Width of the fold-arrow gutter left of the text.
fn gutter() -> Pixels {
    px(22.)
}

/// Space above and below an inline image.
const IMAGE_PADDING: Pixels = px(6.);

/// Extra rows shaped above and below the viewport, so scrolling a little needs no reshaping.
const OVERSCAN_LINES: f32 = 8.;

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
    cursor: Option<PaintQuad>,
    wrap_updates: Vec<(u64, usize)>,
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
                    .map(|&row| {
                        let scale = editor.row_scale(row);
                        let key =
                            editor.wrap_key(row, scale, editor.row_wrap_width(row, wrap_width));
                        let lines = editor.wrap_cache.get(&key).copied().unwrap_or(1);
                        let image = editor
                            .row_wrap_width(row, wrap_width)
                            .and_then(|w| editor.image_display_size(row, w))
                            .map_or(px(0.), |s| s.height + IMAGE_PADDING * 2.);
                        line_height * scale * lines as f32 + image
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
        let text_left = bounds.left() + gutter;
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
            wrap_width: full.then(|| (bounds.size.width - gutter).max(px(80.))),
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

        let mut rows = Vec::with_capacity(editor.visible_rows.len());
        let mut wrap_updates = Vec::new();
        let mut y = px(0.);
        for &row in &editor.visible_rows {
            let scale = editor.row_scale(row);
            let lh = line_height * scale;
            let indent = editor.row_indent(row);
            let key = editor.wrap_key(row, scale, editor.row_wrap_width(row, metrics.wrap_width));
            let estimate = editor.wrap_cache.get(&key).copied().unwrap_or(1);
            let near = (y + lh * estimate as f32 >= paint_top - overscan
                && y <= paint_bottom + overscan)
                || row == cursor_row;
            let (wrapped, display, visual_lines) = if near {
                let (w, display) = editor.shape_row(row, &metrics, window);
                let n = w.wrap_boundaries().len() + 1;
                if n != estimate || !editor.wrap_cache.contains_key(&key) {
                    wrap_updates.push((key, n));
                }
                (Some(w), display, n)
            } else {
                let display = RowDisplay::plain(editor.doc.text(), editor.line_range(row));
                (None, display, estimate)
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
        let text = editor.doc.text();

        for r in rows.iter().filter(|r| r.wrapped.is_some()) {
            if r.top + r.height() < paint_top || r.top > paint_bottom {
                continue;
            }
            let w = r.wrapped.as_ref().expect("filtered");
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
                        point(bounds.right(), origin.y + r.text_height()),
                    ),
                    colors.subtle_secondary,
                ));
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

            // Fold arrows and the "…" after folded headings.
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
                        let end = w
                            .position_for_index(r.display.text.len(), lh)
                            .unwrap_or_default();
                        let shaped = shape_glyph(
                            window,
                            " …",
                            colors.text_tertiary,
                            font_size * (lh / line_height),
                        );
                        glyphs.push((shaped, origin + point(end.x + px(2.), end.y), lh));
                    }
                }
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
                Bounds::from_corners(point(x, y - px(1.)), point(bounds.right(), y + px(1.))),
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
                let p = r.wrapped.as_ref()?.position_for_index(col, r.line_height)?;
                Some(Bounds::new(
                    point(text_left + r.indent + p.x, bounds.top() + r.top + p.y),
                    size(px(2.), r.line_height),
                ))
            });
        let cursor_quad = cursor_rect
            .filter(|_| sel.is_empty() && editor.cursor_visible)
            .map(|b| fill(b, colors.accent));

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
                window.refresh();
            }
        }

        // Newly measured rows changed the document height: lay out again next frame.
        if full && (total_height - bounds.size.height).abs() > px(0.5) {
            window.refresh();
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
            cursor: cursor_quad,
            wrap_updates,
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
        self.editor.update(cx, |editor, _| {
            if editor.wrap_cache.len() > 100_000 {
                editor.wrap_cache.clear();
            }
            editor.wrap_cache.extend(updates);
            editor.autoscroll = false;
            editor.layout = Some(Layout {
                bounds,
                text_left,
                rows,
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
