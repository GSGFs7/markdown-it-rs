use super::*;

#[test]
fn recursive_block_rules_switch_current_node_and_nesting_level() {
    use crate::document::{NodeDraft, Root};
    use crate::parser::block::BlockRule;

    #[derive(Debug)]
    struct Wrapper;
    impl crate::document::NodeValue for Wrapper {}

    #[derive(Debug, PartialEq, Eq)]
    struct ObservedLevel(u32);

    struct WrapperScanner;
    impl BlockRule for WrapperScanner {
        fn run(state: &mut DocumentBlockState<'_>) -> Option<(NodeDraft, usize)> {
            if !state.get_line(state.line).starts_with("%%") {
                return None;
            }

            let start = state.line;
            let old_node = std::mem::replace(&mut state.node, NodeDraft::new(Wrapper));
            let old_line_max = state.line_max;
            state.line = start + 1;
            state.line_max = (start + 2).min(old_line_max);
            state.tokenize_nested();
            let end = state.line;
            state.line = start;
            state.line_max = old_line_max;
            let node = std::mem::replace(&mut state.node, old_node);
            Some((node, end - start))
        }
    }

    struct LevelProbe;
    impl BlockRule for LevelProbe {
        fn run(state: &mut DocumentBlockState<'_>) -> Option<(NodeDraft, usize)> {
            if state.get_line(state.line) != "hello" {
                return None;
            }
            state.root_ext.insert(ObservedLevel(state.level));
            Some((NodeDraft::placeholder(), 1))
        }
    }

    let md = MarkdownIt::empty();
    let source = "%%\nhello";
    let mut state = DocumentBlockState::new(
        source,
        &md,
        vec![
            (
                WrapperScanner::check as fn(&mut DocumentBlockState<'_>) -> Option<()>,
                WrapperScanner::run,
            ),
            (
                LevelProbe::check as fn(&mut DocumentBlockState<'_>) -> Option<()>,
                LevelProbe::run,
            ),
        ],
        NodeDraft::new(Root::new(source.to_owned())),
    );
    state.tokenize();

    let DocumentBlockState {
        node: root,
        root_ext,
        ..
    } = state;
    let wrapper = &root.children()[0];
    assert!(wrapper.is::<Wrapper>());
    assert!(wrapper.children().is_empty());

    assert!(root.is::<Root>());
    assert_eq!(root_ext.get::<ObservedLevel>(), Some(&ObservedLevel(1)));
}
