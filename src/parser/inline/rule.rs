use crate::document::NodeDraft;
use crate::parser::inline::DocumentInlineState;

pub type DocumentFinalizeFn = for<'a> fn(&mut crate::parser::inline::DocumentInlineState<'a>);

/// An arena-backed inline parser rule.
pub trait InlineRule: 'static {
    /// First character that can activate this rule.
    ///
    /// Use `'\0'` for a wildcard rule that must be considered at every input
    /// position. A non-wildcard rule is only called when the current character
    /// matches this marker.
    const MARKER: char;
    const NAMES: &'static [&'static str] = &[];

    /// Check the current position and return its consumed UTF-8 byte length.
    ///
    /// Must not advance the cursor or modify accumulated nodes; the parser
    /// validates this in debug builds. The default calls `run` and discards its
    /// draft, so override it when `run` has side effects or when matching
    /// conditions differ.
    fn check(state: &mut DocumentInlineState<'_>) -> Option<usize> {
        Self::run(state).map(|(_, len)| len)
    }

    /// Inspect the current position and return a draft plus consumed byte length.
    ///
    /// The parser advances the state and assigns the draft's source map. Returning
    /// `None` declines the match; a successful match may omit a draft when it only
    /// updates parser state.
    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)>;
}

crate::parser::rule::rule_builder!(InlineRule);
