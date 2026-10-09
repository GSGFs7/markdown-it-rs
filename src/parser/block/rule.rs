use crate::document::NodeDraft;
use crate::parser::block::DocumentBlockState;
use crate::parser::rule::rule_builder;

/// A block syntax rule operating on document drafts.
pub trait BlockRule: 'static {
    /// First characters that can activate this rule; empty means a wildcard
    /// rule, considered on every non-empty line. A non-empty list must include
    /// every possible first character after indentation, or the rule is skipped.
    const MARKERS: &'static [char] = &[];
    const NAMES: &'static [&'static str] = &[];

    fn check(state: &mut DocumentBlockState<'_>) -> Option<()> {
        Self::run(state).map(|_| ())
    }

    fn run(state: &mut DocumentBlockState<'_>) -> Option<(NodeDraft, usize)>;
}
rule_builder!(BlockRule);
