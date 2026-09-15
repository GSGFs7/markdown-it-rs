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
        let Some(mut child) = context.probe_subrange(1..context.remaining().len()) else {
            return InlineProbeResult::NoMatch;
        };
        assert_eq!(child.trailing_text(), "");
        let Some(token) = child.next_token() else {
            return InlineProbeResult::NoMatch;
        };
        InlineProbeResult::Match {
            len: 1 + token.range.end,
            kind: InlineProbeKind::Token,
        }
    }

    fn run(_: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
        panic!("nested rule run must not be called by the probe consumer")
    }
}

struct DefaultProbe;
impl InlineRule for DefaultProbe {
    const MARKER: char = '?';

    fn run(_: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
        panic!("default probe rule run must not be called")
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
            Some(token) => format!("{}..{}", token.range.start, token.range.end),
            None => "empty".to_owned(),
        };
        Some((Some(NodeDraft::new(Text { content: summary })), len))
    }
}

fn parser(max_nesting: u32) -> MarkdownIt {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    md.inline.add_rule::<Nested>();
    md.inline.add_rule::<DefaultProbe>();
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
fn nested_default_probe_falls_back_to_text() {
    let md = parser(4);
    let document = md.parse_document_direct("@^^?").unwrap();
    assert_eq!(document.into_legacy().render(), "<p>0..3</p>\n");
}

#[test]
fn recursive_depth_limit_falls_back_inward() {
    let md = parser(3);
    let document = md.parse_document_direct("@^^x").unwrap();
    assert_eq!(document.into_legacy().render(), "<p>0..2</p>\n");
}
