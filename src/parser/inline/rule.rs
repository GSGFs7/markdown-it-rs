use crate::document::NodeDraft;
use crate::parser::document_parser::DocumentInlineState;
use crate::parser::inline::probe::{InlineProbeContext, InlineProbeResult};

pub type DocumentFinalizeFn =
    for<'a> fn(&mut crate::parser::document_parser::DocumentInlineState<'a>);

/// An arena-backed inline parser rule.
pub trait InlineRule: 'static {
    /// First character that can activate this rule.
    ///
    /// Use `'\0'` for a wildcard rule that must be considered at every input
    /// position. A non-wildcard rule is only called when the current character
    /// matches this marker.
    const MARKER: char;
    const NAMES: &'static [&'static str] = &[];

    /// Classify the current position during an independent probe.
    ///
    /// `NoMatch` tries lower-priority rules, then character fallback. It does
    /// not imply that `run` would reject this position. Rules whose spans must
    /// remain opaque during boundary scanning should implement this method.
    /// Do not advance the cursor or mutate shared state.
    fn probe(_context: &mut InlineProbeContext<'_>) -> InlineProbeResult {
        InlineProbeResult::NoMatch
    }

    /// Inspect the current position and return a draft plus consumed byte length.
    ///
    /// The parser advances the state and assigns the draft's source map. Returning
    /// `None` declines the match; a successful match may omit a draft when it only
    /// updates parser state.
    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)>;
}

crate::parser::core::rule_builder!(InlineRule);
