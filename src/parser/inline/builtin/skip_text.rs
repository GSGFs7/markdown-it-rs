//! Skip text characters for text token, place those to pending buffer
//! and increment current pos
//!
use regex::{self, Regex};

use crate::parser::document::NodeDraft;
use crate::parser::document_parser::DocumentInlineState;
use crate::parser::inline::{InlineRule, InlineState, LegacyInlineRule};
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
    md.inline.add_migrated_rule::<TextScanner>().before_all();
}

#[derive(Debug)]
pub(crate) enum TextScannerImpl {
    SkipAscii(AsciiMarkSet),
    SkipRegex(Regex),
}

impl TextScannerImpl {
    pub(in crate::parser::inline) fn compile(mut markers: Vec<char>) -> Self {
        markers.sort_unstable();

        if markers.iter().all(|marker| marker.is_ascii()) {
            let mut set = AsciiMarkSet::default();
            for marker in markers {
                set.insert(marker);
            }
            return Self::SkipAscii(set);
        }

        let escaped = markers
            .into_iter()
            .map(|marker| regex::escape(&marker.to_string()))
            .collect::<String>();
        Self::SkipRegex(Regex::new(&format!("^[^{escaped}]+")).unwrap())
    }

    #[inline]
    pub(in crate::parser::inline) fn find(&self, source: &str) -> usize {
        match self {
            Self::SkipAscii(markers) => source
                .as_bytes()
                .iter()
                .position(|byte| markers.contains(*byte))
                .unwrap_or(source.len()),
            Self::SkipRegex(regex) => regex.find(source).map_or(0, |capture| capture.end()),
        }
    }
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
    fn find_text_length(state: &mut InlineState) -> usize {
        state
            .md
            .inline
            .text_length(&state.src, state.pos, state.pos_max)
    }
}

impl InlineRule for TextScanner {
    const MARKER: char = '\0';
    const NAMES: &'static [&'static str] = &["text"];

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
        let len = state
            .markdown_it()
            .inline
            .text_length(&state.src, state.pos, state.pos_max);
        if len == 0 {
            return None;
        }

        state.push_text(state.pos, state.pos + len);
        Some((None, len))
    }
}

impl LegacyInlineRule for TextScanner {
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

#[derive(Debug, Clone, Copy, Default)]
pub(in crate::parser::inline) struct AsciiMarkSet([u64; 2]);

impl AsciiMarkSet {
    fn insert(&mut self, marker: char) {
        debug_assert!(marker.is_ascii());
        let byte = marker as u8;
        self.0[(byte / 64) as usize] |= 1_u64 << (byte % 64);
    }

    #[inline]
    fn contains(self, byte: u8) -> bool {
        byte.is_ascii() && self.0[(byte / 64) as usize] & (1_u64 << (byte % 64)) != 0
    }
}
