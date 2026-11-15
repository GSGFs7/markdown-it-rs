use super::candidate::scan_link_candidate;
use super::target::{InlineLinkTarget, scan_inline_link_target, skip_link_spaces};
use super::*;
use crate::MarkdownIt;
use crate::document::NodeId;
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

fn parser() -> MarkdownIt {
    let mut md = MarkdownIt::empty();
    crate::plugins::cmark::block::paragraph::add(&mut md);
    md.inline
        .add_rule::<crate::parser::inline::TextScanner>()
        .before_all();
    md.inline.add_rule::<Bracket<'['>>();
    md.inline.add_rule::<Bracket<']'>>();
    crate::plugins::cmark::inline::escape::add(&mut md);
    crate::plugins::cmark::inline::backticks::add(&mut md);
    crate::plugins::cmark::inline::entity::add(&mut md);
    crate::plugins::html::html_inline::add(&mut md);
    md
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

mod candidate;
mod registration;
mod target;
