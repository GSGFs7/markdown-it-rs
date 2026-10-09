//! Skip text characters for text token, place those to pending buffer
//! and increment current pos
//!
use regex::{self, Regex};

use crate::MarkdownIt;
use crate::document::NodeId;
use crate::parser::inline::{DocumentInlineState, InlineRule};

pub fn add(md: &mut MarkdownIt) {
    md.inline.add_rule::<TextScanner>().before_all();
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

impl InlineRule for TextScanner {
    const MARKER: char = '\0';
    const NAMES: &'static [&'static str] = &["text"];

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
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
