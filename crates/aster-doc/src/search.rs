//! Find and replace.

use std::ops::Range;

use crate::Transaction;

/// All non-overlapping matches of `query`. Smart case: case-insensitive unless the query
/// contains an uppercase letter.
pub fn find_all(text: &str, query: &str) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    let case_sensitive = query.chars().any(char::is_uppercase);
    let q: Vec<char> = if case_sensitive {
        query.chars().collect()
    } else {
        query.chars().flat_map(char::to_lowercase).collect()
    };
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut out = Vec::new();
    let mut i = 0;
    'outer: while i < chars.len() {
        let mut qi = 0;
        let mut j = i;
        while qi < q.len() {
            let Some(&(_, c)) = chars.get(j) else {
                break 'outer;
            };
            let matched = if case_sensitive {
                c == q[qi] && {
                    qi += 1;
                    true
                }
            } else {
                // Compare the lowercase expansion of `c` against the next query chars.
                let mut ok = true;
                for lc in c.to_lowercase() {
                    if q.get(qi) == Some(&lc) {
                        qi += 1;
                    } else {
                        ok = false;
                        break;
                    }
                }
                ok
            };
            if !matched {
                i += 1;
                continue 'outer;
            }
            j += 1;
        }
        let start = chars[i].0;
        let end = chars.get(j).map_or(text.len(), |&(b, _)| b);
        out.push(start..end);
        i = j.max(i + 1);
    }
    out
}

/// Replace every match in one transaction (a single undo step).
pub fn replace_all(matches: &[Range<usize>], replacement: &str) -> Transaction {
    matches
        .iter()
        .fold(Transaction::new(), |tx, r| tx.with(r.clone(), replacement))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smart_case() {
        let t = "Todo todo TODO";
        assert_eq!(find_all(t, "todo").len(), 3);
        assert_eq!(find_all(t, "TODO"), vec![10..14]);
        assert_eq!(find_all(t, ""), vec![]);
    }

    #[test]
    fn unicode_and_overlap() {
        assert_eq!(find_all("Ελληνικά ελληνικά", "ελλ").len(), 2);
        assert_eq!(find_all("日本語の日本", "日本"), vec![0..6, 12..18]);
        assert_eq!(find_all("aaaa", "aa"), vec![0..2, 2..4]);
    }

    #[test]
    fn replace_everything_in_one_step() {
        let mut text = "a cat, a CAT".to_string();
        let m = find_all(&text, "cat");
        replace_all(&m, "dog").apply(&mut text);
        assert_eq!(text, "a dog, a dog");
    }
}
