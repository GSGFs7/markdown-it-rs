use std::ops::Range;

use crate::MarkdownIt;
use crate::parser::inline::helpers::paired_span::{self, PairMatch, PairOptions};

/// Cursor passed to an inline leaf parser.
///
/// The cursor is local to one callback. Consumption is committed only when the
/// callback returns `Some`, so a failed match rolls back automatically.
#[derive(Debug)]
pub struct InlinePluginState<'a> {
    source: &'a str,
    consumed: usize,
    markdown_it: &'a MarkdownIt,
}

impl<'a> InlinePluginState<'a> {
    pub(super) fn new(source: &'a str, markdown_it: &'a MarkdownIt) -> Self {
        Self {
            source,
            consumed: 0,
            markdown_it,
        }
    }

    /// Unconsumed source available to this rule.
    pub fn remaining(&self) -> &'a str {
        &self.source[self.consumed..]
    }

    /// Return the part of [`remaining`](Self::remaining) covered by `range`.
    ///
    /// Offsets are relative to the remaining source. Returns `None` if the
    /// range is reversed, out of bounds, or not on UTF-8 boundaries.
    pub fn remaining_slice(&self, range: Range<usize>) -> Option<&'a str> {
        self.remaining().get(range)
    }

    /// The complete source window visible to this rule.
    pub fn source(&self) -> &'a str {
        self.source
    }

    /// Return the part of [`source`](Self::source) covered by `range`.
    ///
    /// Offsets are relative to the complete source window. Returns `None` if
    /// the range is reversed, out of bounds, or not on UTF-8 boundaries.
    pub fn source_slice(&self, range: Range<usize>) -> Option<&'a str> {
        self.source.get(range)
    }

    /// Scan a paired span at the start of [`remaining`](Self::remaining).
    ///
    /// The returned offsets are relative to the remaining source. Use
    /// [`consume_pair`](Self::consume_pair) to scan and advance in one step.
    pub fn scan_pair(&self, options: &PairOptions<'_>) -> Option<PairMatch> {
        paired_span::scan(self.remaining(), options)
    }

    /// Scan a paired span and consume its bytes.
    ///
    /// The returned offsets are relative to the previous
    /// [`remaining`](Self::remaining), before the cursor advances.
    pub fn consume_pair(&mut self, options: &PairOptions<'_>) -> Option<PairMatch> {
        let matched = self.scan_pair(options)?;
        self.advance(matched.consumed).then_some(matched)
    }

    /// Parser instance that owns the active inline rule.
    pub fn markdown_it(&self) -> &'a MarkdownIt {
        self.markdown_it
    }

    /// Number of bytes consumed so far.
    pub fn consumed(&self) -> usize {
        self.consumed
    }

    /// Whether the unconsumed source starts with `literal`.
    pub fn starts_with(&self, literal: &str) -> bool {
        self.remaining().starts_with(literal)
    }

    /// Consume `literal` if it is next, returning whether it matched.
    pub fn consume(&mut self, literal: &str) -> bool {
        if self.remaining().starts_with(literal) {
            self.consumed += literal.len();
            true
        } else {
            false
        }
    }

    /// Consume one character, returning it when one is available.
    pub fn consume_char(&mut self) -> Option<char> {
        let ch = self.remaining().chars().next()?;
        self.consumed += ch.len_utf8();
        Some(ch)
    }

    /// Consume exactly `bytes` bytes.
    ///
    /// Returns `false` without moving the cursor when the range is outside the
    /// source or not on a UTF-8 boundary.
    pub fn advance(&mut self, bytes: usize) -> bool {
        if self.remaining().get(bytes..).is_none() {
            return false;
        }
        self.consumed += bytes;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slices_are_relative_and_checked() {
        let md = MarkdownIt::empty();
        let mut cx = InlinePluginState::new("a雪b", &md);

        assert_eq!(cx.source_slice(0..1), Some("a"));
        assert_eq!(cx.source_slice(1..4), Some("雪"));
        assert_eq!(cx.remaining_slice(1..4), Some("雪"));
        assert_eq!(cx.source_slice(1..2), None);
        assert_eq!(cx.source_slice(0..6), None);

        assert!(cx.advance(1));
        assert_eq!(cx.remaining(), "雪b");
        assert_eq!(cx.remaining_slice(0..3), Some("雪"));
        assert_eq!(cx.remaining_slice(3..4), Some("b"));
        assert_eq!(cx.source_slice(1..4), Some("雪"));
    }

    #[test]
    fn paired_scan_does_not_advance_but_consume_does() {
        let md = MarkdownIt::empty();
        let mut cx = InlinePluginState::new("{{a}}tail", &md);
        let options = PairOptions::new("{{", "}}");

        let matched = cx.scan_pair(&options).unwrap();
        assert_eq!(matched.body, 2..3);
        assert_eq!(cx.consumed(), 0);
        assert_eq!(cx.remaining_slice(matched.body), Some("a"));

        let matched = cx.consume_pair(&options).unwrap();
        assert_eq!(matched.consumed, 5);
        assert_eq!(cx.consumed(), 5);
        assert_eq!(cx.remaining(), "tail");
        assert_eq!(cx.consume_pair(&options), None);
    }
}
