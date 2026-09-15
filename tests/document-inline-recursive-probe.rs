use markdown_it::parser::inline::probe::{InlineProbeContext, InlineProbeKind, InlineProbeResult};
use markdown_it::parser::inline::{InlineRule, Text};
use markdown_it::{DocumentInlineState, MarkdownIt, NodeDraft};

struct Nested;
impl InlineRule for Nested {
    const MARKER: char = '^';

    fn probe(context: &mut InlineProbeContext<'_>) -> InlineProbeResult {
        if !context.remaining().starts_with('^') {
            return InlineProbeResult::NoMatch;
        }
        let mut child = match context.probe_subrange(1..context.remaining().len()) {
            Ok(Some(child)) => child,
            Ok(None) => unreachable!("suffix is a valid UTF-8 range"),
            Err(error) => return InlineProbeResult::Error(error),
        };
        assert_eq!(child.trailing_text(), "");
        match child.next_token() {
            Ok(Some(token)) => InlineProbeResult::Match {
                len: 1 + token.range.end,
                kind: InlineProbeKind::Token,
            },
            Ok(None) => InlineProbeResult::NoMatch,
            Err(error) => InlineProbeResult::Error(error),
        }
    }

    fn run(_: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
        panic!("nested rule run must not be called by the probe consumer")
    }
}

struct Unsupported;
impl InlineRule for Unsupported {
    const MARKER: char = '?';

    fn run(_: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
        panic!("unsupported rule run must not be called")
    }
}

struct Consumer;
impl InlineRule for Consumer {
    const MARKER: char = '@';

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
        let len = state.remaining().len();
        let mut context = state.probe_subrange(1..len).unwrap();
        let first = context.next_token();
        let summary = match first {
            Ok(Some(token)) => format!("{}..{}", token.range.start, token.range.end),
            Ok(None) => "empty".to_owned(),
            Err(error) => {
                // The whole recursive failure leaves the outer cursor intact.
                assert_eq!(context.remaining(), &state.remaining()[1..]);
                assert_eq!(context.next_token(), Err(error));
                format!("{error:?}")
            }
        };
        Some((Some(NodeDraft::new(Text { content: summary })), len))
    }
}

fn parser(max_nesting: u32) -> MarkdownIt {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    md.inline.add_rule::<Nested>();
    md.inline.add_rule::<Unsupported>();
    md.inline.add_rule::<Consumer>();
    md.max_nesting = max_nesting;
    md
}

#[test]
fn recursive_probe_returns_outer_relative_length() {
    let md = parser(4);
    let document = md.parse_document_direct("@^^雪").unwrap();
    assert_eq!(document.into_legacy().render(), "<p>0..5</p>\n");
}

#[test]
fn nested_unsupported_is_not_plain_text() {
    let md = parser(4);
    let document = md.parse_document_direct("@^^?").unwrap();
    let html = document.into_legacy().render();
    assert!(html.contains("UnsupportedRule"), "{html}");
}

#[test]
fn recursive_depth_failure_is_explicit() {
    let md = parser(3);
    let document = md.parse_document_direct("@^^x").unwrap();
    let html = document.into_legacy().render();
    assert!(html.contains("NestingLimit"), "{html}");
}

#[test]
fn child_probe_does_not_call_code_pair_factory() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use markdown_it::generics::inline::code_pair;

    static FACTORY_CALLS: AtomicUsize = AtomicUsize::new(0);

    fn factory(_: usize) -> NodeDraft {
        FACTORY_CALLS.fetch_add(1, Ordering::SeqCst);
        NodeDraft::new(Text {
            content: "pair".to_owned(),
        })
    }

    struct PairConsumer;
    impl InlineRule for PairConsumer {
        const MARKER: char = '@';

        fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
            let len = state.remaining().len();
            let mut context = state.probe_subrange(1..len).unwrap();
            let summary = match context.next_token() {
                Ok(Some(token)) => format!("{}..{}", token.range.start, token.range.end),
                Ok(None) => "empty".to_owned(),
                Err(error) => format!("{error:?}"),
            };
            Some((Some(NodeDraft::new(Text { content: summary })), len))
        }
    }

    FACTORY_CALLS.store(0, Ordering::SeqCst);
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    code_pair::add_with::<'$'>(&mut md, factory);
    md.inline.add_rule::<PairConsumer>();

    let html = md
        .parse_document_direct("@$x$")
        .unwrap()
        .into_legacy()
        .render();
    assert_eq!(html, "<p>0..3</p>\n");
    assert_eq!(FACTORY_CALLS.load(Ordering::SeqCst), 0);

    md.parse_document_direct("$x$").unwrap();
    assert_eq!(FACTORY_CALLS.load(Ordering::SeqCst), 1);
}
