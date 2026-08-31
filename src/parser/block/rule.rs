use crate::Node;
use crate::parser::core::rule_builder;

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

rule_builder!(BlockRule);
