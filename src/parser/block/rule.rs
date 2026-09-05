use crate::Node;
use crate::parser::core::rule_builder;
use crate::parser::document::NodeDraft;
use crate::parser::document_parser::DocumentBlockState;

/// Each member of block rule chain must implement this trait
pub trait BlockRule: 'static {
    /// First characters that can activate this rule.
    ///
    /// Leave this empty for a wildcard rule that must be considered on every
    /// non-empty line. This is the default so existing third-party block rules
    /// remain compatible. A non-empty list must include every possible first
    /// character after block indentation, or the rule will be skipped.
    const MARKERS: &'static [char] = &[];
    const NAMES: &'static [&'static str] = &[];

    fn check(state: &mut super::BlockState) -> Option<()> {
        Self::run(state).map(|_| ())
    }

    fn run(state: &mut super::BlockState) -> Option<(Node, usize)>;
}

pub(crate) trait DocumentBlockRule: 'static {
    fn check(state: &mut DocumentBlockState<'_>) -> Option<()> {
        Self::run(state).map(|_| ())
    }

    fn run(state: &mut DocumentBlockState<'_>) -> Option<(NodeDraft, usize)>;
}

rule_builder!(BlockRule);
