//! Inline container plugin.
//!
//! Matches `{...}`, parses the body in a child inline session, and renders the
//! result as `<span>...</span>`. The body may use built-in Markdown rules as
//! well as the `@` rule from the [`counted`](super::counted) module.
//!
//! # What it demonstrates
//!
//! - `parse_subrange` creates a child session without moving the parent cursor.
//! - Parent and child sessions share extension state, but each parse call and
//!   paragraph starts with fresh state.
//! - `on_finish` callbacks run in registration order and stay local to the
//!   session that registered them.
//! - The nesting limit falls back to literal child source.
//! - Invalid subranges are rejected without breaking the parent session.
//! - Source maps are preserved for nodes created from child sessions.
//!
//! # Test coverage
//!
//! - `child_session_shares_source_cache_and_keeps_parent_cursor`: parent and
//!   child sessions share counter state; the parent cursor does not move;
//!   separate parses and paragraphs use independent counters.
//! - `finish_callbacks_run_in_order_and_stay_local_to_child_sessions`: callbacks
//!   run in registration order without attaching data to the container.
//! - `nesting_limit_emits_child_source_literally`: exceeding the nesting limit
//!   emits the child source as text.
//! - `subrange_preserves_whitespace_and_builtin_inline_rules`: subranges keep
//!   whitespace and support built-in emphasis and code rules.
//! - `invalid_subranges_leave_parent_usable_and_empty_ranges_are_valid`:
//!   out-of-bounds, reversed, and non-UTF-8-boundary ranges are rejected;
//!   empty ranges remain valid.
//! - `child_nodes_keep_original_utf8_source_coordinates` (with `sourcemap`):
//!   container and child nodes keep their original UTF-8 byte coordinates.

use markdown_it::MarkdownIt;
use markdown_it::document::{Node, NodeId};
use markdown_it::parser::inline::{InlineRule, InlineState};
use markdown_it::render::{NodeRenderer, RenderContext, Writer};

#[derive(Debug)]
struct Container;
struct ContainerRule;

impl InlineRule for ContainerRule {
    const MARKER: char = '{';

    fn run(state: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        if !state.remaining().starts_with('{') {
            return None;
        }

        let end = state.remaining().find('}')?;
        let children = state.parse_subrange(1..end)?;
        let node = state.document.create_node(Container);
        for child in children {
            state.document.push_child(node, child);
        }
        Some((Some(node), end + 1))
    }
}

struct ContainerRenderer;
impl NodeRenderer<Container> for ContainerRenderer {
    fn render(&self, node: &Node, _: &Container, ctx: &mut RenderContext<'_>, out: &mut Writer) {
        out.write_str("<span>");
        ctx.render_children(node.id(), out);
        out.write_str("</span>");
    }
}

/// Register the container rule and its HTML renderer.
/// Install `counted` separately to enable counters inside and outside containers.
pub fn install(md: &mut MarkdownIt) {
    md.inline.add_rule::<ContainerRule>();
    md.add_renderer::<Container, _>("html", ContainerRenderer);
}

#[cfg(test)]
mod tests {
    use markdown_it::document::Document;

    use super::super::counted::Finished;
    use super::*;

    fn parser() -> MarkdownIt {
        let mut md = MarkdownIt::new();
        markdown_it::plugins::cmark::add(&mut md);
        super::super::counted::install(&mut md);
        install(&mut md);
        md
    }

    fn first(doc: &Document, parent: NodeId) -> NodeId {
        doc.children(parent)[0]
    }

    #[test]
    fn child_session_shares_source_cache_and_keeps_parent_cursor() {
        let md = parser();
        assert_eq!(md.render("@{ @ }@"), "<p>1<span> 2 </span>3</p>\n");
        // Source-wide scratch is not retained by the reusable parser.
        assert_eq!(md.render("@"), "<p>1</p>\n");
        assert_eq!(md.render("@\n\n@"), "<p>1</p>\n<p>1</p>\n");
    }

    #[test]
    fn finish_callbacks_run_in_order_and_stay_local_to_child_sessions() {
        let doc = parser().parse("@{@}");
        let paragraph = first(&doc, doc.root());
        let parent_text = first(&doc, paragraph);
        let container = doc.children(paragraph)[1];
        let child_text = first(&doc, container);
        for node in [parent_text, child_text] {
            assert_eq!(doc.node(node).ext().get::<Finished>().unwrap().0, [1, 2]);
        }
        assert!(doc.node(container).ext().get::<Finished>().is_none());
    }

    #[test]
    fn nesting_limit_emits_child_source_literally() {
        let mut md = parser();
        md.max_nesting = 1;
        assert_eq!(md.render("{@ *雪*}"), "<p><span>@ *雪*</span></p>\n");
    }

    #[test]
    fn subrange_preserves_whitespace_and_builtin_inline_rules() {
        assert_eq!(
            parser().render("前{ *雪* `x` }尾"),
            "<p>前<span> <em>雪</em> <code>x</code> </span>尾</p>\n"
        );
    }

    #[test]
    fn invalid_subranges_leave_parent_usable_and_empty_ranges_are_valid() {
        struct Probe;
        impl InlineRule for Probe {
            const MARKER: char = '{';

            fn run(state: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
                let source = state.remaining().to_owned();
                assert!(
                    state
                        .parse_subrange(source.len() + 1..source.len() + 1)
                        .is_none()
                );
                if source.starts_with("{雪") {
                    assert!(
                        state.parse_subrange(2..3).is_none(),
                        "UTF-8 boundary inside 雪"
                    );
                }
                let reversed = source.len()..0;
                assert!(state.parse_subrange(reversed).is_none());
                assert!(state.parse_subrange(1..1).unwrap().is_empty());
                let result = ContainerRule::run(state);
                assert_eq!(state.remaining(), source, "child must not advance parent");
                result
            }
        }
        let mut md = MarkdownIt::new();
        markdown_it::plugins::cmark::add(&mut md);
        md.inline.add_rule::<Probe>();
        md.add_renderer::<Container, _>("html", ContainerRenderer);
        assert_eq!(
            md.render("前{雪}{}尾"),
            "<p>前<span>雪</span><span></span>尾</p>\n"
        );
    }

    #[cfg(feature = "sourcemap")]
    #[test]
    fn child_nodes_keep_original_utf8_source_coordinates() {
        let doc = parser().parse("前{雪}尾");
        let paragraph = first(&doc, doc.root());
        let container = doc.children(paragraph)[1];
        let child = first(&doc, container);
        assert_eq!(
            doc.node(container).srcmap().unwrap().get_byte_offsets(),
            (3, 8)
        );
        assert_eq!(doc.node(child).srcmap().unwrap().get_byte_offsets(), (4, 7));
    }
}
