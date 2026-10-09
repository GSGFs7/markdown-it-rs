use std::sync::atomic::{AtomicUsize, Ordering};

use markdown_it::parser::inline::InlineRule;
use markdown_it::{DocumentInlineState, MarkdownIt, NodeId, Text};

// A plugin only implementing run must still be opaque inside a link label.
struct RunOnly;
impl InlineRule for RunOnly {
    const MARKER: char = '%';

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        state.remaining().starts_with("%]%").then(|| {
            (
                Some(state.document.create_node(Text {
                    content: "opaque".into(),
                })),
                3,
            )
        })
    }
}

#[test]
fn default_check_uses_run_to_preserve_custom_spans_in_link_labels() {
    let mut md = MarkdownIt::new();
    markdown_it::plugins::cmark::add(&mut md);
    md.inline.add_rule::<RunOnly>();
    assert_eq!(
        md.render("[a %]% b](/url)"),
        "<p><a href=\"/url\">a opaque b</a></p>\n"
    );
}

static RUNS: AtomicUsize = AtomicUsize::new(0);

struct ExplicitCheck;
impl InlineRule for ExplicitCheck {
    const MARKER: char = '%';

    fn check(state: &mut DocumentInlineState<'_>) -> Option<usize> {
        state.remaining().starts_with("%]%").then_some(3)
    }

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        let len = Self::check(state)?;
        RUNS.fetch_add(1, Ordering::SeqCst);
        Some((
            Some(state.document.create_node(Text {
                content: "opaque".into(),
            })),
            len,
        ))
    }
}

#[test]
fn explicit_check_does_not_execute_run_during_label_scanning() {
    RUNS.store(0, Ordering::SeqCst);
    let mut md = MarkdownIt::new();
    markdown_it::plugins::cmark::add(&mut md);
    md.inline.add_rule::<ExplicitCheck>();
    assert_eq!(
        md.render("[a %]% b](/url)"),
        "<p><a href=\"/url\">a opaque b</a></p>\n"
    );
    assert_eq!(RUNS.load(Ordering::SeqCst), 1);
}

struct PanicCheck;
impl InlineRule for PanicCheck {
    const MARKER: char = 'x';

    fn check(_: &mut DocumentInlineState<'_>) -> Option<usize> {
        panic!("normal parsing must not call check")
    }

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        Some((
            Some(state.document.create_node(Text {
                content: "X".into(),
            })),
            1,
        ))
    }
}

#[test]
fn normal_parsing_calls_run_directly() {
    let mut md = MarkdownIt::new();
    markdown_it::plugins::cmark::add(&mut md);
    md.inline.add_rule::<PanicCheck>();
    assert_eq!(md.render("x"), "<p>X</p>\n");
}
