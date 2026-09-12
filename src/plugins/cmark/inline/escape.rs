//! Backslash escapes
//!
//! Allows escapes like `\*hello*`, also processes hard breaks at the end
//! of the line.
//!
//! <https://spec.commonmark.org/0.30/#backslash-escapes>
use crate::parser::document::NodeDraft;
use crate::parser::document_parser::DocumentInlineState;
use crate::parser::inline::{
    InlineProbeContext,
    InlineProbeKind,
    InlineProbeResult,
    InlineRule,
    InlineState,
    LegacyInlineRule,
    TextSpecial,
};
use crate::parser::main::MarkdownIt;
use crate::parser::node::Node;
use crate::plugins::cmark::inline::newline::Hardbreak;

pub fn add(md: &mut MarkdownIt) {
    md.inline.add_migrated_rule::<EscapeScanner>();
}

#[derive(Clone, Copy)]
enum EscapeMatch {
    Hardbreak { len: usize },
    Character { ch: char },
}

impl EscapeMatch {
    fn len(self) -> usize {
        match self {
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
            len: 2 + chars.take_while(|ch| matches!(ch, ' ' | '\t')).count(),
        }),
        ' ' => None,
        ch => Some(EscapeMatch::Character { ch }),
    }
}

#[doc(hidden)]
pub struct EscapeScanner;
impl InlineRule for EscapeScanner {
    const MARKER: char = '\\';
    const NAMES: &'static [&'static str] = &["escape"];

    fn probe(context: &mut InlineProbeContext<'_>) -> InlineProbeResult {
        match scan_escape(context.remaining()) {
            Some(matched) => InlineProbeResult::Match {
                len: matched.len(),
                kind: InlineProbeKind::Token,
            },
            None => InlineProbeResult::NoMatch,
        }
    }

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
        let matched = scan_escape(state.remaining())?;
        let len = matched.len();
        match matched {
            EscapeMatch::Hardbreak { .. } => Some((Some(NodeDraft::new(Hardbreak)), len)),
            EscapeMatch::Character { ch } => {
                let markup = format!("\\{ch}");
                let content = if ch.is_ascii_punctuation() {
                    ch.to_string()
                } else {
                    markup.clone()
                };
                Some((
                    Some(NodeDraft::new(TextSpecial {
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

impl LegacyInlineRule for EscapeScanner {
    const MARKER: char = '\\';
    const NAMES: &'static [&'static str] = &["escape"];

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let mut chars = state.src[state.pos..state.pos_max].chars();
        if chars.next().unwrap() != '\\' {
            return None;
        }

        match chars.next() {
            Some('\n') => {
                // skip leading whitespaces from next line
                let mut len = 2;
                while let Some(' ' | '\t') = chars.next() {
                    len += 1;
                }
                Some((Node::new(Hardbreak), len))
            }
            // A space is not escapable. Leave both characters in the pending
            // text so the newline rule can still see two trailing spaces.
            Some(' ') => None,
            Some(chr) => {
                let start = state.pos;
                let end = state.pos + 1 + chr.len_utf8();

                let mut orig_str = "\\".to_owned();
                orig_str.push(chr);

                let content_str = match chr {
                    '\\' | '!' | '"' | '#' | '$' | '%' | '&' | '\'' | '(' | ')' | '*' | '+'
                    | ',' | '.' | '/' | ':' | ';' | '<' | '=' | '>' | '?' | '@' | '[' | ']'
                    | '^' | '_' | '`' | '{' | '|' | '}' | '~' | '-' => chr.into(),
                    _ => orig_str.clone(),
                };

                let node = Node::new(TextSpecial {
                    content: content_str,
                    markup: orig_str,
                    info: "escape",
                });
                Some((node, end - start))
            }
            None => None,
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
        assert!(scan_escape("\\ ").is_none());
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
