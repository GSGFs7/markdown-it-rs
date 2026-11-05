use super::*;

#[test]
fn recursive_block_rules_switch_current_node_and_nesting_level() {
    use crate::document::{NodeId, Root};
    use crate::parser::block::BlockRule;

    #[derive(Debug)]
    struct Wrapper;

    #[derive(Debug, PartialEq, Eq)]
    struct ObservedLevel(u32);

    struct WrapperScanner;
    impl BlockRule for WrapperScanner {
        fn run(state: &mut BlockState<'_>) -> Option<(Option<NodeId>, usize)> {
            if !state.get_line(state.line).starts_with("%%") {
                return None;
            }

            let start = state.line;
            let old_node = std::mem::replace(&mut state.node, state.document.create_node(Wrapper));
            let old_line_max = state.line_max;
            state.line = start + 1;
            state.line_max = (start + 2).min(old_line_max);
            state.tokenize_nested();
            let end = state.line;
            state.line = start;
            state.line_max = old_line_max;
            let node = std::mem::replace(&mut state.node, old_node);
            Some((Some(node), end - start))
        }
    }

    struct LevelProbe;
    impl BlockRule for LevelProbe {
        fn run(state: &mut BlockState<'_>) -> Option<(Option<NodeId>, usize)> {
            if state.get_line(state.line) != "hello" {
                return None;
            }
            state.root_ext.insert(ObservedLevel(state.level));
            Some((None, 1))
        }
    }

    let md = MarkdownIt::empty();
    let source = "%%\nhello";
    let mut document = Document::new(source, Root::new(source));
    let mut state = BlockState::new(
        source,
        &md,
        &[
            (
                WrapperScanner::check as fn(&mut BlockState<'_>) -> Option<()>,
                WrapperScanner::run,
            ),
            (
                LevelProbe::check as fn(&mut BlockState<'_>) -> Option<()>,
                LevelProbe::run,
            ),
        ],
        &mut document,
    );
    state.tokenize();

    let BlockState {
        node: root,
        root_ext,
        document,
        ..
    } = state;
    let wrapper = document.node(document.children(root)[0]);
    assert!(wrapper.is::<Wrapper>());
    assert!(wrapper.children().is_empty());

    assert!(document.node(root).is::<Root>());
    assert_eq!(root_ext.get::<ObservedLevel>(), Some(&ObservedLevel(1)));
}

#[cfg(debug_assertions)]
#[test]
fn run_contract_rejects_invalid_consumption() {
    type Run = fn(&mut BlockState<'_>) -> Option<(Option<NodeId>, usize)>;
    let cases: &[(&str, Run, &str)] = &[
        ("zero", |_| Some((None, 0)), "invalid run length"),
        ("past window", |_| Some((None, 2)), "invalid run length"),
        (
            "overflow",
            |_| Some((None, usize::MAX)),
            "invalid run length",
        ),
        (
            "rewind without progress",
            |s| {
                s.line = 0;
                Some((None, 1))
            },
            "invalid run length",
        ),
        (
            "failed cursor",
            |s| {
                s.line += 1;
                None
            },
            "changed cursor on no match",
        ),
        (
            "success window",
            |s| {
                s.line_max += 1;
                Some((None, 1))
            },
            "changed the source window",
        ),
        (
            "failed window",
            |s| {
                s.line_max -= 1;
                None
            },
            "changed the source window",
        ),
    ];
    for &(name, run, expected) in cases {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let md = MarkdownIt::empty();
            let source = "a\nb\nc";
            let mut document = Document::new(source, crate::document::Root::new(source));
            let rules = [((|_: &mut BlockState<'_>| None) as _, run)];
            let mut state = BlockState::new(source, &md, &rules, &mut document);
            state.line = 1;
            state.line_max = 2;
            state.tokenize();
        }));
        let panic = result.expect_err(name);
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap_or("");
        assert!(message.contains(expected), "{name}: {message}");
    }
}

#[test]
fn run_contract_allows_rewind_with_progress_and_consume_only() {
    let md = MarkdownIt::empty();
    let source = "a\nb";
    let mut document = Document::new(source, crate::document::Root::new(source));
    let rules: [super::super::BlockRuleFns; 1] = [(
        |_| None,
        |s| {
            s.line = 0;
            Some((None, 2))
        },
    )];
    let mut state = BlockState::new(source, &md, &rules, &mut document);
    state.line = 1;
    state.tokenize();
    assert_eq!(state.line, 2);
    assert!(state.document.children(state.node).is_empty());
}
