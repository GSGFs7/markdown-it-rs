use super::config::{
    IndependentLabelSource,
    InlineTargetFallback,
    LabelContext,
    LinkOptions,
    MarkdownLinkLabel,
};
use super::reference::ParseLinkResult;
use crate::document::{Document, NodeId};
use crate::parser::inline::InlineState;
use crate::plugins::cmark::block::reference::ReferenceMap;

/// Parse one `[label](...)` / `[label][reference]` candidate.
///
/// This is the shared syntax behind links, images, and prefixed variants. The
/// label is scanned first; the suffix then decides between an inline target and
/// a reference using the same fallback rules as markdown-it's link rule.
pub(super) fn scan_link_candidate(
    context: &mut InlineState<'_>,
    pos: usize,
    enable_nested: bool,
    _references: Option<&ReferenceMap>,
    fallback: InlineTargetFallback,
) -> Option<ParseLinkResult> {
    super::scan::scan_candidate(context, pos, enable_nested, fallback, _references)
}

pub(super) fn link_check(
    context: &mut InlineState<'_>,
    enable_nested: bool,
    offset: usize,
    fallback: InlineTargetFallback,
) -> Option<usize> {
    let references = context.root_ext.and_then(|ext| ext.get::<ReferenceMap>());
    scan_link_candidate(context, offset, enable_nested, references, fallback)
        .map(|candidate| candidate.end)
}

pub(super) fn link_run(
    state: &mut InlineState<'_>,
    enable_nested: bool,
    offset: usize,
    options: LinkOptions,
    factory: fn(&mut Document, Option<String>, Option<String>) -> NodeId,
) -> Option<(Option<NodeId>, usize)> {
    // Parses [link](<to> "stuff") and other link forms (inline target or
    // reference), then parses the label as inline children.
    let references = state.root_ext.and_then(|ext| ext.get::<ReferenceMap>());
    let candidate = scan_link_candidate(
        state,
        offset,
        enable_nested,
        references,
        options.inline_target_fallback,
    )?;
    // We found the end of the link and know for a fact it's a valid link;
    // all that's left to do is to parse the label contents as inline children.
    let label_range = candidate.label_start..candidate.label_end;
    // Parse the label directly (extensions survive) before allocating the node.
    let children = match options.label_context {
        LabelContext::Independent => state.parse_child_at(label_range.clone(), 0, 0, |label| {
            label
                .inline_ext
                .insert(super::super::delimiters::EmphasisSourceStart(label.pos));
        }),
        LabelContext::NestedLink => {
            let child_link_level = state
                .link_level
                .checked_add(1)
                .expect("inline link nesting level overflow");
            state.parse_child_at(
                label_range.clone(),
                state.depth.saturating_add(1),
                child_link_level,
                |label| {
                    label.inline_ext.insert(MarkdownLinkLabel);
                },
            )
        }
    }?;
    let label_source = (options.label_context == LabelContext::Independent)
        .then(|| IndependentLabelSource(state.remaining()[label_range].to_owned()));
    let node = factory(state.document, candidate.href, candidate.title);
    if let Some(source) = label_source {
        state.document.node_mut(node).ext_mut().insert(source);
    }
    state.document.attach_children(node, children);

    Some((Some(node), candidate.end))
}
