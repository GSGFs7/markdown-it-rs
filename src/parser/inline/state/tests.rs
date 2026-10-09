use std::sync::{Arc, Mutex};

use super::*;
use crate::links::LinkFormatter;
use crate::parser::inline::{InlineCheckFn, InlineRule, InlineRuleFn, InlineRuleFns};

impl<'a> DocumentInlineState<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        document: &'a mut Document,
        source: &'a str,
        start: usize,
        end: usize,
        md: &'a MarkdownIt,
        ruleset: &'a DocumentRuleSet,
        depth: u32,
        link_level: i32,
    ) -> Self {
        #[cfg(debug_assertions)]
        crate::parser::validation::inline_range(source, start, end);
        Self {
            src: Cow::Borrowed(source),
            pos: start,
            pos_max: end,
            md,
            mapping: Cow::Borrowed(&[(0, 0)]),
            depth,
            inline_ext: InlineRootExtSet::new(),
            root_ext: None,
            link_level,
            ruleset,
            nodes: Vec::new(),
            document,
            pending_text: None,
        }
    }
}

// Test shorthand for a nested boundary scan; production uses child_state directly.
fn scan<'s>(
    state: &'s mut DocumentInlineState<'_>,
    range: Range<usize>,
) -> Option<DocumentInlineState<'s>> {
    state.child_state(range, state.depth.checked_add(1)?, state.link_level)
}

fn check_rule(check: InlineCheckFn) -> InlineRuleFns {
    InlineRuleFns {
        type_id: std::any::TypeId::of::<()>(),
        run: panic_run,
        check,
        marker: '\0',
    }
}

fn run_rule_with_marker(run: InlineRuleFn, marker: char) -> InlineRuleFns {
    InlineRuleFns {
        type_id: std::any::TypeId::of::<()>(),
        run,
        check: panic_check,
        marker,
    }
}

fn run_rule(run: InlineRuleFn) -> InlineRuleFns {
    run_rule_with_marker(run, '\0')
}

fn panic_check(_: &mut DocumentInlineState<'_>) -> Option<usize> {
    unreachable!("rule check must not be called")
}

fn panic_run(_: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
    unreachable!("rule run must not be called")
}

#[derive(Debug, Default)]
struct FinalizerCalls(usize);

fn count_finalizer(state: &mut DocumentInlineState<'_>) {
    assert!(state.pending_text.is_none());
    state.inline_ext.get_or_insert_default::<FinalizerCalls>().0 += 1;
}

fn parent_state<'a>(
    document: &'a mut Document,
    md: &'a MarkdownIt,
    ruleset: &'a DocumentRuleSet,
) -> DocumentInlineState<'a> {
    let mut inline_ext = InlineRootExtSet::new();
    inline_ext.insert(FinalizerCalls(41));
    DocumentInlineState {
        src: Cow::Owned("前{ 雪\n次 }尾".to_owned()),
        pos: 3,
        pos_max: 14,
        md,
        mapping: Cow::Owned(vec![(0, 10), (9, 30)]),
        depth: 0,
        inline_ext,
        root_ext: None,
        link_level: 2,
        ruleset,
        nodes: vec![document.create_node(Text {
            content: "sentinel".into(),
        })],
        document,
        pending_text: Some((0, 3)),
    }
}

/// Inline state with a small source and no pending text, for check tests.
fn check_state<'a>(
    document: &'a mut Document,
    md: &'a MarkdownIt,
    ruleset: &'a DocumentRuleSet,
    source: &str,
) -> DocumentInlineState<'a> {
    DocumentInlineState {
        src: Cow::Owned(source.to_owned()),
        pos: 0,
        pos_max: source.len(),
        md,
        mapping: Cow::Owned(vec![(0, 0)]),
        depth: 0,
        inline_ext: InlineRootExtSet::new(),
        root_ext: None,
        link_level: 0,
        ruleset,
        nodes: vec![],
        document,
        pending_text: None,
    }
}

#[derive(Debug, PartialEq, Eq)]
struct CheckParentSnapshot {
    cursor: (usize, usize, u32, i32),
    pending_text: Option<(usize, usize)>,
    source: String,
    mapping: Vec<(usize, usize)>,
    inline_ext: String,
    root_ext: String,
    nodes: String,
}

fn check_parent_snapshot(state: &DocumentInlineState<'_>) -> CheckParentSnapshot {
    CheckParentSnapshot {
        cursor: (state.pos, state.pos_max, state.depth, state.link_level),
        pending_text: state.pending_text,
        source: state.src.to_string(),
        mapping: state.mapping.to_vec(),
        inline_ext: format!("{:?}", state.inline_ext),
        root_ext: format!("{:?}", state.root_ext),
        nodes: state
            .nodes
            .iter()
            .map(|&id| format!("{:?}", state.document.events(id).collect::<Vec<_>>()))
            .collect(),
    }
}

#[test]
fn finishing_nodes_preserves_source_and_byte_mapping() {
    let mut document = Document::new("", crate::Root::new(""));
    let md = MarkdownIt::empty();
    let ruleset = DocumentRuleSet {
        runs: vec![],
        checks: vec![],
        finalizers: vec![count_finalizer],
    };
    let mut state = DocumentInlineState {
        src: Cow::Owned("前x雪尾".to_owned()),
        pos: 7,
        pos_max: 7,
        md: &md,
        mapping: Cow::Owned(vec![(0, 10)]),
        depth: 0,
        inline_ext: InlineRootExtSet::new(),
        root_ext: None,
        link_level: 1,
        ruleset: &ruleset,
        nodes: vec![document.create_node(Text {
            content: "x".to_owned(),
        })],
        document: &mut document,
        pending_text: Some((4, 7)),
    };

    state.finish_nodes();

    assert_eq!(state.src, "前x雪尾");
    assert_eq!(state.mapping.as_ref(), &[(0, 10)]);
    assert_eq!((state.pos, state.pos_max, state.link_level), (7, 7, 1));
    assert_eq!(state.nodes.len(), 2);
    assert_eq!(
        state
            .document
            .node(state.nodes[1])
            .cast::<Text>()
            .unwrap()
            .content,
        "雪"
    );
    assert_eq!(
        state.document.node(state.nodes[1]).srcmap(),
        Some(SourcePos::new(14, 17))
    );
    assert_eq!(state.inline_ext.get::<FinalizerCalls>().unwrap().0, 1);
}

#[test]
fn top_level_plain_text_still_skips_finalizers() {
    let mut document = Document::new("", crate::Root::new(""));
    let md = MarkdownIt::empty();
    let ruleset = DocumentRuleSet {
        runs: vec![],
        checks: vec![],
        finalizers: vec![|_| panic!("plain pending text must skip finalizers")],
    };
    let nodes = DocumentInlineState::parse(
        &mut document,
        " plain ".to_owned(),
        vec![(0, 0)],
        &md,
        &ruleset,
        None,
    );
    assert_eq!(nodes.len(), 1);
    assert_eq!(
        document.node(nodes[0]).cast::<Text>().unwrap().content,
        "plain"
    );
    assert_eq!(document.node(nodes[0]).srcmap(), Some(SourcePos::new(1, 6)));
}

#[test]
fn subrange_preserves_parent_whitespace_and_multiline_mapping() {
    let mut document = Document::new("", crate::Root::new(""));
    let md = MarkdownIt::empty();
    let ruleset = DocumentRuleSet {
        runs: vec![],
        checks: vec![],
        finalizers: vec![count_finalizer],
    };
    let mut state = parent_state(&mut document, &md, &ruleset);
    let nodes = state.parse_subrange(1..10).unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(
        state
            .document
            .node(nodes[0])
            .cast::<Text>()
            .unwrap()
            .content,
        " 雪\n次 "
    );
    assert_eq!(
        state.document.node(nodes[0]).srcmap(),
        Some(SourcePos::new(14, 34))
    );
    assert_eq!(state.src, "前{ 雪\n次 }尾");
    assert_eq!(state.mapping.as_ref(), &[(0, 10), (9, 30)]);
    assert_eq!(
        (state.pos, state.pos_max, state.depth, state.link_level),
        (3, 14, 0, 2)
    );
    assert_eq!(state.pending_text, Some((0, 3)));
    assert_eq!(state.nodes.len(), 1);
    assert_eq!(
        state
            .document
            .node(state.nodes[0])
            .cast::<Text>()
            .unwrap()
            .content,
        "sentinel"
    );
    assert_eq!(state.inline_ext.get::<FinalizerCalls>().unwrap().0, 41);
}

#[test]
fn subrange_validates_ranges_and_empty_output() {
    let mut document = Document::new("", crate::Root::new(""));
    let md = MarkdownIt::empty();
    let ruleset = DocumentRuleSet {
        runs: vec![],
        checks: vec![],
        finalizers: vec![|_| panic!("empty/plain range")],
    };
    let mut state = parent_state(&mut document, &md, &ruleset);
    for range in [0..0, 11..11] {
        assert!(state.parse_subrange(range).unwrap().is_empty());
    }
    let reversed = Range { start: 2, end: 1 };
    for range in [reversed, 0..12, 0..usize::MAX, 3..4, 3..3] {
        assert!(state.parse_subrange(range).is_none());
    }
    assert_eq!(state.pending_text, Some((0, 3)));
    assert_eq!(state.inline_ext.get::<FinalizerCalls>().unwrap().0, 41);
}

#[test]
fn child_finalizer_sees_only_child_nodes_and_scratch() {
    let mut document = Document::new("", crate::Root::new(""));
    fn emit(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        assert_eq!(state.depth, 1);
        assert_eq!(state.link_level, 2);
        assert!(state.inline_ext.get::<FinalizerCalls>().is_none());
        state.inline_ext.insert(FinalizerCalls(0));
        let content = state.remaining().to_owned();
        Some((
            Some(state.document.create_node(Text { content })),
            state.remaining().len(),
        ))
    }
    fn finalize(state: &mut DocumentInlineState<'_>) {
        assert!(state.pending_text.is_none());
        assert_eq!(state.nodes.len(), 1);
        assert_eq!(state.inline_ext.get::<FinalizerCalls>().unwrap().0, 0);
        state.inline_ext.get_mut::<FinalizerCalls>().unwrap().0 += 1;
        state
            .document
            .node_mut(state.nodes[0])
            .cast_mut::<Text>()
            .unwrap()
            .content
            .push('!');
        state.link_level = 99;
    }
    let md = MarkdownIt::empty();
    let ruleset = DocumentRuleSet {
        runs: vec![run_rule(emit)],
        checks: vec![],
        finalizers: vec![finalize],
    };
    let mut state = parent_state(&mut document, &md, &ruleset);
    for _ in 0..2 {
        let nodes = state.parse_subrange(1..10).unwrap();
        assert_eq!(
            state
                .document
                .node(nodes[0])
                .cast::<Text>()
                .unwrap()
                .content,
            " 雪\n次 !"
        );
    }
    assert_eq!(state.inline_ext.get::<FinalizerCalls>().unwrap().0, 41);
    assert_eq!(state.link_level, 2);
}

#[test]
fn subrange_at_nesting_limit_emits_literal_text() {
    let mut document = Document::new("", crate::Root::new(""));
    let mut md = MarkdownIt::empty();
    md.max_nesting = 1;
    let ruleset = DocumentRuleSet {
        runs: vec![run_rule(|_| panic!("nesting limit must skip rules"))],
        checks: vec![],
        finalizers: vec![|_| panic!("literal text must skip finalizers")],
    };
    let mut state = parent_state(&mut document, &md, &ruleset);
    let nodes = state.parse_subrange(0..11).unwrap();
    assert_eq!(
        state
            .document
            .node(nodes[0])
            .cast::<Text>()
            .unwrap()
            .content,
        "{ 雪\n次 }"
    );
}

#[test]
fn normal_parse_does_not_call_check() {
    let mut document = Document::new("", crate::Root::new(""));
    fn emit(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        let content = state.remaining().to_owned();
        Some((
            Some(state.document.create_node(Text { content })),
            state.remaining().len(),
        ))
    }
    let md = MarkdownIt::empty();
    let ruleset = DocumentRuleSet {
        runs: vec![run_rule(emit)],
        checks: vec![check_rule(|_| panic!("normal parsing must not call check"))],
        finalizers: vec![],
    };
    let nodes = DocumentInlineState::parse(
        &mut document,
        "x".to_owned(),
        vec![(0, 0)],
        &md,
        &ruleset,
        None,
    );
    assert_eq!(nodes.len(), 1);
    assert_eq!(document.node(nodes[0]).cast::<Text>().unwrap().content, "x");
}

#[test]
fn run_dispatch_skips_non_matching_markers() {
    let mut document = Document::new("", crate::Root::new(""));
    fn panic_run_with(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        panic!(
            "rule for another marker must not run on {:?}",
            state.remaining()
        )
    }
    fn consume_all(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        let content = state.remaining().to_owned();
        Some((
            Some(state.document.create_node(Text { content })),
            state.remaining().len(),
        ))
    }

    let md = MarkdownIt::empty();
    let ruleset = DocumentRuleSet {
        runs: vec![
            run_rule_with_marker(panic_run_with, '!'),
            run_rule(consume_all),
        ],
        checks: vec![],
        finalizers: vec![],
    };

    for source in ["abc", "雪x"] {
        let nodes = DocumentInlineState::parse(
            &mut document,
            source.to_owned(),
            vec![(0, 0)],
            &md,
            &ruleset,
            None,
        );
        assert_eq!(nodes.len(), 1);
        assert_eq!(
            document.node(nodes[0]).cast::<Text>().unwrap().content,
            source
        );
    }
}

#[test]
fn run_dispatch_matches_unicode_marker() {
    let mut document = Document::new("", crate::Root::new(""));
    fn consume_snow(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        assert!(state.remaining().starts_with('雪'));
        Some((
            Some(state.document.create_node(Text {
                content: "雪".to_owned(),
            })),
            '雪'.len_utf8(),
        ))
    }
    fn consume_all(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        let content = state.remaining().to_owned();
        Some((
            Some(state.document.create_node(Text { content })),
            state.remaining().len(),
        ))
    }

    let md = MarkdownIt::empty();
    let ruleset = DocumentRuleSet {
        runs: vec![
            run_rule_with_marker(consume_snow, '雪'),
            run_rule(consume_all),
        ],
        checks: vec![],
        finalizers: vec![],
    };
    let nodes = DocumentInlineState::parse(
        &mut document,
        "雪x".to_owned(),
        vec![(0, 0)],
        &md,
        &ruleset,
        None,
    );
    assert_eq!(nodes.len(), 2);
    assert_eq!(
        document.node(nodes[0]).cast::<Text>().unwrap().content,
        "雪"
    );
    assert_eq!(document.node(nodes[1]).cast::<Text>().unwrap().content, "x");

    let nodes = DocumentInlineState::parse(
        &mut document,
        "xx".to_owned(),
        vec![(0, 0)],
        &md,
        &ruleset,
        None,
    );
    assert_eq!(nodes.len(), 1);
    assert_eq!(
        document.node(nodes[0]).cast::<Text>().unwrap().content,
        "xx"
    );
}

#[test]
fn run_dispatch_preserves_wildcard_order() {
    let mut document = Document::new("", crate::Root::new(""));
    fn consume_x(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        if !state.remaining().starts_with('x') {
            return None;
        }
        Some((
            Some(state.document.create_node(Text {
                content: "X".to_owned(),
            })),
            1,
        ))
    }
    fn consume_all(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        let content = state.remaining().to_owned();
        Some((
            Some(state.document.create_node(Text { content })),
            state.remaining().len(),
        ))
    }
    fn panic_run(_: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        panic!("later rule must not run after an earlier wildcard match")
    }

    let md = MarkdownIt::empty();
    // The marker rule is registered first and must win over the wildcard.
    let marker_first = DocumentRuleSet {
        runs: vec![run_rule_with_marker(consume_x, 'x'), run_rule(consume_all)],
        checks: vec![],
        finalizers: vec![],
    };
    let nodes = DocumentInlineState::parse(
        &mut document,
        "xy".to_owned(),
        vec![(0, 0)],
        &md,
        &marker_first,
        None,
    );
    assert_eq!(nodes.len(), 2);
    assert_eq!(document.node(nodes[0]).cast::<Text>().unwrap().content, "X");
    assert_eq!(document.node(nodes[1]).cast::<Text>().unwrap().content, "y");

    // A wildcard registered first must keep its position and win.
    let wildcard_first = DocumentRuleSet {
        runs: vec![run_rule(consume_all), run_rule_with_marker(panic_run, 'x')],
        checks: vec![],
        finalizers: vec![],
    };
    let nodes = DocumentInlineState::parse(
        &mut document,
        "xy".to_owned(),
        vec![(0, 0)],
        &md,
        &wildcard_first,
        None,
    );
    assert_eq!(nodes.len(), 1);
    assert_eq!(
        document.node(nodes[0]).cast::<Text>().unwrap().content,
        "xy"
    );
}

#[test]
fn check_advances_pending_text_between_tokens() {
    let mut document = Document::new("", crate::Root::new(""));
    fn check_sequence(context: &mut DocumentInlineState<'_>) -> Option<usize> {
        let remaining = context.remaining();
        if remaining.starts_with('a') {
            None
        } else if remaining.starts_with('b') {
            assert_eq!(context.trailing_text(), "a");
            Some(1)
        } else if remaining.starts_with('c') {
            assert_eq!(context.trailing_text(), "");
            context.push_text(context.pos, context.pos + 1);
            Some(1)
        } else {
            None
        }
    }
    let md = MarkdownIt::empty();
    let ruleset = DocumentRuleSet {
        runs: vec![],
        checks: vec![check_rule(check_sequence)],
        finalizers: vec![],
    };
    let mut state = check_state(&mut document, &md, &ruleset, "abc");
    let mut context = scan(&mut state, 0..3).unwrap();

    assert_eq!(context.skip_token().unwrap(), 1);
    assert_eq!(context.skip_token().unwrap(), 1);
    assert_eq!(context.skip_token().unwrap(), 1);
    assert_eq!(context.skip_token(), None);
    assert_eq!(context.trailing_text(), "c");

    assert_eq!(state.pending_text, None);
    assert_eq!((state.pos, state.pos_max, state.link_level), (0, 3, 0));
}

#[test]
fn check_stops_at_nesting_limit() {
    let mut document = Document::new("", crate::Root::new(""));
    let mut md = MarkdownIt::empty();
    md.max_nesting = 1;
    let ruleset = DocumentRuleSet {
        runs: vec![],
        checks: vec![check_rule(|_| panic!("nesting limit must skip checks"))],
        finalizers: vec![],
    };
    let mut state = check_state(&mut document, &md, &ruleset, "abc");
    let mut context = scan(&mut state, 0..3).unwrap();

    assert_eq!(context.skip_token().unwrap(), 3);
    assert_eq!(context.skip_token(), None);
}

#[test]
fn check_no_match_falls_through_to_lower_priority_rules() {
    let mut document = Document::new("", crate::Root::new(""));
    let md = MarkdownIt::empty();
    let ruleset = DocumentRuleSet {
        runs: vec![],
        checks: vec![
            InlineRuleFns {
                type_id: std::any::TypeId::of::<()>(),
                run: panic_run,
                check: |_| None,
                marker: 'x',
            },
            InlineRuleFns {
                type_id: std::any::TypeId::of::<()>(),
                run: panic_run,
                check: |_| Some(1),
                marker: 'x',
            },
        ],
        finalizers: vec![],
    };
    let mut state = check_state(&mut document, &md, &ruleset, "x");
    let mut context = scan(&mut state, 0..1).unwrap();

    assert_eq!(context.skip_token().unwrap(), 1);
    assert_eq!(context.skip_token(), None);
    assert_eq!(context.remaining(), "");
}

#[test]
fn check_dispatch_skips_non_matching_markers() {
    let mut document = Document::new("", crate::Root::new(""));
    let md = MarkdownIt::empty();
    let ruleset = DocumentRuleSet {
        runs: vec![],
        checks: vec![InlineRuleFns {
            type_id: std::any::TypeId::of::<()>(),
            run: panic_run,
            check: |_| panic!("marker must filter check dispatch"),
            marker: 'y',
        }],
        finalizers: vec![],
    };
    let mut state = check_state(&mut document, &md, &ruleset, "x");
    let mut context = scan(&mut state, 0..1).unwrap();

    assert_eq!(context.skip_token().unwrap(), 1);
    assert_eq!(context.skip_token(), None);
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "inline rule 0 reported invalid check length 0")]
fn check_rejects_zero_length() {
    let mut document = Document::new("", crate::Root::new(""));
    let md = MarkdownIt::empty();
    let ruleset = DocumentRuleSet {
        runs: vec![],
        checks: vec![check_rule(|_| Some(0))],
        finalizers: vec![],
    };
    let mut state = check_state(&mut document, &md, &ruleset, "x");
    let mut context = scan(&mut state, 0..1).unwrap();
    let _ = context.skip_token();
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "inline rule 0 reported invalid check length 2")]
fn check_rejects_out_of_bounds_length() {
    let mut document = Document::new("", crate::Root::new(""));
    let md = MarkdownIt::empty();
    let ruleset = DocumentRuleSet {
        runs: vec![],
        checks: vec![check_rule(|_| Some(2))],
        finalizers: vec![],
    };
    let mut state = check_state(&mut document, &md, &ruleset, "x");
    let mut context = scan(&mut state, 0..1).unwrap();
    let _ = context.skip_token();
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "inline rule 0 reported invalid check length 1")]
fn check_rejects_non_utf8_length() {
    let mut document = Document::new("", crate::Root::new(""));
    let md = MarkdownIt::empty();
    let ruleset = DocumentRuleSet {
        runs: vec![],
        checks: vec![check_rule(|_| Some(1))],
        finalizers: vec![],
    };
    let mut state = check_state(&mut document, &md, &ruleset, "éx");
    let mut context = scan(&mut state, 0..3).unwrap();
    let _ = context.skip_token();
}

#[test]
fn child_state_validates_ranges_and_empty_output() {
    let mut document = Document::new("", crate::Root::new(""));
    let md = MarkdownIt::empty();
    let ruleset = DocumentRuleSet {
        runs: vec![],
        checks: vec![],
        finalizers: vec![],
    };
    let mut state = check_state(&mut document, &md, &ruleset, "éx");

    for range in [0..0, 0..2, 0..3] {
        assert!(scan(&mut state, range).is_some());
    }
    for range in [1..2, 0..4, Range { start: 2, end: 1 }] {
        assert!(scan(&mut state, range).is_none());
    }

    let mut empty = scan(&mut state, 0..0).unwrap();
    assert_eq!(empty.skip_token(), None);

    let mut full = scan(&mut state, 0..3).unwrap();
    assert_eq!(full.skip_token().unwrap(), 2);
    assert_eq!(full.skip_token().unwrap(), 1);
    assert_eq!(full.skip_token(), None);
}

#[test]
fn child_state_keeps_parent_state() {
    let mut document = Document::new("", crate::Root::new(""));
    let md = MarkdownIt::empty();
    let ruleset = DocumentRuleSet {
        runs: vec![],
        checks: vec![],
        finalizers: vec![],
    };
    let mut state = parent_state(&mut document, &md, &ruleset);
    let before = check_parent_snapshot(&state);
    let mut context = scan(&mut state, 0..11).unwrap();

    assert_eq!(context.depth, 1);
    assert_eq!(context.link_level, 2);
    assert_eq!(context.remaining(), "{ 雪\n次 }");
    assert_eq!(context.markdown_it().max_nesting, md.max_nesting);
    assert_eq!(context.skip_token().unwrap(), 1);
    assert_eq!(context.skip_token().unwrap(), 1);

    assert_eq!(state.pending_text, Some((0, 3)));
    assert_eq!(
        (state.pos, state.pos_max, state.depth, state.link_level),
        (3, 14, 0, 2)
    );
    assert_eq!(state.nodes.len(), 1);
    assert_eq!(state.inline_ext.get::<FinalizerCalls>().unwrap().0, 41);
    assert_eq!(check_parent_snapshot(&state), before);
}

#[test]
fn check_matches_and_no_matches_keep_scratch_and_effects_private() {
    let mut document = Document::new("", crate::Root::new(""));
    fn check(context: &mut DocumentInlineState<'_>) -> Option<usize> {
        assert_eq!(context.root_ext.unwrap().get::<u16>(), Some(&73));
        context
            .inline_ext
            .get_or_insert_default::<FinalizerCalls>()
            .0 += 1;
        if context.remaining().starts_with('{') {
            {
                context.link_level = context
                    .link_level
                    .checked_add(1)
                    .expect("check link level overflow");
                Some(1)
            }
        } else {
            None
        }
    }

    let md = MarkdownIt::empty();
    let ruleset = DocumentRuleSet {
        runs: vec![run_rule(panic_run)],
        checks: vec![check_rule(check)],
        finalizers: vec![|_| panic!("check must not run finalizers")],
    };
    let mut root_ext = RootExtSet::new();
    root_ext.insert(73u16);
    let mut state = parent_state(&mut document, &md, &ruleset);
    state.root_ext = Some(&root_ext);
    state
        .document
        .node_mut(state.nodes[0])
        .set_srcmap(Some(SourcePos::new(10, 13)));
    state
        .document
        .node_mut(state.nodes[0])
        .attrs_mut()
        .push(("sentinel".into(), "unchanged".into()));
    state
        .document
        .node_mut(state.nodes[0])
        .ext_mut()
        .insert(17u8);
    let child = state.document.create_node(Text {
        content: "child".into(),
    });
    state.document.push_child(state.nodes[0], child);
    let before = check_parent_snapshot(&state);

    // Invalid and empty ranges must leave the same populated parent untouched.
    assert!(scan(&mut state, 3..4).is_none()); // inside 雪's UTF-8 bytes
    assert!(scan(&mut state, 0..12).is_none());
    assert!(scan(&mut state, Range { start: 2, end: 1 }).is_none());
    assert!(scan(&mut state, 0..0).unwrap().skip_token().is_none());
    assert_eq!(check_parent_snapshot(&state), before);

    for session in 0..4 {
        let mut context = match session {
            0 => state
                .child_state(0..state.remaining().len(), state.depth, state.link_level)
                .expect("current source range is valid"),
            1 => state
                .child_state(0..state.remaining().len(), state.depth, state.link_level)
                .expect("scan offset is valid"),
            _ => scan(&mut state, 0..11).unwrap(),
        };
        assert!(context.inline_ext.is_empty());
        assert_eq!(context.skip_token().unwrap(), 1);
        assert_eq!(context.link_level, 3);
        assert_eq!(context.inline_ext.get::<FinalizerCalls>().unwrap().0, 1);

        let scan_end = context.remaining().len();
        let mut child = scan(&mut context, 0..scan_end).unwrap();
        assert!(child.inline_ext.is_empty());
        assert!(child.skip_token().is_some());
        assert_eq!(child.inline_ext.get::<FinalizerCalls>().unwrap().0, 1);
        assert_eq!(context.inline_ext.get::<FinalizerCalls>().unwrap().0, 1);

        while context.skip_token().is_some() {}
        assert_eq!(context.link_level, 3);
        assert!(context.inline_ext.get::<FinalizerCalls>().unwrap().0 > 1);
        assert_eq!(check_parent_snapshot(&state), before);
    }
}

#[test]
fn recursive_check_validates_ranges_and_empty_suffixes() {
    let mut document = Document::new("", crate::Root::new(""));
    let md = MarkdownIt::empty();
    let ruleset = DocumentRuleSet {
        runs: vec![],
        checks: vec![],
        finalizers: vec![],
    };
    let mut state = check_state(&mut document, &md, &ruleset, "éx");
    let mut context = scan(&mut state, 0..3).unwrap();

    assert!(scan(&mut context, Range { start: 2, end: 1 }).is_none());
    assert!(scan(&mut context, 0..4).is_none());
    assert!(scan(&mut context, 1..2).is_none());

    let mut empty = scan(&mut context, 0..0).unwrap();
    assert_eq!(empty.depth, 2);
    assert_eq!(empty.remaining(), "");
    assert_eq!(empty.trailing_text(), "");
    assert_eq!(empty.skip_token(), None);

    let mut full = scan(&mut context, 0..3).unwrap();
    assert_eq!(full.remaining(), "éx");
    assert_eq!(full.link_level, 0);
    assert_eq!(full.skip_token().unwrap(), 2);
    assert_eq!(full.skip_token().unwrap(), 1);
    assert_eq!(full.skip_token(), None);

    // A rule reporting no match does not poison later child sessions.
    let no_match = DocumentRuleSet {
        runs: vec![],
        checks: vec![InlineRuleFns {
            type_id: std::any::TypeId::of::<()>(),
            run: panic_run,
            check: |_| None,
            marker: 'x',
        }],
        finalizers: vec![],
    };
    let mut state = check_state(&mut document, &md, &no_match, "x");
    let mut context = scan(&mut state, 0..1).unwrap();
    assert!(context.skip_token().is_some());
    assert_eq!(context.skip_token(), None);
    assert!(scan(&mut context, 0..0).is_some());
    assert!(scan(&mut context, 0..1).is_none());
}

#[test]
fn recursive_check_depth_limit_is_exact() {
    let mut document = Document::new("", crate::Root::new(""));
    let mut md = MarkdownIt::empty();
    md.max_nesting = 3;
    let ruleset = DocumentRuleSet {
        runs: vec![],
        checks: vec![],
        finalizers: vec![],
    };
    let mut state = check_state(&mut document, &md, &ruleset, "xy");

    let mut first = scan(&mut state, 0..2).unwrap();
    assert_eq!(first.depth, 1);

    let mut second = scan(&mut first, 0..2).unwrap();
    assert_eq!(second.depth, 2);
    assert_eq!(second.remaining(), "xy");

    let mut limit = scan(&mut second, 0..2).unwrap();
    assert_eq!(limit.depth, 3);
    assert_eq!(limit.skip_token().unwrap(), 2);
    assert!(limit.skip_token().is_none());
}

#[test]
fn recursive_check_relative_ranges_keep_child_state_isolated() {
    let mut document = Document::new("", crate::Root::new(""));
    let md = MarkdownIt::empty();
    let ruleset = DocumentRuleSet {
        runs: vec![],
        checks: vec![],
        finalizers: vec![],
    };
    let mut state = check_state(&mut document, &md, &ruleset, "abcdef");
    let mut parent = scan(&mut state, 0..6).unwrap();

    assert_eq!(parent.skip_token().unwrap(), 1);
    assert_eq!(parent.trailing_text(), "a");

    let mut child = scan(&mut parent, 1..3).unwrap();
    assert_eq!(child.remaining(), "cd");
    assert_eq!(child.trailing_text(), "");
    assert_eq!(child.skip_token().unwrap(), 1);
    assert_eq!(child.skip_token().unwrap(), 1);
    assert_eq!(child.trailing_text(), "cd");

    assert_eq!(parent.remaining(), "bcdef");
    assert_eq!(parent.trailing_text(), "a");
    assert_eq!(parent.link_level, 0);
    assert_eq!(state.pending_text, None);
    assert_eq!((state.pos, state.pos_max), (0, 6));
}

#[test]
fn recursive_check_inherits_current_link_level_and_isolates_effects() {
    let mut document = Document::new("", crate::Root::new(""));
    let mut md = MarkdownIt::empty();
    crate::plugins::html::html_inline::add(&mut md);
    let ruleset = md.inline.document_rules();
    let source = "<a><a>";
    let mut state = check_state(&mut document, &md, &ruleset, source);
    let mut parent = scan(&mut state, 0..source.len()).unwrap();

    assert_eq!(parent.skip_token().unwrap(), 3);
    assert_eq!(parent.link_level, 1);

    let mut child = scan(&mut parent, 0..3).unwrap();
    assert_eq!(child.link_level, 1);
    assert_eq!(child.skip_token().unwrap(), 3);
    assert_eq!(child.link_level, 2);

    assert_eq!(parent.remaining(), "<a>");
    assert_eq!(parent.link_level, 1);
    assert_eq!(state.link_level, 0);
}

#[test]
fn recursive_check_child_scratch_is_private() {
    let mut document = Document::new("", crate::Root::new(""));
    use crate::parser::inline::helpers::code_pair::CodePairScanner;

    let mut md = MarkdownIt::empty();
    md.inline.add_rule::<CodePairScanner<'`'>>();
    let ruleset = md.inline.document_rules();
    let mut state = check_state(&mut document, &md, &ruleset, "`x`");
    let mut parent = scan(&mut state, 0..3).unwrap();

    // An unclosed child scan must not poison its siblings.
    let mut short = scan(&mut parent, 0..2).unwrap();
    assert_eq!(short.skip_token().unwrap(), 1);
    assert_eq!(short.skip_token().unwrap(), (2) - (1));
    assert_eq!(short.skip_token(), None);

    let mut long = scan(&mut parent, 0..3).unwrap();
    assert_eq!(long.skip_token().unwrap(), 3);
    assert_eq!(long.skip_token(), None);

    let mut again = scan(&mut parent, 0..3).unwrap();
    assert_eq!(again.skip_token().unwrap(), 3);
    assert_eq!(again.skip_token(), None);
}

#[test]
fn recursive_check_same_range_terminates_at_depth_limit() {
    let mut document = Document::new("", crate::Root::new(""));
    use std::cell::Cell;

    thread_local! {
        static CALLS: Cell<usize> = const { Cell::new(0) };
    }

    fn same_range(context: &mut DocumentInlineState<'_>) -> Option<usize> {
        CALLS.with(|calls| calls.set(calls.get() + 1));
        let len = context.remaining().len();
        match scan(context, 0..len) {
            Some(mut child) => child.skip_token(),
            None => None,
        }
    }

    let mut md = MarkdownIt::empty();
    md.max_nesting = 5;
    let ruleset = DocumentRuleSet {
        runs: vec![],
        checks: vec![InlineRuleFns {
            type_id: std::any::TypeId::of::<()>(),
            run: panic_run,
            check: same_range,
            marker: '#',
        }],
        finalizers: vec![],
    };
    let mut state = check_state(&mut document, &md, &ruleset, "#");
    let mut context = scan(&mut state, 0..1).unwrap();
    CALLS.with(|calls| calls.set(0));

    assert_eq!(context.skip_token().unwrap(), 1);
    assert_eq!(CALLS.with(Cell::get), 4);
    assert_eq!(context.remaining(), "");
    assert_eq!(context.skip_token(), None);
}

#[test]
fn recursive_check_long_chain_is_linear_and_stack_safe() {
    let mut document = Document::new("", crate::Root::new(""));
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    static CALLS: AtomicUsize = AtomicUsize::new(0);

    fn nested(context: &mut DocumentInlineState<'_>) -> Option<usize> {
        CALLS.fetch_add(1, Ordering::SeqCst);
        if !context.remaining().starts_with('^') {
            return None;
        }
        match scan(context, 1..context.remaining().len()) {
            Some(mut child) => child.skip_token().map(|len| 1 + len),
            None => None,
        }
    }

    const CHAIN: usize = 10_000;
    let mut md = MarkdownIt::empty();
    md.max_nesting = 65_536;
    let ruleset = DocumentRuleSet {
        runs: vec![],
        checks: vec![InlineRuleFns {
            type_id: std::any::TypeId::of::<()>(),
            run: panic_run,
            check: nested,
            marker: '^',
        }],
        finalizers: vec![],
    };
    let source = "^".repeat(CHAIN) + "x";
    let mut state = check_state(&mut document, &md, &ruleset, &source);
    let mut context = scan(&mut state, 0..source.len()).unwrap();

    CALLS.store(0, Ordering::SeqCst);
    let start = Instant::now();
    let token = context.skip_token().unwrap();
    assert!(start.elapsed() < Duration::from_secs(10));
    assert_eq!(token, source.len());
    assert_eq!(CALLS.load(Ordering::SeqCst), CHAIN);
    assert_eq!(context.skip_token(), None);
}

#[test]
fn check_dispatch_keeps_wildcard_position() {
    let mut document = Document::new("", crate::Root::new(""));
    let md = MarkdownIt::empty();
    let ruleset = DocumentRuleSet {
        runs: vec![],
        checks: vec![
            check_rule(|_| Some(1)),
            InlineRuleFns {
                type_id: std::any::TypeId::of::<()>(),
                run: panic_run,
                check: |_| panic!("specific rule must not run after a wildcard match"),
                marker: 'x',
            },
        ],
        finalizers: vec![],
    };
    let mut state = check_state(&mut document, &md, &ruleset, "x");
    let mut context = scan(&mut state, 0..1).unwrap();

    assert_eq!(context.skip_token().unwrap(), 1);
}

#[test]
fn check_html_effects_stay_in_the_session() {
    let mut document = Document::new("", crate::Root::new(""));
    let mut md = MarkdownIt::empty();
    crate::plugins::cmark::block::paragraph::add(&mut md);
    crate::plugins::html::html_inline::add(&mut md);
    let ruleset = md.inline.document_rules();
    let source = "<a title=']'>x</a>";
    let mut state = check_state(&mut document, &md, &ruleset, source);
    let mut context = scan(&mut state, 0..source.len()).unwrap();

    let token = context.skip_token().unwrap();
    assert_eq!(token, 13);
    assert_eq!(context.link_level, 1);
    assert_eq!(context.skip_token().unwrap(), (14) - (13));
    assert_eq!(context.skip_token().unwrap(), (18) - (14));
    assert_eq!(context.link_level, 0);
    assert!(context.skip_token().is_none());
    assert_eq!(state.link_level, 0);
    assert_eq!(state.pos, 0);
    assert_eq!(scan(&mut state, 0..source.len()).unwrap().link_level, 0);
}

#[test]
fn invalid_check_effect_panics_and_preserves_context() {
    let mut document = Document::new("", crate::Root::new(""));
    let md = MarkdownIt::empty();
    let ruleset = DocumentRuleSet {
        runs: vec![],
        checks: vec![check_rule(|context| {
            context.link_level = context
                .link_level
                .checked_add(1)
                .expect("check link level overflow");
            Some(1)
        })],
        finalizers: vec![],
    };
    let mut state = check_state(&mut document, &md, &ruleset, "x");
    state.link_level = i32::MAX;
    let mut context = scan(&mut state, 0..1).unwrap();
    assert_eq!(context.remaining(), "x");
    assert_eq!(context.link_level, i32::MAX);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| context.skip_token()));
    assert!(result.is_err());
    assert_eq!(context.remaining(), "x");
    assert_eq!(context.link_level, i32::MAX);
    assert_eq!(state.link_level, i32::MAX);
}

struct LaterMarkerCheck;

impl InlineRule for LaterMarkerCheck {
    const MARKER: char = '*';

    fn check(_: &mut DocumentInlineState<'_>) -> Option<usize> {
        Some(1)
    }

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        panic_run(state)
    }
}

struct LaterUnderscoreCheck;

impl InlineRule for LaterUnderscoreCheck {
    const MARKER: char = '_';

    fn check(_: &mut DocumentInlineState<'_>) -> Option<usize> {
        Some(1)
    }

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        panic_run(state)
    }
}

struct LaterUnicodeMarkerCheck;

impl InlineRule for LaterUnicodeMarkerCheck {
    const MARKER: char = '雪';

    fn check(_: &mut DocumentInlineState<'_>) -> Option<usize> {
        Some('雪'.len_utf8())
    }

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        panic_run(state)
    }
}

struct ExpectLinkLevelOne;

impl InlineRule for ExpectLinkLevelOne {
    const MARKER: char = '[';

    fn check(context: &mut DocumentInlineState<'_>) -> Option<usize> {
        assert_eq!(context.link_level, 1);
        Some(1)
    }

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        panic_run(state)
    }
}

#[derive(Debug)]
struct RecordingFormatter {
    calls: Arc<Mutex<Vec<String>>>,
    reject: bool,
}

impl LinkFormatter for RecordingFormatter {
    fn validate_link(&self, url: &str) -> Option<()> {
        self.calls.lock().unwrap().push(format!("validate:{url}"));
        if self.reject { None } else { Some(()) }
    }

    fn normalize_link(&self, url: &str) -> String {
        self.calls.lock().unwrap().push(format!("normalize:{url}"));
        url.to_owned()
    }

    fn normalize_link_text(&self, url: &str) -> String {
        self.calls.lock().unwrap().push(format!("text:{url}"));
        url.to_owned()
    }
}

#[test]
fn check_newline_consumes_one_byte_and_resets_pending() {
    let mut document = Document::new("", crate::Root::new(""));
    let mut md = MarkdownIt::empty();
    crate::plugins::cmark::inline::newline::add(&mut md);
    let ruleset = md.inline.document_rules();
    let mut state = check_state(&mut document, &md, &ruleset, "a  \n \tb");
    let scan_end = state.pos_max;
    let mut context = scan(&mut state, 0..scan_end).unwrap();

    assert_eq!(context.skip_token().unwrap(), 3);
    assert_eq!(context.trailing_text(), "a  ");

    assert_eq!(context.skip_token().unwrap(), 1);
    assert_eq!(context.trailing_text(), "");

    assert_eq!(context.skip_token().unwrap(), 3);
    assert_eq!(context.trailing_text(), " \tb");
    assert!(context.skip_token().is_none());
    assert_eq!(state.pending_text, None);
}

#[test]
fn check_entity_uses_raw_byte_length() {
    let mut document = Document::new("", crate::Root::new(""));
    let mut md = MarkdownIt::empty();
    crate::plugins::cmark::inline::entity::add(&mut md);
    let ruleset = md.inline.document_rules();

    for (source, len) in [
        ("&#91;", 5),
        ("&#x5D;", 6),
        ("&amp;", 5),
        ("&#0;", 4),
        ("&#x110000;", 10),
    ] {
        let mut state = check_state(&mut document, &md, &ruleset, source);
        let mut context = scan(&mut state, 0..source.len()).unwrap();
        assert_eq!(context.skip_token().unwrap(), (len), "{source}");
        assert!(context.skip_token().is_none(), "{source}");
    }
}

#[test]
fn check_unknown_and_truncated_entities_fall_back_to_text() {
    let mut document = Document::new("", crate::Root::new(""));
    let mut md = MarkdownIt::empty();
    crate::plugins::cmark::inline::entity::add(&mut md);
    let ruleset = md.inline.document_rules();

    for source in ["&notanentity;", "&#x;", "&amp"] {
        let mut state = check_state(&mut document, &md, &ruleset, source);
        let mut context = scan(&mut state, 0..source.len()).unwrap();
        assert_eq!(context.skip_token().unwrap(), 1, "{source}");
        assert_eq!(
            context.skip_token().unwrap(),
            (source.len()) - (1),
            "{source}"
        );
        assert!(context.skip_token().is_none(), "{source}");
    }

    let mut state = check_state(&mut document, &md, &ruleset, "&amp;");
    let mut short = scan(&mut state, 0..4).unwrap();
    assert_eq!(short.skip_token().unwrap(), 1);
    let mut long = scan(&mut state, 0..5).unwrap();
    assert_eq!(long.skip_token().unwrap(), 5);
}

#[test]
fn check_emphasis_defers_to_later_same_marker_rule() {
    let mut document = Document::new("", crate::Root::new(""));
    let mut md = MarkdownIt::empty();
    crate::plugins::cmark::inline::emphasis::add(&mut md);
    md.inline.add_rule::<LaterMarkerCheck>();
    let ruleset = md.inline.document_rules();

    let mut state = check_state(&mut document, &md, &ruleset, "*");
    let mut context = scan(&mut state, 0..1).unwrap();
    assert_eq!(context.skip_token().unwrap(), 1);

    let mut md = MarkdownIt::empty();
    crate::plugins::cmark::inline::emphasis::add(&mut md);
    md.inline.add_rule::<LaterUnderscoreCheck>();
    let ruleset = md.inline.document_rules();

    let mut state = check_state(&mut document, &md, &ruleset, "_");
    let mut context = scan(&mut state, 0..1).unwrap();
    assert_eq!(context.skip_token().unwrap(), 1);
}

#[test]
fn check_emphasis_defers_for_unicode_marker() {
    let mut document = Document::new("", crate::Root::new(""));
    fn node(document: &mut Document) -> NodeId {
        document.create_node(Text {
            content: String::new(),
        })
    }

    let mut md = MarkdownIt::empty();
    crate::parser::inline::helpers::emph_pair::add_with::<'雪', 1, true>(&mut md, node);
    md.inline.add_rule::<LaterUnicodeMarkerCheck>();
    let ruleset = md.inline.document_rules();

    let mut state = check_state(&mut document, &md, &ruleset, "雪");
    let mut context = scan(&mut state, 0..3).unwrap();
    assert_eq!(context.skip_token().unwrap(), 3);
}

#[test]
fn check_html_comment_cache_is_private_to_each_session() {
    let mut document = Document::new("", crate::Root::new(""));
    let mut md = MarkdownIt::empty();
    crate::plugins::html::html_inline::add(&mut md);
    let ruleset = md.inline.document_rules();
    let mut state = check_state(&mut document, &md, &ruleset, "<!-- [ -->");
    state.inline_ext.insert(FinalizerCalls(41));
    let node = state.document.create_node(Text {
        content: "sentinel".into(),
    });
    state.nodes.push(node);
    let before = check_parent_snapshot(&state);

    let mut short = scan(&mut state, 0..5).unwrap();
    while short.skip_token().is_some() {}

    let scan_end = state.pos_max;
    let mut long = scan(&mut state, 0..scan_end).unwrap();
    assert_eq!(long.skip_token().unwrap(), 10);
    assert!(long.skip_token().is_none());

    let scan_end = state.pos_max;
    let mut long_first = scan(&mut state, 0..scan_end).unwrap();
    assert_eq!(long_first.skip_token().unwrap(), 10);
    let mut short_after = scan(&mut state, 0..5).unwrap();
    while short_after.skip_token().is_some() {}
    assert_eq!(check_parent_snapshot(&state), before);
}

#[cfg(debug_assertions)]
#[test]
fn invalid_check_length_panics_and_preserves_pending() {
    let mut document = Document::new("", crate::Root::new(""));
    let md = MarkdownIt::empty();
    let ruleset = DocumentRuleSet {
        runs: vec![],
        checks: vec![
            InlineRuleFns {
                type_id: std::any::TypeId::of::<()>(),
                run: panic_run,
                check: |context| {
                    if context.remaining() == "aa" {
                        context.push_text(context.pos, context.pos + 1);
                        Some(1)
                    } else {
                        None
                    }
                },
                marker: 'a',
            },
            InlineRuleFns {
                type_id: std::any::TypeId::of::<()>(),
                run: panic_run,
                check: |_| Some(0),
                marker: 'a',
            },
        ],
        finalizers: vec![],
    };
    let mut state = check_state(&mut document, &md, &ruleset, "aa");
    state.link_level = 2;
    state.inline_ext.insert(FinalizerCalls(41));
    let node = state.document.create_node(Text {
        content: "sentinel".into(),
    });
    state.nodes.push(node);
    let before = check_parent_snapshot(&state);
    let mut context = scan(&mut state, 0..2).unwrap();

    assert_eq!(context.skip_token().unwrap(), 1);
    assert_eq!(context.trailing_text(), "a");
    assert_eq!(context.link_level, 2);
    assert_eq!(context.remaining(), "a");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| context.skip_token()));
    assert!(result.is_err());
    assert_eq!(context.remaining(), "a");
    assert_eq!(context.trailing_text(), "a");
    assert_eq!(context.link_level, 2);
    assert_eq!(state.link_level, 2);
    assert_eq!(check_parent_snapshot(&state), before);
}

#[test]
fn invalid_check_effect_underflow_panics_and_preserves_context() {
    let mut document = Document::new("", crate::Root::new(""));
    let md = MarkdownIt::empty();
    let ruleset = DocumentRuleSet {
        runs: vec![],
        checks: vec![check_rule(|context| {
            context.link_level = context
                .link_level
                .checked_add(-1)
                .expect("check link level overflow");
            Some(1)
        })],
        finalizers: vec![],
    };
    let mut state = check_state(&mut document, &md, &ruleset, "x");
    state.link_level = i32::MIN;
    let mut context = scan(&mut state, 0..1).unwrap();
    assert_eq!(context.remaining(), "x");
    assert_eq!(context.link_level, i32::MIN);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| context.skip_token()));
    assert!(result.is_err());
    assert_eq!(context.remaining(), "x");
    assert_eq!(context.link_level, i32::MIN);
    assert_eq!(state.link_level, i32::MIN);
}

#[test]
fn check_html_level_is_read_by_later_rules_and_inherited_from_parent() {
    let mut document = Document::new("", crate::Root::new(""));
    let mut md = MarkdownIt::empty();
    crate::plugins::html::html_inline::add(&mut md);
    md.inline.add_rule::<ExpectLinkLevelOne>();
    let ruleset = md.inline.document_rules();
    let source = "<a>[";
    let mut state = check_state(&mut document, &md, &ruleset, source);
    let mut context = scan(&mut state, 0..source.len()).unwrap();

    assert_eq!(context.skip_token().unwrap(), 3);
    assert_eq!(context.skip_token().unwrap(), 1);
    assert_eq!(context.link_level, 1);
    assert_eq!(state.link_level, 0);

    let mut inherited = check_state_with_level(&mut document, &md, &ruleset, source, 5);
    let context = scan(&mut inherited, 0..source.len()).unwrap();
    assert_eq!(context.link_level, 5);
    assert_eq!(inherited.link_level, 5);
}

#[test]
fn check_html_lone_closing_tag_keeps_negative_level() {
    let mut document = Document::new("", crate::Root::new(""));
    let mut md = MarkdownIt::empty();
    crate::plugins::html::html_inline::add(&mut md);
    let ruleset = md.inline.document_rules();
    let source = "</a></A>";
    let mut state = check_state(&mut document, &md, &ruleset, source);
    let mut context = scan(&mut state, 0..source.len()).unwrap();

    assert_eq!(context.skip_token().unwrap(), 4);
    assert_eq!(context.link_level, -1);
    assert_eq!(context.skip_token().unwrap(), 4);
    assert_eq!(context.link_level, -1);
    assert!(context.skip_token().is_none());
    assert_eq!(state.link_level, 0);
}

#[test]
fn check_autolink_email_normalizes_mailto_before_validate() {
    let mut document = Document::new("", crate::Root::new(""));
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut md = MarkdownIt::empty();
    crate::plugins::cmark::inline::autolink::add(&mut md);
    md.link_formatter = Box::new(RecordingFormatter {
        calls: calls.clone(),
        reject: false,
    });
    let ruleset = md.inline.document_rules();
    let source = "<foo@example.com>";
    let mut state = check_state(&mut document, &md, &ruleset, source);
    let mut context = scan(&mut state, 0..source.len()).unwrap();

    assert_eq!(context.skip_token().unwrap(), source.len());
    drop(context);
    assert_eq!(
        *calls.lock().unwrap(),
        vec![
            "normalize:mailto:foo@example.com".to_owned(),
            "validate:mailto:foo@example.com".to_owned(),
            "text:foo@example.com".to_owned(),
        ]
    );
}

fn check_state_with_level<'a>(
    document: &'a mut Document,
    md: &'a MarkdownIt,
    ruleset: &'a DocumentRuleSet,
    source: &str,
    link_level: i32,
) -> DocumentInlineState<'a> {
    let mut state = check_state(document, md, ruleset, source);
    state.link_level = link_level;
    state
}

#[test]
fn child_link_level_override_is_isolated() {
    let mut document = Document::new("", crate::Root::new(""));
    fn emit(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        let initial = state.link_level;
        let len = state.remaining().len();
        let check = scan(state, 0..len).unwrap();
        assert_eq!(check.link_level, initial);
        assert_eq!(state.depth, 1);
        assert!(state.inline_ext.get::<FinalizerCalls>().is_none());
        state.inline_ext.insert(FinalizerCalls(0));
        state.link_level = 99;
        Some((
            Some(state.document.create_node(Text {
                content: initial.to_string(),
            })),
            len,
        ))
    }

    let md = MarkdownIt::empty();
    let ruleset = DocumentRuleSet {
        runs: vec![run_rule(emit)],
        checks: vec![],
        finalizers: vec![count_finalizer],
    };
    let mut state = parent_state(&mut document, &md, &ruleset);
    // This fixture's parent link level is 2; range 1..10 excludes braces.
    let inherited = state.parse_subrange(1..10).unwrap();
    assert_eq!(
        state
            .document
            .node(inherited[0])
            .cast::<Text>()
            .unwrap()
            .content,
        "2"
    );

    for level in [3, -1] {
        let children = state.parse_subrange_with_link_level(1..10, level).unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(
            state
                .document
                .node(children[0])
                .cast::<Text>()
                .unwrap()
                .content,
            level.to_string()
        );
        assert_eq!(
            state.document.node(children[0]).srcmap(),
            state.document.node(inherited[0]).srcmap()
        );
    }

    assert_eq!((state.pos, state.pos_max, state.link_level), (3, 14, 2));
    assert_eq!(state.pending_text, Some((0, 3)));
    assert_eq!(state.inline_ext.get::<FinalizerCalls>().unwrap().0, 41);
    assert_eq!(state.nodes.len(), 1);
    assert_eq!(
        state
            .document
            .node(state.nodes[0])
            .cast::<Text>()
            .unwrap()
            .content,
        "sentinel"
    );
}

#[test]
fn child_link_level_override_keeps_range_and_depth_rules() {
    let mut document = Document::new("", crate::Root::new(""));
    let mut md = MarkdownIt::empty();
    md.max_nesting = 1;
    let ruleset = DocumentRuleSet {
        runs: vec![run_rule(|_| panic!("depth limit must skip rules"))],
        checks: vec![],
        finalizers: vec![|_| panic!("literal children must skip finalizers")],
    };
    let mut state = parent_state(&mut document, &md, &ruleset);
    assert!(state.parse_subrange_with_link_level(0..12, 3).is_none());
    assert!(state.parse_subrange_with_link_level(3..4, 3).is_none());
    assert!(
        state
            .parse_subrange_with_link_level(0..0, 3)
            .unwrap()
            .is_empty()
    );
    let children = state.parse_subrange_with_link_level(0..11, 3).unwrap();
    assert_eq!(
        state
            .document
            .node(children[0])
            .cast::<Text>()
            .unwrap()
            .content,
        "{ 雪\n次 }"
    );
    assert_eq!(state.link_level, 2);
}
