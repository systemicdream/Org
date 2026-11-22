//! Org table editing: align columns and move between cells (like Org's Tab / Shift+Tab).

use std::ops::Range;

use unicode_width::UnicodeWidthStr;

use crate::ops::OpResult;
use crate::{Document, Selection, Transaction};

#[derive(Debug, Clone)]
enum Row {
    Rule,
    Cells(Vec<String>),
}

struct Table {
    /// Byte range of the table lines (without the final line terminator).
    range: Range<usize>,
    indent: String,
    eol: &'static str,
    rows: Vec<Row>,
}

/// Byte range of the line containing `offset` (without terminator).
fn line_at(text: &str, offset: usize) -> Range<usize> {
    let start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    let end = text[offset..].find('\n').map_or(text.len(), |i| offset + i);
    let end = if text[start..end].ends_with('\r') {
        end - 1
    } else {
        end
    };
    start..end
}

fn is_table_line(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

fn parse_row(line: &str) -> Row {
    let body = line.trim();
    if body.starts_with("|-") {
        return Row::Rule;
    }
    let body = body.strip_prefix('|').unwrap_or(body);
    let body = body.strip_suffix('|').unwrap_or(body);
    Row::Cells(body.split('|').map(|c| c.trim().to_string()).collect())
}

fn find_table(text: &str, offset: usize) -> Option<Table> {
    let line = line_at(text, offset);
    if !is_table_line(&text[line.clone()]) {
        return None;
    }
    let mut start = line.start;
    while start > 0 {
        let prev = line_at(text, start - 1);
        if !is_table_line(&text[prev.clone()]) {
            break;
        }
        start = prev.start;
    }
    let mut end = line.end;
    loop {
        let next = text[end..].find('\n').map(|i| end + i + 1);
        let Some(next) = next.filter(|&n| n < text.len()) else {
            break;
        };
        let l = line_at(text, next);
        if !is_table_line(&text[l.clone()]) {
            break;
        }
        end = l.end;
    }
    let first = &text[line_at(text, start)];
    let indent = first[..first.len() - first.trim_start().len()].to_string();
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let rows = text[start..end]
        .lines()
        .map(|l| parse_row(l.trim_end_matches('\r')))
        .collect();
    Some(Table {
        range: start..end,
        indent,
        eol,
        rows,
    })
}

fn is_number(s: &str) -> bool {
    !s.is_empty() && s.trim_start_matches(['-', '+']).parse::<f64>().is_ok()
}

/// Render rows aligned; returns the text and, for each row, the byte offset (relative to the
/// table start) where each cell's content begins.
fn render(t: &Table) -> (String, Vec<Vec<usize>>) {
    let cols = t
        .rows
        .iter()
        .map(|r| match r {
            Row::Cells(c) => c.len(),
            Row::Rule => 0,
        })
        .max()
        .unwrap_or(0)
        .max(1);
    let mut widths = vec![1; cols];
    let mut numeric = vec![(0, 0); cols];
    for r in &t.rows {
        if let Row::Cells(cells) = r {
            for (i, c) in cells.iter().enumerate() {
                widths[i] = widths[i].max(c.width());
                if !c.is_empty() {
                    numeric[i].1 += 1;
                    numeric[i].0 += usize::from(is_number(c));
                }
            }
        }
    }
    let right_align: Vec<bool> = numeric
        .iter()
        .map(|(n, total)| *total > 0 && n * 2 > *total)
        .collect();

    let mut out = String::new();
    let mut cell_starts = Vec::new();
    for (ri, r) in t.rows.iter().enumerate() {
        if ri > 0 {
            out.push_str(t.eol);
        }
        out.push_str(&t.indent);
        let mut starts = Vec::new();
        match r {
            Row::Rule => {
                out.push('|');
                let parts: Vec<String> = widths.iter().map(|w| "-".repeat(w + 2)).collect();
                out.push_str(&parts.join("+"));
                out.push('|');
            }
            Row::Cells(cells) => {
                out.push('|');
                for (i, w) in widths.iter().enumerate() {
                    let c = cells.get(i).map_or("", String::as_str);
                    let pad = " ".repeat(w - c.width());
                    out.push(' ');
                    if right_align[i] {
                        out.push_str(&pad);
                        starts.push(out.len());
                        out.push_str(c);
                    } else {
                        starts.push(out.len());
                        out.push_str(c);
                        out.push_str(&pad);
                    }
                    out.push_str(" |");
                }
            }
        }
        cell_starts.push(starts);
    }
    (out, cell_starts)
}

/// (row, column, offset within the cell's trimmed content) for a buffer offset.
fn locate(text: &str, t: &Table, offset: usize) -> (usize, usize, usize) {
    let mut line_start = t.range.start;
    for (ri, line) in text[t.range.clone()].split('\n').enumerate() {
        let line = line.trim_end_matches('\r');
        let line_end = line_start + line.len();
        if offset <= line_end {
            let rel = offset - line_start;
            let Row::Cells(_) = &t.rows[ri] else {
                return (ri, 0, 0);
            };
            // Column = number of '|' before the cursor (minus the leading one).
            let before = &line[..rel.min(line.len())];
            let col = before.matches('|').count().saturating_sub(1);
            let cell_start = before.rfind('|').map_or(0, |i| i + 1);
            let cell = &line[cell_start..];
            let cell = &cell[..cell.find('|').unwrap_or(cell.len())];
            let lead = cell.len() - cell.trim_start().len();
            let within = rel.saturating_sub(cell_start + lead).min(cell.trim().len());
            return (ri, col, within);
        }
        line_start = line_end + 1 + usize::from(text[line_end..].starts_with("\r\n"));
    }
    (t.rows.len() - 1, 0, 0)
}

fn result(t: &Table, text: &str, row: usize, col: usize, within: usize) -> OpResult {
    let (rendered, starts) = render(t);
    let col = col.min(starts[row].len().saturating_sub(1));
    let cell_len = match &t.rows[row] {
        Row::Cells(c) => c.get(col).map_or(0, String::len),
        Row::Rule => 0,
    };
    let rel = starts[row].get(col).map_or(0, |s| s + within.min(cell_len));
    let tx = if text[t.range.clone()] == rendered {
        Transaction::new()
    } else {
        Transaction::replace(t.range.clone(), rendered)
    };
    OpResult {
        tx,
        selection: Selection::cursor(t.range.start + rel),
    }
}

/// Align the table at the cursor, keeping the cursor in the same cell.
pub fn align(doc: &Document, sel: Selection) -> Option<OpResult> {
    let text = doc.text();
    let t = find_table(text, sel.head)?;
    let (row, col, within) = locate(text, &t, sel.head);
    Some(result(&t, text, row, col, within))
}

/// Align the table and move to the next cell (previous with `back`). Moving forward from the
/// last cell adds a new row, as in Org.
pub fn next_cell(doc: &Document, sel: Selection, back: bool) -> Option<OpResult> {
    let text = doc.text();
    let mut t = find_table(text, sel.head)?;
    let (mut row, mut col, _) = locate(text, &t, sel.head);
    let cols = |r: &Row| match r {
        Row::Cells(c) => c.len(),
        Row::Rule => 0,
    };
    let width = t.rows.iter().map(cols).max().unwrap_or(1).max(1);
    if back {
        loop {
            if col > 0 && matches!(t.rows[row], Row::Cells(_)) {
                col -= 1;
                break;
            }
            if row == 0 {
                col = 0;
                break;
            }
            row -= 1;
            col = width;
        }
    } else {
        loop {
            if matches!(t.rows[row], Row::Cells(_)) && col + 1 < width {
                col += 1;
                break;
            }
            if row + 1 == t.rows.len() {
                t.rows.push(Row::Cells(vec![String::new(); width]));
                row += 1;
                col = 0;
                break;
            }
            row += 1;
            col = usize::MAX; // becomes 0 on the next loop iteration
            if matches!(t.rows[row], Row::Cells(_)) {
                col = 0;
                break;
            }
        }
    }
    // Pad short rows so every row has `width` cells.
    for r in &mut t.rows {
        if let Row::Cells(c) = r {
            c.resize(width, String::new());
        }
    }
    Some(result(&t, text, row, col, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(input: &str, op: impl Fn(&Document, Selection) -> Option<OpResult>) -> String {
        let at = input.rfind('‸').expect("cursor marker");
        let mut text = input.to_string();
        text.replace_range(at..at + '‸'.len_utf8(), "");
        let doc = Document::new(text.clone());
        let res = op(&doc, Selection::cursor(at)).expect("in a table");
        res.tx.apply(&mut text);
        text.insert(res.selection.head, '‸');
        text
    }

    #[test]
    fn aligns_columns_and_numbers() {
        let out = run("| a | bbb |\n|-\n| cc‸ | 7 |\n| d | 10 |\n", align);
        assert_eq!(
            out,
            "| a  | bbb |\n|----+-----|\n| cc‸ |   7 |\n| d  |  10 |\n"
        );
    }

    #[test]
    fn tab_moves_through_cells_and_adds_rows() {
        let tab = |d: &Document, s| next_cell(d, s, false);
        let out = run("| a‸ | b |\n", tab);
        assert_eq!(out, "| a | ‸b |\n");
        let out = run("| a | b‸ |\n|---+---|\n| c | d |\n", tab);
        assert_eq!(out, "| a | b |\n|---+---|\n| ‸c | d |\n");
        let out = run("| a | b‸ |", tab);
        assert_eq!(out, "| a | b |\n| ‸  |   |");
        let back = |d: &Document, s| next_cell(d, s, true);
        let out = run("| a | b |\n| c | ‸d |\n", back);
        assert_eq!(out, "| a | b |\n| ‸c | d |\n");
        let out = run("| a | b |\n| ‸c | d |\n", back);
        assert_eq!(out, "| a | ‸b |\n| c | d |\n");
    }

    #[test]
    fn wide_characters_and_indent() {
        let out = run("  | 日本 | x |\n  | a‸ | y |\n", align);
        assert_eq!(out, "  | 日本 | x |\n  | a‸    | y |\n");
    }

    #[test]
    fn not_in_table() {
        let doc = Document::new("plain\n");
        assert!(align(&doc, Selection::cursor(2)).is_none());
    }
}
