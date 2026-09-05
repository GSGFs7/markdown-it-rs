use crate::parser::document::NodeDraft;
use crate::parser::document_parser::DocumentInlineState;
use crate::parser::node::Node;

/// An arena-backed inline parser rule.
pub trait InlineRule: 'static {
    /// First character that can activate this rule.
    ///
    /// Use `'\0'` for a wildcard rule that must be considered at every input
    /// position. A non-wildcard rule is only called when the current character
    /// matches this marker.
    const MARKER: char;
    const NAMES: &'static [&'static str] = &[];

    /// Inspect the current position and return a draft plus consumed byte length.
    ///
    /// The parser advances the state and assigns the draft's source map. Returning
    /// `None` declines the match; a successful match may omit a draft when it only
    /// updates parser state.
    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)>;
}

pub(crate) trait LegacyInlineRule: 'static {
    const MARKER: char;
    const NAMES: &'static [&'static str] = &[];

    fn check(state: &mut super::InlineState) -> Option<usize> {
        Self::run(state).map(|(_node, len)| len)
    }

    fn run(state: &mut super::InlineState) -> Option<(Node, usize)>;
}

crate::parser::core::rule_builder!(InlineRule);

#[allow(dead_code)]
mod legacy_builder {
    use super::LegacyInlineRule;

    crate::parser::core::rule_builder!(LegacyInlineRule);
}

pub(crate) use legacy_builder::RuleBuilder as LegacyRuleBuilder;
