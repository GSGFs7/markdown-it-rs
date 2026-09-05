//! Backslash escapes
//!
//! Allows escapes like `\*hello*`, also processes hard breaks at the end
//! of the line.
//!
//! <https://spec.commonmark.org/0.30/#backslash-escapes>
use crate::parser::document::NodeDraft;
use crate::parser::document_parser::DocumentInlineState;
use crate::parser::inline::{InlineRule, InlineState, LegacyInlineRule, TextSpecial};
use crate::parser::main::MarkdownIt;
use crate::parser::node::Node;
use crate::plugins::cmark::inline::newline::Hardbreak;

pub fn add(md: &mut MarkdownIt) {
    md.inline.add_migrated_rule::<EscapeScanner>();
}

#[doc(hidden)]
pub struct EscapeScanner;
impl InlineRule for EscapeScanner {
    const MARKER: char = '\\';
    const NAMES: &'static [&'static str] = &["escape"];

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
        let mut chars = state.src[state.pos..state.pos_max].chars();
        if chars.next()? != '\\' {
            return None;
        }
        match chars.next()? {
            '\n' => {
                let len = 2 + chars.take_while(|ch| matches!(ch, ' ' | '\t')).count();
                Some((Some(NodeDraft::new(Hardbreak)), len))
            }
            ' ' => None,
            ch => {
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
                    1 + ch.len_utf8(),
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
