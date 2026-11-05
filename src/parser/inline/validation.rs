use super::InlineState;
use crate::document::NodeId;

/// Check the debug-only rule invariants; run length is validated by the
/// always-on progress check in [`InlineState::tokenize`].
#[inline]
pub(super) fn run_inline(
    state: &mut InlineState<'_>,
    index: usize,
    run: impl FnOnce(&mut InlineState<'_>) -> Option<(Option<NodeId>, usize)>,
) -> Option<(Option<NodeId>, usize)> {
    let before = (state.pos, state.pos_max);
    let result = run(state);
    assert_eq!(
        state.pos_max, before.1,
        "inline rule {index} changed the source window"
    );
    if result.is_none() {
        assert_eq!(
            state.pos, before.0,
            "inline rule {index} changed cursor on no match"
        );
    }
    result
}

#[inline]
pub(super) fn check_inline(
    state: &mut InlineState<'_>,
    index: usize,
    check: impl FnOnce(&mut InlineState<'_>) -> Option<usize>,
) -> Option<usize> {
    let before = InlineCheckSnapshot::capture(state);
    let matched = check(state);
    assert_eq!(
        before,
        InlineCheckSnapshot::capture(state),
        "inline rule {index} check modified cursor or accumulated nodes"
    );
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
    fn capture(state: &InlineState<'_>) -> Self {
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

#[inline]
pub(super) fn inline_range(source: &str, start: usize, end: usize) {
    debug_assert!(start <= end && end <= source.len());
    debug_assert!(source.is_char_boundary(start) && source.is_char_boundary(end));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MarkdownIt;
    use crate::document::{Root, Text};

    #[test]
    #[should_panic(expected = "check modified cursor or accumulated nodes")]
    fn inline_check_rejects_cursor_mutation_even_on_no_match() {
        let mut document = crate::document::Document::new("", Root::new(""));
        let md = MarkdownIt::empty();
        let ruleset = md.inline.rules();
        let mut state = InlineState::new(&mut document, "xy", 0, 2, &md, ruleset, 0, 0);
        check_inline(&mut state, 0, |state| {
            state.pos += 1;
            None
        });
    }

    #[test]
    #[should_panic(expected = "check modified cursor or accumulated nodes")]
    fn inline_check_rejects_node_mutation() {
        let mut document = crate::document::Document::new("", Root::new(""));
        let md = MarkdownIt::empty();
        let ruleset = md.inline.rules();
        let mut state = InlineState::new(&mut document, "x", 0, 1, &md, ruleset, 0, 0);
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
        let mut document = crate::document::Document::new("", Root::new(""));
        let md = MarkdownIt::empty();
        let ruleset = md.inline.rules();
        let mut state = InlineState::new(&mut document, "x", 0, 1, &md, ruleset, 0, 0);
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
        let mut document = crate::document::Document::new("", Root::new(""));
        let md = MarkdownIt::empty();
        let ruleset = md.inline.rules();
        let mut state = InlineState::new(&mut document, "x", 0, 1, &md, ruleset, 0, 0);
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
}
