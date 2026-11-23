//! Text navigation helpers: grapheme and word boundaries.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

fn is_word(segment: &str) -> bool {
    segment.chars().any(char::is_alphanumeric)
}

/// Previous grapheme boundary (treats `\r\n` as one step).
pub fn prev_grapheme(text: &str, offset: usize) -> usize {
    if offset == 0 {
        return 0;
    }
    if text[..offset].ends_with("\r\n") {
        return offset - 2;
    }
    text[..offset]
        .grapheme_indices(true)
        .next_back()
        .map_or(0, |(i, _)| i)
}

/// Next grapheme boundary.
pub fn next_grapheme(text: &str, offset: usize) -> usize {
    text[offset..]
        .graphemes(true)
        .next()
        .map_or(text.len(), |g| offset + g.len())
}

/// Start of the word before `offset`, skipping whitespace and punctuation in between.
pub fn prev_word_start(text: &str, offset: usize) -> usize {
    text[..offset]
        .split_word_bound_indices()
        .rev()
        .find(|(_, s)| is_word(s))
        .map_or(0, |(i, _)| i)
}

/// End of the word after `offset`, skipping whitespace and punctuation in between.
pub fn next_word_end(text: &str, offset: usize) -> usize {
    text[offset..]
        .split_word_bound_indices()
        .find(|(_, s)| is_word(s))
        .map_or(text.len(), |(i, s)| offset + i + s.len())
}

/// The word (or run of non-word text) at `offset`, for double-click selection.
pub fn word_at(text: &str, offset: usize) -> Range<usize> {
    let line_start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[offset..].find('\n').map_or(text.len(), |i| offset + i);
    let line = &text[line_start..line_end];
    let rel = offset - line_start;
    let mut best = rel..rel;
    for (i, s) in line.split_word_bound_indices() {
        let r = i..i + s.len();
        // Prefer a word touching the offset (so clicking just after a word selects it).
        if r.contains(&rel) || (r.end == rel && is_word(s)) {
            best = r;
            if is_word(s) {
                break;
            }
        }
    }
    best.start + line_start..best.end + line_start
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_movement() {
        let t = "hello, wide world!  ok";
        assert_eq!(next_word_end(t, 0), 5);
        assert_eq!(next_word_end(t, 5), 11);
        assert_eq!(next_word_end(t, 17), 22);
        assert_eq!(prev_word_start(t, 22), 20);
        assert_eq!(prev_word_start(t, 20), 12);
        assert_eq!(prev_word_start(t, 9), 7);
        assert_eq!(prev_word_start(t, 3), 0);
        assert_eq!(next_word_end(t, 22), 22);
    }

    #[test]
    fn word_movement_unicode() {
        let t = "café naïve 🎉 end";
        assert_eq!(next_word_end(t, 0), "café".len());
        assert_eq!(prev_word_start(t, t.len()), t.len() - 3);
    }

    #[test]
    fn double_click_word() {
        let t = "* TODO buy milk\nnext";
        let at = |s: &str| t.find(s).unwrap();
        assert_eq!(&t[word_at(t, at("milk") + 1)], "milk");
        assert_eq!(&t[word_at(t, at("milk") + 4)], "milk"); // just after the word
        assert_eq!(&t[word_at(t, at("TODO"))], "TODO");
        assert_eq!(&t[word_at(t, at("next"))], "next");
    }

    #[test]
    fn graphemes() {
        let t = "a\r\ne\u{301}";
        assert_eq!(next_grapheme(t, 0), 1);
        assert_eq!(next_grapheme(t, 1), 3);
        assert_eq!(prev_grapheme(t, 3), 1);
        assert_eq!(next_grapheme(t, 3), t.len());
    }
}
