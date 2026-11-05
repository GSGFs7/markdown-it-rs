use super::BlockState;
use crate::document::NodeId;

/// Check the debug-only rule invariants; run length is validated by the
/// always-on progress check in [`BlockState::tokenize`].
#[inline]
pub(super) fn run_block(
    state: &mut BlockState<'_>,
    index: usize,
    run: impl FnOnce(&mut BlockState<'_>) -> Option<(Option<NodeId>, usize)>,
) -> Option<(Option<NodeId>, usize)> {
    let before = (state.line, state.line_max);
    let result = run(state);
    assert_eq!(
        state.line_max, before.1,
        "block rule {index} changed the source window"
    );
    if result.is_none() {
        assert_eq!(
            state.line, before.0,
            "block rule {index} changed cursor on no match"
        );
    }
    result
}

#[inline]
pub(super) fn check_block(
    state: &mut BlockState<'_>,
    _index: usize,
    check: fn(&mut BlockState<'_>) -> Option<()>,
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

#[derive(Debug, PartialEq)]
struct BlockCheckSnapshot {
    cursor: (usize, usize, usize, Option<u32>, u32, bool),
    current_line: Option<(usize, usize, usize, i32)>,
    node_name: &'static str,
    arena_len: usize,
    child_count: usize,
    attrs: crate::document::HtmlAttributes,
    extension_count: usize,
    #[cfg(feature = "sourcemap")]
    srcmap: Option<crate::common::sourcemap::SourcePos>,
}

impl BlockCheckSnapshot {
    fn capture(state: &BlockState<'_>) -> Self {
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
            #[cfg(feature = "sourcemap")]
            srcmap: state.document.node(state.node).srcmap(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MarkdownIt;
    use crate::document::Root;

    #[test]
    #[should_panic(expected = "check modified cursor, indentation or accumulated nodes")]
    fn block_check_rejects_state_mutation_even_on_no_match() {
        let md = MarkdownIt::empty();
        let source = "hello";
        let mut document = crate::document::Document::new(source, Root::new(source));
        let mut state = BlockState::new(source, &md, &[], &mut document);
        check_block(&mut state, 0, |state| {
            state.blk_indent += 1;
            None
        });
    }
}
