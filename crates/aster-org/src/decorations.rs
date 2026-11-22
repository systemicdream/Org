//! Presentation hints for live preview: which markup to hide, replace, or style.
//!
//! The editor shows raw text for an element whose range contains the cursor, and the
//! decorated form otherwise. Ranges are byte offsets into the document.

#![allow(clippy::single_range_in_vec_init)] // `hide` lists ranges; a single one is common.

use std::ops::Range;

use orgize::rowan::ast::AstNode;
use orgize::{SyntaxElement, SyntaxKind, SyntaxNode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecoKind {
    Bold,
    Italic,
    Underline,
    Verbatim,
    Code,
    Strike,
    /// A link; `target` is the raw path (e.g. `https://…`, `file:x.org`, `*Heading`, `id:…`).
    Link {
        target: String,
        has_description: bool,
    },
    Checkbox {
        checked: bool,
        partial: bool,
    },
    /// An unordered list bullet character (`-`, `+`, or indented `*`).
    Bullet,
    Timestamp,
    /// Leading heading stars and the space after them.
    HeadingStars,
    /// A `#+TITLE:` line; the prefix is hidden and the value shown as the document title.
    Title,
    /// Any other `#+KEY: value` line.
    Keyword,
    /// `#+begin_…` / `#+end_…` lines.
    BlockDelimiter(BlockKind),
    /// Lines inside a block. `lang` is set for source blocks.
    BlockContent {
        block: BlockKind,
        lang: Option<String>,
    },
    /// A `:DRAWER:` … `:END:` region (including property drawers).
    Drawer,
    /// SCHEDULED / DEADLINE / CLOSED line.
    Planning,
    /// A table (all its lines).
    Table,
    /// `: fixed width` lines.
    FixedWidth,
    /// `# comment` lines.
    Comment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Source,
    Example,
    Quote,
    Verse,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoration {
    pub kind: DecoKind,
    /// The whole element.
    pub range: Range<usize>,
    /// Byte ranges hidden while the element isn't being edited.
    pub hide: Vec<Range<usize>>,
    /// A byte range shown as a different string while not being edited (e.g. `[ ]` → `☐`).
    pub replace: Option<(Range<usize>, &'static str)>,
}

impl Decoration {
    fn styled(kind: DecoKind, range: Range<usize>) -> Self {
        Self {
            kind,
            range,
            hide: Vec::new(),
            replace: None,
        }
    }

    /// Whether this decoration covers whole lines (blocks, tables, drawers…) rather than inline text.
    pub fn is_line_level(&self) -> bool {
        matches!(
            self.kind,
            DecoKind::BlockDelimiter(_)
                | DecoKind::BlockContent { .. }
                | DecoKind::Drawer
                | DecoKind::Planning
                | DecoKind::Table
                | DecoKind::FixedWidth
                | DecoKind::Comment
                | DecoKind::Keyword
                | DecoKind::Title
        )
    }
}

fn range_of(node: &SyntaxNode) -> Range<usize> {
    let r = node.text_range();
    usize::from(r.start())..usize::from(r.end())
}

fn range_of_el(el: &SyntaxElement) -> Range<usize> {
    let r = el.text_range();
    usize::from(r.start())..usize::from(r.end())
}

fn block_kind(kind: SyntaxKind) -> Option<BlockKind> {
    Some(match kind {
        SyntaxKind::SOURCE_BLOCK => BlockKind::Source,
        SyntaxKind::EXAMPLE_BLOCK => BlockKind::Example,
        SyntaxKind::QUOTE_BLOCK => BlockKind::Quote,
        SyntaxKind::VERSE_BLOCK => BlockKind::Verse,
        SyntaxKind::CENTER_BLOCK
        | SyntaxKind::SPECIAL_BLOCK
        | SyntaxKind::COMMENT_BLOCK
        | SyntaxKind::EXPORT_BLOCK => BlockKind::Other,
        _ => return None,
    })
}

/// Collect decorations for a document, sorted by start offset.
pub(crate) fn collect(root: &SyntaxNode) -> Vec<Decoration> {
    let mut out = Vec::new();
    for node in root.descendants() {
        let range = range_of(&node);
        let kind = node.kind();
        let emphasis = match kind {
            SyntaxKind::BOLD => Some(DecoKind::Bold),
            SyntaxKind::ITALIC => Some(DecoKind::Italic),
            SyntaxKind::UNDERLINE => Some(DecoKind::Underline),
            SyntaxKind::VERBATIM => Some(DecoKind::Verbatim),
            SyntaxKind::CODE => Some(DecoKind::Code),
            SyntaxKind::STRIKE => Some(DecoKind::Strike),
            _ => None,
        };
        if let Some(k) = emphasis {
            if range.len() >= 2 {
                out.push(Decoration {
                    kind: k,
                    hide: vec![range.start..range.start + 1, range.end - 1..range.end],
                    range,
                    replace: None,
                });
            }
            continue;
        }
        if let Some(block) = block_kind(kind) {
            let mut lang = None;
            for child in node.children() {
                let r = range_of(&child);
                match child.kind() {
                    SyntaxKind::BLOCK_BEGIN => {
                        lang = child
                            .children_with_tokens()
                            .find(|c| c.kind() == SyntaxKind::SRC_BLOCK_LANGUAGE)
                            .map(|c| c.to_string());
                        out.push(Decoration::styled(DecoKind::BlockDelimiter(block), r));
                    }
                    SyntaxKind::BLOCK_END => {
                        out.push(Decoration::styled(DecoKind::BlockDelimiter(block), r));
                    }
                    SyntaxKind::BLOCK_CONTENT => out.push(Decoration::styled(
                        DecoKind::BlockContent {
                            block,
                            lang: lang.clone(),
                        },
                        r,
                    )),
                    _ => {}
                }
            }
            continue;
        }
        match kind {
            SyntaxKind::LINK => {
                let tokens: Vec<SyntaxElement> = node.children_with_tokens().collect();
                let path = tokens.iter().find(|t| t.kind() == SyntaxKind::LINK_PATH);
                let Some(path) = path else { continue };
                let path_range = range_of_el(path);
                let target = path.to_string();
                let has_description = tokens.iter().any(|t| t.kind() == SyntaxKind::R_BRACKET);
                let hide = if has_description && range.len() >= path_range.end - range.start + 4 {
                    // [[target][description]]
                    vec![range.start..path_range.end + 2, range.end - 2..range.end]
                } else if range.len() >= 4 {
                    vec![range.start..range.start + 2, range.end - 2..range.end]
                } else {
                    continue;
                };
                out.push(Decoration {
                    kind: DecoKind::Link {
                        target,
                        has_description,
                    },
                    range,
                    hide,
                    replace: None,
                });
            }
            SyntaxKind::LIST_ITEM_CHECK_BOX => {
                let mark = node.to_string();
                let state = mark.chars().nth(1).unwrap_or(' ');
                let (checked, partial) = (matches!(state, 'X' | 'x'), state == '-');
                let glyph = if checked {
                    "☑"
                } else if partial {
                    "⊟"
                } else {
                    "☐"
                };
                out.push(Decoration {
                    kind: DecoKind::Checkbox { checked, partial },
                    replace: Some((range.clone(), glyph)),
                    range,
                    hide: Vec::new(),
                });
            }
            SyntaxKind::TIMESTAMP_ACTIVE | SyntaxKind::TIMESTAMP_INACTIVE => {
                out.push(Decoration::styled(DecoKind::Timestamp, range));
            }
            SyntaxKind::KEYWORD => {
                let tokens: Vec<SyntaxElement> = node.children_with_tokens().collect();
                let key = tokens
                    .iter()
                    .find(|t| t.kind() == SyntaxKind::TEXT)
                    .map(|t| t.to_string())
                    .unwrap_or_default();
                let colon = tokens.iter().find(|t| t.kind() == SyntaxKind::COLON);
                match colon {
                    Some(colon) if key.eq_ignore_ascii_case("title") => {
                        let after = range_of_el(colon).end;
                        let text = node.to_string();
                        let rel = after - range.start;
                        let spaces = text[rel..].len() - text[rel..].trim_start_matches(' ').len();
                        out.push(Decoration {
                            kind: DecoKind::Title,
                            hide: vec![range.start..after + spaces],
                            range,
                            replace: None,
                        });
                    }
                    _ => out.push(Decoration::styled(DecoKind::Keyword, range)),
                }
            }
            SyntaxKind::DRAWER | SyntaxKind::PROPERTY_DRAWER => {
                out.push(Decoration::styled(DecoKind::Drawer, range));
            }
            SyntaxKind::PLANNING => out.push(Decoration::styled(DecoKind::Planning, range)),
            SyntaxKind::ORG_TABLE => out.push(Decoration::styled(DecoKind::Table, range)),
            SyntaxKind::FIXED_WIDTH => out.push(Decoration::styled(DecoKind::FixedWidth, range)),
            SyntaxKind::COMMENT => out.push(Decoration::styled(DecoKind::Comment, range)),
            _ => {}
        }
    }
    // Tokens: heading stars and list bullets.
    for el in root.descendants_with_tokens() {
        let Some(token) = el.as_token() else { continue };
        let r = token.text_range();
        let range = usize::from(r.start())..usize::from(r.end());
        match token.kind() {
            SyntaxKind::HEADLINE_STARS => {
                let ws = token
                    .next_token()
                    .filter(|t| t.kind() == SyntaxKind::WHITESPACE)
                    .map_or(0, |t| t.text().len());
                out.push(Decoration {
                    kind: DecoKind::HeadingStars,
                    hide: vec![range.start..range.end + ws],
                    range: range.start..range.end + ws,
                    replace: None,
                });
            }
            SyntaxKind::LIST_ITEM_BULLET => {
                let text = token.text();
                if text.starts_with(['-', '+', '*']) {
                    out.push(Decoration {
                        kind: DecoKind::Bullet,
                        replace: Some((range.start..range.start + 1, "•")),
                        range: range.start..range.start + 1,
                        hide: Vec::new(),
                    });
                }
            }
            _ => {}
        }
    }
    out.sort_by_key(|d| d.range.start);
    out
}

/// Helper so callers outside this module can use the same AST trait.
pub(crate) fn root_of(org: &orgize::Org) -> SyntaxNode {
    org.document().syntax().clone()
}

#[cfg(test)]
mod tests {
    use crate::OrgTree;

    use super::*;

    fn decos(text: &str) -> Vec<Decoration> {
        OrgTree::parse(text).decorations()
    }

    fn hidden(text: &str, d: &Decoration) -> Vec<String> {
        d.hide.iter().map(|r| text[r.clone()].to_string()).collect()
    }

    #[test]
    fn emphasis_and_links() {
        let t = "a *b* [[https://x.org][desc]] [[file:pic.png]]\n";
        let ds = decos(t);
        let bold = ds.iter().find(|d| d.kind == DecoKind::Bold).unwrap();
        assert_eq!(hidden(t, bold), ["*", "*"]);
        let links: Vec<_> = ds
            .iter()
            .filter(|d| matches!(d.kind, DecoKind::Link { .. }))
            .collect();
        assert_eq!(hidden(t, links[0]), ["[[https://x.org][", "]]"]);
        assert_eq!(
            links[0].kind,
            DecoKind::Link {
                target: "https://x.org".into(),
                has_description: true
            }
        );
        assert_eq!(hidden(t, links[1]), ["[[", "]]"]);
    }

    #[test]
    fn lists_headings_title() {
        let t = "#+TITLE:  My notes\n* TODO Head\n- [X] done\n  + [ ] todo\n1. [-] num\n";
        let ds = decos(t);
        let title = ds.iter().find(|d| d.kind == DecoKind::Title).unwrap();
        assert_eq!(hidden(t, title), ["#+TITLE:  "]);
        let stars = ds
            .iter()
            .find(|d| d.kind == DecoKind::HeadingStars)
            .unwrap();
        assert_eq!(hidden(t, stars), ["* "]);
        let boxes: Vec<_> = ds
            .iter()
            .filter_map(|d| d.replace.as_ref().map(|(r, g)| (&t[r.clone()], *g)))
            .collect();
        assert_eq!(
            boxes,
            [
                ("-", "•"),
                ("[X]", "☑"),
                ("+", "•"),
                ("[ ]", "☐"),
                ("[-]", "⊟")
            ]
        );
    }

    #[test]
    fn blocks_tables_drawers() {
        let t = "#+begin_src rust\nfn x() {}\n#+end_src\n| a |\n:PROPERTIES:\n:ID: 1\n:END:\n";
        let ds = decos(t);
        let content = ds
            .iter()
            .find(|d| matches!(d.kind, DecoKind::BlockContent { .. }))
            .unwrap();
        assert_eq!(&t[content.range.clone()], "fn x() {}\n");
        assert_eq!(
            content.kind,
            DecoKind::BlockContent {
                block: BlockKind::Source,
                lang: Some("rust".into())
            }
        );
        assert!(ds.iter().any(|d| d.kind == DecoKind::Table));
        assert!(ds.iter().any(|d| d.kind == DecoKind::Drawer));
        assert_eq!(
            ds.iter()
                .filter(|d| matches!(d.kind, DecoKind::BlockDelimiter(_)))
                .count(),
            2
        );
    }
}
