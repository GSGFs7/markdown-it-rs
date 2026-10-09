use super::block::DocumentBlockState;
use super::inline::DocumentInlineState;

#[inline]
pub(super) fn inline_range(source: &str, start: usize, end: usize) {
    debug_assert!(start <= end && end <= source.len());
    debug_assert!(source.is_char_boundary(start) && source.is_char_boundary(end));
}

#[inline]
pub(super) fn check_block(
    state: &mut DocumentBlockState<'_>,
    _index: usize,
    check: fn(&mut DocumentBlockState<'_>) -> Option<()>,
) -> Option<()> {
    let before = BlockCheckSnapshot::capture(state);
    let matched = check(state);
    assert_eq!(
        before,
        BlockCheckSnapshot::capture(state),
        "block rule {_index} check modified cursor, indentation or accumulated nodes"
    );
    matched
}

#[inline]
pub(super) fn check_inline(
    state: &mut DocumentInlineState<'_>,
    index: usize,
    check: fn(&mut DocumentInlineState<'_>) -> Option<usize>,
) -> Option<usize> {
    let before = InlineCheckSnapshot::capture(state);
    let matched = check(state);
    assert_eq!(
        before,
        InlineCheckSnapshot::capture(state),
        "inline rule {index} check modified cursor or accumulated nodes"
    );
    if let Some(len) = matched {
        assert!(
            len > 0 && state.remaining().get(..len).is_some(),
            "inline rule {index} reported invalid check length {len}"
        );
    }
    matched
}

#[derive(Debug, PartialEq)]
struct InlineCheckSnapshot {
    pos: usize,
    pos_max: usize,
    depth: u32,
    nodes: String,
    arena_len: usize,
}

impl InlineCheckSnapshot {
    fn capture(state: &DocumentInlineState<'_>) -> Self {
        Self {
            pos: state.pos,
            pos_max: state.pos_max,
            depth: state.depth,
            nodes: state
                .nodes()
                .iter()
                .map(|&id| format!("{:?}", state.document.events(id).collect::<Vec<_>>()))
                .collect(),
            arena_len: state.document.len(),
        }
    }
}

#[derive(Debug, PartialEq)]
struct BlockCheckSnapshot {
    cursor: (usize, usize, usize, Option<u32>, u32, bool),
    current_line: Option<(usize, usize, usize, i32)>,
    node_name: &'static str,
    arena_len: usize,
    child_count: usize,
    attrs: crate::HtmlAttributes,
    extension_count: usize,
    srcmap: Option<crate::common::sourcemap::SourcePos>,
}

impl BlockCheckSnapshot {
    fn capture(state: &DocumentBlockState<'_>) -> Self {
        Self {
            cursor: (
                state.line,
                state.line_max,
                state.blk_indent,
                state.list_indent,
                state.level,
                state.tight,
            ),
            current_line: state.line_offsets.get(state.line).map(|line| {
                (
                    line.line_start,
                    line.line_end,
                    line.first_nonspace,
                    line.indent_nonspace,
                )
            }),
            node_name: state.document.node(state.node).name(),
            arena_len: state.document.len(),
            child_count: state.document.node(state.node).children().len(),
            attrs: state.document.node(state.node).attrs().clone(),
            extension_count: state.document.node(state.node).ext().len(),
            srcmap: state.document.node(state.node).srcmap(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MarkdownIt, Root, Text};

    #[test]
    #[should_panic(expected = "check modified cursor or accumulated nodes")]
    fn inline_check_rejects_cursor_mutation_even_on_no_match() {
        let mut document = crate::Document::new("", crate::Root::new(""));
        let md = MarkdownIt::empty();
        let ruleset = md.inline.document_rules();
        let mut state = DocumentInlineState::new(&mut document, "xy", 0, 2, &md, &ruleset, 0, 0);
        check_inline(&mut state, 0, |state| {
            state.pos += 1;
            None
        });
    }

    #[test]
    #[should_panic(expected = "check modified cursor or accumulated nodes")]
    fn inline_check_rejects_node_mutation() {
        let mut document = crate::Document::new("", crate::Root::new(""));
        let md = MarkdownIt::empty();
        let ruleset = md.inline.document_rules();
        let mut state = DocumentInlineState::new(&mut document, "x", 0, 1, &md, &ruleset, 0, 0);
        check_inline(&mut state, 0, |state| {
            let node = state.document.create_node(Text {
                content: "bad".into(),
            });
            state.nodes_mut().push(node);
            Some(1)
        });
    }

    #[test]
    #[should_panic(expected = "check modified cursor or accumulated nodes")]
    fn inline_check_rejects_detached_allocation_on_no_match() {
        let mut document = crate::Document::new("", Root::new(""));
        let md = MarkdownIt::empty();
        let ruleset = md.inline.document_rules();
        let mut state = DocumentInlineState::new(&mut document, "x", 0, 1, &md, &ruleset, 0, 0);
        check_inline(&mut state, 0, |state| {
            state.document.create_node(Text {
                content: "leaked".into(),
            });
            None
        });
    }

    #[test]
    #[should_panic(expected = "check modified cursor or accumulated nodes")]
    fn inline_check_rejects_payload_mutation_through_id() {
        let mut document = crate::Document::new("", Root::new(""));
        let md = MarkdownIt::empty();
        let ruleset = md.inline.document_rules();
        let mut state = DocumentInlineState::new(&mut document, "x", 0, 1, &md, &ruleset, 0, 0);
        let text = state.document.create_node(Text {
            content: "kept".into(),
        });
        state.nodes_mut().push(text);
        check_inline(&mut state, 0, |state| {
            let text = state.nodes()[0];
            state
                .document
                .node_mut(text)
                .cast_mut::<Text>()
                .unwrap()
                .content = "changed".into();
            None
        });
    }

    #[test]
    #[should_panic(expected = "check modified cursor, indentation or accumulated nodes")]
    fn block_check_rejects_state_mutation_even_on_no_match() {
        let md = MarkdownIt::empty();
        let source = "hello";
        let mut state = DocumentBlockState::new(
            source,
            &md,
            vec![],
            crate::Document::new(source, Root::new(source)),
        );
        check_block(&mut state, 0, |state| {
            state.blk_indent += 1;
            None
        });
    }
}
