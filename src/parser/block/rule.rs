use crate::document::NodeId;
use crate::parser::block::DocumentBlockState;
use crate::parser::rule::rule_builder;

/// A block-level syntax rule.
///
/// Implement this trait to add a new block construct (heading, fence, list,
/// ...) to a [`BlockParser`](crate::parser::block::BlockParser). Register it
/// with [`BlockParser::add_rule`](crate::parser::block::BlockParser::add_rule),
/// then use the returned [`RuleBuilder`] to position it relative to the
/// built-in rules.
///
/// Implement [`run`](BlockRule::run) to parse the construct and add nodes to
/// the document. Override [`check`](BlockRule::check) only if the default does
/// not fit.
pub trait BlockRule: 'static {
    /// First characters that may begin this construct.
    ///
    /// Leave empty to be considered on every non-empty line. Otherwise list
    /// every possible first character after indentation, or the rule is skipped.
    const MARKERS: &'static [char] = &[];
    /// Extra names identifying this rule, so it can be referenced by name when
    /// ordering rules, e.g. `before_named("fence")`.
    const NAMES: &'static [&'static str] = &[];

    /// Whether this construct interrupts the current block continuation, such
    /// as a paragraph.
    ///
    /// The default reuses [`run`](BlockRule::run) and discards its node, which
    /// is fine for pass-through constructs; override it when `run` has side
    /// effects or a match cannot interrupt a paragraph.
    fn check(state: &mut DocumentBlockState<'_>) -> Option<()> {
        let (node, _) = Self::run(state)?;
        if let Some(node) = node {
            state.document.discard_node(node);
        }
        Some(())
    }

    /// Parses this construct starting at the current line.
    ///
    /// Returns the node to insert (if any) and the number of lines consumed.
    /// Return `Some((None, n))` to consume `n` lines without adding a node, or
    /// `None` if the line does not match, in which case the parser position is
    /// left unchanged.
    fn run(state: &mut DocumentBlockState<'_>) -> Option<(Option<NodeId>, usize)>;
}

rule_builder!(BlockRule);
