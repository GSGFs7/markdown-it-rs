use crate::parser::core::rule_builder;
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

rule_builder!(InlineRule);
