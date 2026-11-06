use super::cache::has_possible_link_label_close;
use super::config::{
    IndependentLabelSource,
    InlineTargetFallback,
    LabelContext,
    LinkOptions,
    MarkdownLinkLabel,
};
use super::label::scan_link_label;
use super::reference::{ParseLinkResult, resolve_reference_link};
use super::target::{LinkTargetScan, scan_link_target};
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
    references: Option<&ReferenceMap>,
    fallback: InlineTargetFallback,
) -> Option<ParseLinkResult> {
    if !has_possible_link_label_close(context, pos, references) {
        return None;
    }

    let label_start = pos + 1;
    let label_end = {
        let source = context.remaining();
        label_start
            + scan_link_label(
                context.child_state(label_start..source.len())?,
                enable_nested,
            )?
    };

    let target = {
        let source = context.remaining();
        scan_link_target(
            context.markdown_it(),
            source,
            label_end + 1,
            source.len(),
            fallback,
        )
    };

    let (href, title, end) = match target {
        LinkTargetScan::Inline(target) => (target.href, target.title, target.end),
        LinkTargetScan::Reject => return None,
        LinkTargetScan::Reference {
            resume: reference_pos,
        } => {
            let reference_end = {
                let source = context.remaining();
                if source.as_bytes().get(reference_pos) == Some(&b'[') {
                    let reference_start = reference_pos + 1;
                    context
                        .child_state(reference_start..source.len())
                        .and_then(|child| scan_link_label(child, true))
                        .map(|close| (reference_start, reference_start + close))
                } else {
                    None
                }
            };

            let source = context.remaining();
            let reference =
                resolve_reference_link(source, label_start, label_end, reference_end, references?)?;
            (reference.href, reference.title, reference.end)
        }
    };

    Some(ParseLinkResult {
        label_start,
        label_end,
        href,
        title,
        end,
    })
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
