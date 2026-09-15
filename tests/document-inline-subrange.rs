use std::sync::{Arc, Mutex};

use markdown_it::parser::inline::probe::{InlineProbeContext, InlineProbeKind, InlineProbeResult};
use markdown_it::parser::inline::{InlineRule, Text};
use markdown_it::parser::linkfmt::LinkFormatter;
use markdown_it::{DocumentInlineState, MarkdownIt, NodeDraft, NodeValue};

#[derive(Debug)]
struct TestContainer;
impl NodeValue for TestContainer {
    fn render(&self, node: &markdown_it::Node, fmt: &mut dyn markdown_it::Renderer) {
        fmt.text("{");
        fmt.contents(&node.children);
        fmt.text("}");
    }
}

struct ContainerRule;
impl InlineRule for ContainerRule {
    const MARKER: char = '{';

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
        let source = state.remaining();
        if !source.starts_with('{') {
            return None;
        }
        // Test-only brace grammar; this is not Markdown link-label probing.
        let mut depth = 0usize;
        let mut close = None;
        for (index, ch) in source.char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(index);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close = close?;
        let mut node = NodeDraft::new(TestContainer);
        *node.children_mut() = state.parse_subrange(1..close)?;
        Some((Some(node), close + 1))
    }
}

fn parser() -> MarkdownIt {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    markdown_it::plugins::cmark::inline::emphasis::add(&mut md);
    markdown_it::plugins::cmark::inline::backticks::add(&mut md);
    md.inline.add_rule::<ContainerRule>();
    md
}

#[test]
fn custom_rule_parses_nested_children_without_losing_parent_text() {
    let md = parser();
    let document = md.parse_document_direct("前{ 雪 {*x*} }尾 {}").unwrap();
    assert_eq!(
        document.into_legacy().render(),
        "<p>前{ 雪 {<em>x</em>} }尾 {}</p>\n"
    );
}

#[test]
fn delimiters_and_negative_caches_do_not_leak_between_ranges() {
    let md = parser();
    for (source, expected) in [
        ("*a{*b*}c*", "<p><em>a{<em>b</em>}c</em></p>\n"),
        ("*a{b*}c", "<p>*a{b*}c</p>\n"),
        ("a{*b}c*", "<p>a{*b}c*</p>\n"),
        ("{`x} `y`", "<p>{`x} <code>y</code></p>\n"),
    ] {
        assert_eq!(
            md.parse_document_direct(source)
                .unwrap()
                .into_legacy()
                .render(),
            expected,
            "{source}"
        );
    }
}

#[test]
fn nesting_limit_stops_recursive_container_rules() {
    let mut md = parser();
    md.max_nesting = 2;
    let document = md.parse_document_direct("{ {*x*} *y* }").unwrap();
    assert_eq!(
        document.into_legacy().render(),
        "<p>{ {*x*} <em>y</em> }</p>\n"
    );
}

struct ProbeSummaryRule;
impl InlineRule for ProbeSummaryRule {
    const MARKER: char = '{';

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
        let source = state.remaining();
        if !source.starts_with('{') {
            return None;
        }
        let close = source.find('}')?;
        let mut context = state.probe_subrange(1..close)?;
        let mut summary = String::new();
        while let Some(token) = context.next_token() {
            let kind = match token.kind {
                InlineProbeKind::Text => "text",
                InlineProbeKind::Token => "token",
            };
            summary.push_str(&format!(
                "{}..{}={kind};",
                token.range.start, token.range.end
            ));
        }
        Some((Some(NodeDraft::new(Text { content: summary })), close + 1))
    }
}

struct PanicProbeRule;
impl InlineRule for PanicProbeRule {
    const MARKER: char = 'x';

    fn probe(_: &mut InlineProbeContext<'_>) -> InlineProbeResult {
        panic!("normal direct parsing must not call probe")
    }

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
        if !state.remaining().starts_with('x') {
            return None;
        }
        Some((
            Some(NodeDraft::new(Text {
                content: "X".to_owned(),
            })),
            1,
        ))
    }
}

struct NoProbeRule;
impl InlineRule for NoProbeRule {
    const MARKER: char = 'y';

    fn run(_: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
        None
    }
}

fn probe_parser() -> MarkdownIt {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    markdown_it::plugins::cmark::inline::escape::add(&mut md);
    markdown_it::plugins::cmark::inline::backticks::add(&mut md);
    md.inline.add_rule::<ProbeSummaryRule>();
    md
}

fn mixed_probe_parser() -> MarkdownIt {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    markdown_it::plugins::cmark::inline::newline::add(&mut md);
    markdown_it::plugins::cmark::inline::escape::add(&mut md);
    markdown_it::plugins::cmark::inline::backticks::add(&mut md);
    markdown_it::plugins::cmark::inline::emphasis::add(&mut md);
    markdown_it::plugins::cmark::inline::entity::add(&mut md);
    markdown_it::plugins::cmark::inline::autolink::add(&mut md);
    markdown_it::plugins::html::html_inline::add(&mut md);
    md.inline.add_rule::<ProbeSummaryRule>();
    md
}

fn autolink_probe_parser(formatter: Box<dyn LinkFormatter>) -> MarkdownIt {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    markdown_it::plugins::cmark::inline::autolink::add(&mut md);
    md.inline.add_rule::<ProbeSummaryRule>();
    md.link_formatter = formatter;
    md
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
fn custom_rule_can_probe_ranges_through_public_interface() {
    let md = probe_parser();
    for (source, expected) in [
        ("{ab}", "<p>0..2=text;</p>\n"),
        ("{\\]x}", "<p>0..2=token;2..3=text;</p>\n"),
        ("{a\\]b}", "<p>0..1=text;1..3=token;3..4=text;</p>\n"),
        ("{a\\\nb}", "<p>0..1=text;1..3=token;3..4=text;</p>\n"),
        ("{\\雪}", "<p>0..4=token;</p>\n"),
        ("{\\ x}", "<p>0..1=text;1..3=text;</p>\n"),
        ("{a\\}", "<p>0..1=text;1..2=text;</p>\n"),
        ("{`x`}", "<p>0..3=token;</p>\n"),
        (
            "{```x``}",
            "<p>0..1=text;1..2=text;2..3=text;3..4=text;4..5=text;5..6=text;</p>\n",
        ),
    ] {
        let document = md.parse_document_direct(source).unwrap();
        assert_eq!(document.into_legacy().render(), expected, "{source}");
    }
}

#[test]
fn normal_direct_parsing_never_calls_probe() {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    md.inline.add_rule::<PanicProbeRule>();

    let document = md.parse_document_direct("x").unwrap();
    assert_eq!(document.into_legacy().render(), "<p>X</p>\n");
}

#[test]
fn default_probe_for_matching_marker_falls_back_to_text() {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    md.inline.add_rule::<ProbeSummaryRule>();
    md.inline.add_rule::<NoProbeRule>();

    let html = md
        .parse_document_direct("{y}")
        .unwrap()
        .into_legacy()
        .render();
    assert_eq!(html, "<p>0..1=text;</p>\n");

    let html = md
        .parse_document_direct("{x}")
        .unwrap()
        .into_legacy()
        .render();
    assert_eq!(html, "<p>0..1=text;</p>\n");
}

#[test]
fn probe_does_not_call_code_pair_factory() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use markdown_it::generics::inline::code_pair;

    static CALLS: AtomicUsize = AtomicUsize::new(0);

    fn factory(_: usize) -> NodeDraft {
        CALLS.fetch_add(1, Ordering::SeqCst);
        NodeDraft::new(Text {
            content: "pair".to_owned(),
        })
    }

    CALLS.store(0, Ordering::SeqCst);
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    code_pair::add_with::<'$'>(&mut md, factory);
    md.inline.add_rule::<ProbeSummaryRule>();

    let html = md
        .parse_document_direct("{$x$}")
        .unwrap()
        .into_legacy()
        .render();
    assert_eq!(html, "<p>0..3=token;</p>\n");
    assert_eq!(CALLS.load(Ordering::SeqCst), 0);

    md.parse_document_direct("$x$").unwrap();
    assert_eq!(CALLS.load(Ordering::SeqCst), 1);
}

#[test]
fn autolink_and_html_dispatch_by_registration_order() {
    let md = mixed_probe_parser();
    for (source, expected) in [
        ("{<https://example.com>}", "<p>0..21=token;</p>\n"),
        ("{<a>}", "<p>0..3=token;</p>\n"),
        ("{<foo@example.com>}", "<p>0..17=token;</p>\n"),
        ("{<javascript:alert(1)>}", "<p>0..1=text;1..21=text;</p>\n"),
    ] {
        let html = md
            .parse_document_direct(source)
            .unwrap()
            .into_legacy()
            .render();
        assert_eq!(html, expected, "{source}");
    }
}

#[test]
fn mixed_rules_probe_through_public_consumer() {
    let md = mixed_probe_parser();
    for (source, expected) in [
        (
            "{a &amp; &#91;\n<b>}",
            "<p>0..2=text;2..7=token;7..8=text;8..13=token;13..14=token;14..17=token;</p>\n",
        ),
        (
            "{*x* `y` \\*}",
            "<p>0..1=text;1..2=text;2..3=text;3..4=text;4..7=token;7..8=text;8..10=token;</p>\n",
        ),
    ] {
        let html = md
            .parse_document_direct(source)
            .unwrap()
            .into_legacy()
            .render();
        assert_eq!(html, expected, "{source}");
    }
}

#[test]
fn autolink_probe_uses_formatter_and_rejection_falls_back() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let md = autolink_probe_parser(Box::new(RecordingFormatter {
        calls: calls.clone(),
        reject: false,
    }));
    let html = md
        .parse_document_direct("{<https://example.com>}")
        .unwrap()
        .into_legacy()
        .render();
    assert_eq!(html, "<p>0..21=token;</p>\n");
    assert_eq!(
        *calls.lock().unwrap(),
        vec![
            "normalize:https://example.com".to_owned(),
            "validate:https://example.com".to_owned(),
            "text:https://example.com".to_owned(),
        ]
    );

    let calls = Arc::new(Mutex::new(Vec::new()));
    let md = autolink_probe_parser(Box::new(RecordingFormatter {
        calls: calls.clone(),
        reject: true,
    }));
    let html = md
        .parse_document_direct("{<https://example.com>}")
        .unwrap()
        .into_legacy()
        .render();
    assert_eq!(html, "<p>0..1=text;1..21=text;</p>\n");
    assert_eq!(
        *calls.lock().unwrap(),
        vec![
            "normalize:https://example.com".to_owned(),
            "validate:https://example.com".to_owned(),
        ]
    );

    let calls = Arc::new(Mutex::new(Vec::new()));
    let md = autolink_probe_parser(Box::new(RecordingFormatter {
        calls: calls.clone(),
        reject: false,
    }));
    let html = md
        .parse_document_direct("{<https://foo bar>}")
        .unwrap()
        .into_legacy()
        .render();
    assert_eq!(html, "<p>0..1=text;1..17=text;</p>\n");
    assert!(calls.lock().unwrap().is_empty());
}
