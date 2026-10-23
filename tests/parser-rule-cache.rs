use markdown_it::parser::block::BlockRule;
use markdown_it::parser::core::{CoreRule, CoreState};
use markdown_it::parser::inline::{InlineParserRule, InlineRule};
use markdown_it::{BlockState, InlineState, MarkdownIt, NodeId, Text};

struct ConsumeBlock;
impl BlockRule for ConsumeBlock {
    fn run(state: &mut BlockState<'_>) -> Option<(Option<NodeId>, usize)> {
        (state.get_line(state.line) == "hide").then_some((None, 1))
    }
}

// A wildcard rule exercises invalidation independently of marker-table changes.
struct Replace<const MARKER: char, const SECOND: bool>;
impl<const MARKER: char, const SECOND: bool> InlineRule for Replace<MARKER, SECOND> {
    const MARKER: char = MARKER;

    fn run(state: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        let ch = state.remaining().chars().next()?;
        if ch != '%' && ch != '雪' {
            return None;
        }
        let node = state.document.create_node(Text {
            content: if SECOND { "second" } else { "first" }.into(),
        });
        Some((Some(node), ch.len_utf8()))
    }
}

#[derive(Default)]
struct CoreFinalize;
impl CoreRule for CoreFinalize {
    fn run(&self, state: &mut CoreState<'_>) {
        let document = &mut *state.document;
        let root = document.root();
        document
            .node_mut(root)
            .attrs_mut()
            .push(("core".into(), "yes".into()));
    }
}

#[test]
fn warmed_block_rules_follow_registration_order_and_removal() {
    let mut md = MarkdownIt::new();
    assert_eq!(md.render("hide"), "<p>hide</p>\n");
    md.block.add_rule::<ConsumeBlock>().before_all();
    assert_eq!(md.render("hide"), "");
    md.block.remove_rule::<ConsumeBlock>();
    assert_eq!(md.render("hide"), "<p>hide</p>\n");
}

#[test]
fn warmed_inline_rules_update_markers_and_order() {
    let mut md = MarkdownIt::empty();
    assert_eq!(md.render("a%雪"), "a%雪\n");
    md.inline.add_rule::<Replace<'%', false>>();
    let document = md.parse("a%雪");
    assert_eq!(md.render_html(&document), "afirst雪\n");

    // Unicode markers must rebuild the text scanner as well as the rule set.
    md.inline.add_rule::<Replace<'雪', false>>();
    let document = md.parse("a%雪");
    assert_eq!(md.render_html(&document), "afirstfirst\n");

    md.inline.add_rule::<Replace<'\0', true>>().before_all();
    assert_eq!(md.render("%雪"), "secondsecond\n");
    md.inline.remove_rule::<Replace<'\0', true>>();
    assert_eq!(md.render("%雪"), "firstfirst\n");
    md.inline.remove_rule::<Replace<'%', false>>();
    md.inline.remove_rule::<Replace<'雪', false>>();
    let document = md.parse("a%雪");
    assert_eq!(md.render_html(&document), "a%雪\n");
    assert!(document.node(document.root()).attrs().is_empty());
}

#[test]
fn warmed_core_ruler_revalidates_explicit_dependencies_after_changes() {
    let mut md = MarkdownIt::new();
    md.parse("text");
    md.add_rule::<CoreFinalize>()
        .after::<InlineParserRule>()
        .require::<InlineParserRule>();
    let document = md.parse("text");
    assert_eq!(document.node(document.root()).attrs().len(), 1);
    md.remove_rule::<InlineParserRule>();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| md.parse("text"))).is_err());
    md.add_rule::<InlineParserRule>().before::<CoreFinalize>();
    let document = md.parse("text");
    assert_eq!(document.node(document.root()).attrs().len(), 1);
    md.remove_rule::<CoreFinalize>();
    let document = md.parse("text");
    assert!(document.node(document.root()).attrs().is_empty());
}

#[test]
fn warmed_parser_observes_runtime_settings_and_replaced_engines() {
    let mut md = MarkdownIt::new();
    assert_eq!(md.render("**x**"), "<p><strong>x</strong></p>\n");
    md.max_nesting = 0;
    assert_eq!(md.render("**x**"), "");
    md.max_nesting = 100;
    let empty = MarkdownIt::empty();
    md.block = empty.block;
    md.inline = empty.inline;
    assert_eq!(md.render("**x**"), "**x**\n");
}
