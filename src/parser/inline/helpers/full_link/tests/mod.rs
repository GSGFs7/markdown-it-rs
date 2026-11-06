use super::cache::{is_memoizable_check, plain_label_closes};
use super::candidate::scan_link_candidate;
use super::label::scan_link_label;
use super::reference::resolve_reference_link;
use super::target::{InlineLinkTarget, scan_inline_link_target, skip_link_spaces};
use super::*;
use crate::MarkdownIt;
use crate::document::{NodeId, Text};
use crate::parser::inline::{InlineRule, InlineState};
use crate::plugins::cmark::block::reference::ReferenceMap;
use crate::plugins::cmark::inline::link::Link;

// Register both brackets so the built-in text classifier stops at them.
struct Bracket<const C: char>;
impl<const C: char> InlineRule for Bracket<C> {
    const MARKER: char = C;
    fn check(_: &mut InlineState<'_>) -> Option<usize> {
        None
    }
    fn run(_: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        panic!("the consumer must consume the complete test input")
    }
}

struct Consumer<const NESTED: bool>;
impl<const NESTED: bool> InlineRule for Consumer<NESTED> {
    const MARKER: char = '@';
    fn run(state: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        if !state.remaining().starts_with("@[") {
            return None;
        }
        let len = state.remaining().len();
        let expected_source = state.remaining()[1..].to_owned();
        let mut parent = state.child_state(1..len).unwrap();
        let result = parent
            .child_state(1..parent.remaining().len())
            .and_then(|child| scan_link_label(child, NESTED));
        assert_eq!(parent.remaining(), expected_source);
        assert_eq!(parent.trailing_text(), "");
        assert_eq!(parent.link_level, 0);
        let content = match result {
            Some(end) => format!("end={end}"),
            None => "none".to_owned(),
        };
        Some((Some(state.document.create_node(Text { content })), len))
    }
}

fn parser<const NESTED: bool>() -> MarkdownIt {
    let mut md = MarkdownIt::empty();
    crate::plugins::cmark::block::paragraph::add(&mut md);
    md.inline
        .add_rule::<crate::parser::inline::TextScanner>()
        .before_all();
    md.inline.add_rule::<Bracket<'['>>();
    md.inline.add_rule::<Bracket<']'>>();
    md.inline.add_rule::<Consumer<NESTED>>();
    crate::plugins::cmark::inline::escape::add(&mut md);
    crate::plugins::cmark::inline::backticks::add(&mut md);
    crate::plugins::cmark::inline::entity::add(&mut md);
    crate::plugins::html::html_inline::add(&mut md);
    md
}

fn render(md: &MarkdownIt, source: &str) -> String {
    md.render(source)
}

/// Label bodies shared by the raw-label and combined-candidate tests.
///
/// A `]` inside a code span, escape, entity, or raw HTML must not close
/// the label; raw nested `[` bytes still increase the bracket level.
const LABELS: &[&str] = &[
    "",
    "abc",
    "雪[a]雨",
    "`]`",
    r"\]",
    "&#93;",
    "<i x=']'>",
    "<a>x",
];

struct DefaultCheck;
impl InlineRule for DefaultCheck {
    const MARKER: char = '?';
    fn run(_: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        None
    }
}

struct Closing;
impl InlineRule for Closing {
    const MARKER: char = ']';
    fn check(_: &mut InlineState<'_>) -> Option<usize> {
        panic!("terminal bracket must be checked before dispatch")
    }
    fn run(_: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        panic!("consumer owns input")
    }
}

struct BracketToken;
impl InlineRule for BracketToken {
    const MARKER: char = '[';
    fn check(context: &mut InlineState<'_>) -> Option<usize> {
        if context.remaining().starts_with("[x]") {
            // Legacy uses consumed length, even if a custom rule calls it Text.
            Some(3)
        } else {
            None
        }
    }
    fn run(_: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        panic!("consumer owns input")
    }
}

fn utf8_parser<const PREFIX: char>() -> MarkdownIt {
    let mut md = MarkdownIt::empty();
    crate::plugins::cmark::add(&mut md);
    super::add_prefix::<PREFIX, true>(&mut md, |document, href, title| {
        document.create_node(Link {
            url: href.unwrap_or_default(),
            title,
        })
    });
    md
}

mod cache;
mod candidate;
mod label;
mod reference;
mod registration;
mod target;
