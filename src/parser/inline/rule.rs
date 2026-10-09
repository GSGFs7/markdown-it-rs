use crate::document::NodeId;
use crate::parser::inline::DocumentInlineState;

pub type DocumentFinalizeFn = for<'a> fn(&mut crate::parser::inline::DocumentInlineState<'a>);

/// An inline-level syntax rule.
///
/// Implement this trait to add a new inline construct (emphasis, code span,
/// link, ...) to an [`InlineParser`](crate::parser::inline::InlineParser).
/// Register it with
/// [`InlineParser::add_rule`](crate::parser::inline::InlineParser::add_rule)
/// (or `add_rule_with_finalize`), then use the returned [`RuleBuilder`] to
/// position it relative to the built-in rules.
///
/// Implement [`run`](InlineRule::run) to match the construct and add nodes to
/// the document. Override [`check`](InlineRule::check) only if the default does
/// not fit.
pub trait InlineRule: 'static {
    /// Character that may begin this construct.
    ///
    /// Use `'\0'` to be considered at every input position. Otherwise the rule
    /// is only considered when the current character matches.
    const MARKER: char;
    /// Extra names identifying this rule, so it can be referenced by name when
    /// ordering rules, e.g. `after_named("emphasis")`.
    const NAMES: &'static [&'static str] = &[];

    /// Probes the current position while scanning text, returning the number of
    /// bytes this construct consumes when it starts here.
    ///
    /// The default reuses [`run`](InlineRule::run) and discards its node, which
    /// is fine for simple matches; override it when `run` has side effects or
    /// when matching conditions differ.
    fn check(state: &mut DocumentInlineState<'_>) -> Option<usize> {
        let (node, len) = Self::run(state)?;
        if let Some(node) = node {
            state.document.discard_node(node);
        }
        Some(len)
    }

    /// Tries to match this construct at the current position.
    ///
    /// Returns the node to insert (if any) and the number of UTF-8 bytes
    /// consumed. Return `Some((None, n))` to consume `n` bytes without adding a
    /// node, or `None` if the construct does not match.
    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)>;
}

crate::parser::rule::rule_builder!(InlineRule);
