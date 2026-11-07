//! Inline counter plugin.
//!
//! Replaces every `@` with an increasing number. Parent and child inline
//! sessions share a counter through `shared_ext`, while separate parse calls
//! and separate paragraphs start again from `1`. Every match also registers
//! two ordered `on_finish` callbacks.
//!
//! # What it demonstrates
//!
//! - `check` can test whether a match exists without creating nodes or changing
//!   shared state.
//! - Extension state inserted with `shared_ext` is visible to both parent and
//!   child inline sessions.
//! - `on_finish` callbacks run in registration order and affect only their own
//!   inline session.
//!
//! # Test coverage
//!
//! - `explicit_check_does_not_generate_nodes_or_increment_shared_counter`:
//!   link parsing calls `check` without generating nodes or advancing the
//!   counter.
//! - The parent/child session and finish-callback behavior is exercised from
//!   the [`container`](super::container) module, which nests this rule inside a
//!   child inline session.

use markdown_it::MarkdownIt;
use markdown_it::document::{NodeId, Text};
use markdown_it::parser::inline::{InlineRule, InlineState};

#[derive(Debug, Default)]
struct Counter(usize);

#[derive(Debug, Default)]
pub(super) struct Finished(pub(super) Vec<u8>);

pub(super) struct Counted;

/// Register the counter rule on a parser.
pub fn install(md: &mut MarkdownIt) {
    md.inline.add_rule::<Counted>();
}

impl InlineRule for Counted {
    const MARKER: char = '@';

    fn check(state: &mut InlineState<'_>) -> Option<usize> {
        state.remaining().starts_with('@').then_some(1)
    }

    fn run(state: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        let len = Self::check(state)?;
        let count = {
            let mut shared = state.shared_ext();
            let counter = shared.get_or_insert_default::<Counter>();
            counter.0 += 1;
            counter.0
        };
        state.on_finish(|s| record(s, 1));
        state.on_finish(|s| record(s, 2));
        let node = state.document.create_node(Text {
            content: count.to_string(),
        });
        Some((Some(node), len))
    }
}

fn record(state: &mut InlineState<'_>, step: u8) {
    let first = state.nodes()[0];
    state
        .document
        .node_mut(first)
        .ext_mut()
        .get_or_insert_default::<Finished>()
        .0
        .push(step);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parser() -> MarkdownIt {
        let mut md = MarkdownIt::new();
        markdown_it::plugins::cmark::add(&mut md);
        install(&mut md);
        md
    }

    #[test]
    fn explicit_check_does_not_generate_nodes_or_increment_shared_counter() {
        assert_eq!(
            parser().render("[ @ ](/url) @"),
            "<p><a href=\"/url\"> 1 </a> 2</p>\n"
        );
    }
}
