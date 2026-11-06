use crate::document::{Document, NodeId};

/// How to proceed when an attempted inline target cannot be parsed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum InlineTargetFallback {
    /// Try resolving the label as a reference.
    #[default]
    Reference,
    /// Reject this rule's candidate without trying a reference.
    Reject,
}

/// Nesting context used to parse the label's inline children.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum LabelContext {
    /// Inherit the current depth and increment the link nesting level.
    #[default]
    NestedLink,
    /// Parse with a fresh depth and link nesting budget.
    Independent,
}

/// Parsing behavior independent of the marker and the constructed node type.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct LinkOptions {
    pub(crate) inline_target_fallback: InlineTargetFallback,
    pub(crate) label_context: LabelContext,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct LinkCfg<const PREFIX: char> {
    pub(super) factory: fn(&mut Document, Option<String>, Option<String>) -> NodeId,
    pub(super) options: LinkOptions,
}

/// Marks a Markdown link label, which core linkify must skip regardless of
/// inline anchor counter changes from HTML tags.
#[derive(Debug)]
pub(crate) struct MarkdownLinkLabel;

/// Original content of a label parsed in its own inline context (image alt).
#[derive(Debug)]
pub(crate) struct IndependentLabelSource(pub String);
