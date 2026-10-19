//! Backslash escapes
//!
//! Allows escapes like `\*hello*`, also processes hard breaks at the end
//! of the line.
//!
//! <https://spec.commonmark.org/0.30/#backslash-escapes>
use crate::MarkdownIt;
use crate::document::{NodeId, TextSpecial};
use crate::parser::inline::{InlineRule, InlineState};
use crate::plugins::cmark::inline::newline::Hardbreak;

pub fn add(md: &mut MarkdownIt) {
    md.inline.add_rule::<EscapeScanner>();
}

#[derive(Clone, Copy)]
enum EscapeMatch {
    Space,
    Hardbreak { len: usize },
    Character { ch: char },
}

impl EscapeMatch {
    fn len(self) -> usize {
        match self {
            Self::Space => 1,
            Self::Hardbreak { len } => len,
            Self::Character { ch } => 1 + ch.len_utf8(),
        }
    }
}

/// Recognize a backslash escape without creating any node.
fn scan_escape(source: &str) -> Option<EscapeMatch> {
    let mut chars = source.chars();
    if chars.next()? != '\\' {
        return None;
    }
    match chars.next()? {
        '\n' => Some(EscapeMatch::Hardbreak {
            // skip leading whitespaces from next line
            len: 2 + chars.take_while(|ch| matches!(ch, ' ' | '\t')).count(),
        }),
        // Emit only the backslash; keep the space pending so newline can
        // still recognize two trailing spaces as a hard break.
        ' ' => Some(EscapeMatch::Space),
        ch => Some(EscapeMatch::Character { ch }),
    }
}

#[doc(hidden)]
pub struct EscapeScanner;
impl InlineRule for EscapeScanner {
    const MARKER: char = '\\';
    const NAMES: &'static [&'static str] = &["escape"];

    fn check(context: &mut InlineState<'_>) -> Option<usize> {
        scan_escape(context.remaining()).map(|matched| matched.len())
    }

    fn run(state: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        let matched = scan_escape(state.remaining())?;
        let len = matched.len();
        match matched {
            EscapeMatch::Space => Some((
                Some(state.document.create_node(TextSpecial {
                    content: "\\".into(),
                    markup: "\\".into(),
                    info: "escape",
                })),
                len,
            )),
            EscapeMatch::Hardbreak { .. } => {
                Some((Some(state.document.create_node(Hardbreak)), len))
            }
            EscapeMatch::Character { ch } => {
                let markup = format!("\\{ch}");
                let content = if ch.is_ascii_punctuation() {
                    ch.to_string()
                } else {
                    markup.clone()
                };
                Some((
                    Some(state.document.create_node(TextSpecial {
                        content,
                        markup,
                        info: "escape",
                    })),
                    len,
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_escape_reports_lengths_without_building_nodes() {
        assert!(matches!(
            scan_escape("\\\n \t"),
            Some(EscapeMatch::Hardbreak { len: 4 })
        ));
        assert!(matches!(
            scan_escape("\\\n"),
            Some(EscapeMatch::Hardbreak { len: 2 })
        ));
        assert!(matches!(scan_escape("\\ "), Some(EscapeMatch::Space)));
        assert_eq!(scan_escape("\\ ").unwrap().len(), 1);
        assert!(scan_escape("\\").is_none());
        assert!(scan_escape("x").is_none());
        assert!(matches!(
            scan_escape("\\*"),
            Some(EscapeMatch::Character { ch: '*' })
        ));
        assert_eq!(scan_escape("\\*").unwrap().len(), 2);

        // A non-ASCII character keeps its full UTF-8 width.
        assert!(matches!(
            scan_escape("\\雪"),
            Some(EscapeMatch::Character { ch: '雪' })
        ));
        assert_eq!(scan_escape("\\雪").unwrap().len(), 4);
    }
}
