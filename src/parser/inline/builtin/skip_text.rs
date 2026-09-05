//! Skip text characters for text token, place those to pending buffer
//! and increment current pos
//!
use regex::{self, Regex};

use crate::parser::document::NodeDraft;
use crate::parser::document_parser::DocumentInlineState;
use crate::parser::inline::{DocumentInlineRule, InlineRule, InlineState};
use crate::parser::main::MarkdownIt;
use crate::parser::node::{Node, NodeValue};
use crate::parser::renderer::Renderer;

#[derive(Debug)]
/// Plain text AST node.
pub struct Text {
    pub content: String,
}

impl AsRef<str> for Text {
    fn as_ref(&self) -> &str {
        &self.content
    }
}

impl NodeValue for Text {
    fn render(&self, _: &Node, fmt: &mut dyn Renderer) {
        fmt.text(&self.content);
    }
}

#[derive(Debug)]
/// Escaped text AST node (backslash escapes and entities).
pub struct TextSpecial {
    pub content: String,
    pub markup: String,
    pub info: &'static str,
}

impl AsRef<str> for TextSpecial {
    fn as_ref(&self) -> &str {
        &self.content
    }
}

impl NodeValue for TextSpecial {
    fn render(&self, _: &Node, fmt: &mut dyn Renderer) {
        fmt.text(&self.content);
    }
}

pub fn add(md: &mut MarkdownIt) {
    md.inline
        .add_rule_with_document::<TextScanner>()
        .before_all();
}

impl DocumentInlineRule for TextScanner {
    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
        let len = state.src[state.pos..state.pos_max]
            .char_indices()
            .find_map(|(offset, marker)| state.is_rule_marker(marker).then_some(offset))
            .unwrap_or(state.pos_max - state.pos);
        if len == 0 {
            return None;
        }
        state.push_text(state.pos, state.pos + len);
        Some((None, len))
    }
}

#[derive(Debug)]
pub(crate) enum TextScannerImpl {
    SkipPunct,
    SkipRegex(Regex),
}

/// Rule to skip pure text
/// '{}$%@~+=:' reserved for extensions
///
/// !, ", #, $, %, &, ', (, ), *, +, ,, -, ., /, :, ;, <, =, >, ?, @, [, \, ], ^, _, `, {, |, }, or ~
///
/// !!!! Don't confuse with "Markdown ASCII Punctuation" chars
/// <http://spec.commonmark.org/0.15/#ascii-punctuation-character>
///
pub struct TextScanner;

impl TextScanner {
    fn choose_text_impl(charmap: Vec<char>) -> TextScannerImpl {
        let mut can_use_punct = true;
        for ch in charmap.iter() {
            match ch {
                '\n' | '!' | '#' | '$' | '%' | '&' | '*' | '+' | '-' | ':' | '<' | '=' | '>'
                | '@' | '[' | '\\' | ']' | '^' | '_' | '`' | '{' | '}' | '~' => {}
                _ => {
                    can_use_punct = false;
                    break;
                }
            }
        }

        if can_use_punct {
            TextScannerImpl::SkipPunct
        } else {
            TextScannerImpl::SkipRegex(
                Regex::new(
                    // [] panics on "unclosed character class", but it cannot happen here
                    // (we'd use punct rule instead)
                    &format!(
                        "^[^{}]+",
                        charmap
                            .into_iter()
                            .map(|c| regex::escape(&c.to_string()))
                            .collect::<String>()
                    ),
                )
                .unwrap(),
            )
        }
    }

    fn find_text_length(state: &mut InlineState) -> usize {
        let text_impl = state.md.inline.text_impl.get_or_init(|| {
            Self::choose_text_impl(state.md.inline.text_charmap.keys().copied().collect())
        });

        let mut len = 0;

        match text_impl {
            TextScannerImpl::SkipPunct => {
                let mut chars = state.src[state.pos..state.pos_max].chars();

                loop {
                    match chars.next() {
                        Some(
                            '\n' | '!' | '#' | '$' | '%' | '&' | '*' | '+' | '-' | ':' | '<' | '='
                            | '>' | '@' | '[' | '\\' | ']' | '^' | '_' | '`' | '{' | '}' | '~',
                        ) => {
                            break;
                        }
                        Some(chr) => {
                            len += chr.len_utf8();
                        }
                        None => {
                            break;
                        }
                    }
                }
            }
            TextScannerImpl::SkipRegex(re) => {
                if let Some(capture) = re.find(&state.src[state.pos..state.pos_max]) {
                    len = capture.end();
                }
            }
        }

        len
    }
}

impl InlineRule for TextScanner {
    const MARKER: char = '\0';
    const NAMES: &'static [&'static str] = &["text"];

    fn check(state: &mut InlineState) -> Option<usize> {
        let len = Self::find_text_length(state);
        if len == 0 {
            return None;
        }
        Some(len)
    }

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let len = Self::find_text_length(state);
        if len == 0 {
            return None;
        }
        state.trailing_text_push(state.pos, state.pos + len);
        state.pos += len;
        Some((Node::default(), 0))
    }
}
