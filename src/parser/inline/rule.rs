use crate::parser::core::rule_builder;
use crate::parser::document::NodeDraft;
use crate::parser::document_parser::DocumentInlineState;
use crate::parser::node::Node;

/// Each member of inline rule chain must implement this trait
pub trait InlineRule: 'static {
    /// First character that can activate this rule.
    ///
    /// Use `'\0'` for a wildcard rule that must be considered at every input
    /// position. A non-wildcard rule is only called when the current character
    /// matches this marker.
    const MARKER: char;
    const NAMES: &'static [&'static str] = &[];

    fn check(state: &mut super::InlineState) -> Option<usize> {
        Self::run(state).map(|(_node, len)| len)
    }

    fn run(state: &mut super::InlineState) -> Option<(Node, usize)>;
}

pub(crate) trait DocumentInlineRule: 'static {
    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)>;
}

rule_builder!(InlineRule);
