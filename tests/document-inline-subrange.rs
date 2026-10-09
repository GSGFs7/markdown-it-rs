use markdown_it::parser::inline::InlineRule;
use markdown_it::{DocumentInlineState, MarkdownIt, NodeId, NodeValue};

#[derive(Debug)]
struct TestContainer;
impl NodeValue for TestContainer {}

struct ContainerRule;
impl InlineRule for ContainerRule {
    const MARKER: char = '{';

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
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
        let node = state.document.create_node(TestContainer);
        let children = state.parse_subrange(1..close)?;
        for child in children {
            state.document.push_child(node, child);
        }
        Some((Some(node), close + 1))
    }
}

fn parser() -> MarkdownIt {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    markdown_it::plugins::cmark::inline::emphasis::add(&mut md);
    markdown_it::plugins::cmark::inline::backticks::add(&mut md);
    md.inline.add_rule::<ContainerRule>();
    md.add_document_renderer::<TestContainer, _>("html", ContainerRenderer);
    md
}

#[test]
fn custom_rule_parses_nested_children_without_losing_parent_text() {
    let md = parser();
    let document = md.parse_document("前{ 雪 {*x*} }尾 {}");
    assert_eq!(
        md.render_document(&document),
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
        assert_eq!(md.render(source), expected, "{source}");
    }
}

#[test]
fn nesting_limit_stops_recursive_container_rules() {
    let mut md = parser();
    md.max_nesting = 2;
    let document = md.parse_document("{ {*x*} *y* }");
    assert_eq!(
        md.render_document(&document),
        "<p>{ {*x*} <em>y</em> }</p>\n"
    );
}

struct ContainerRenderer;
impl markdown_it::DocumentNodeRenderer<TestContainer> for ContainerRenderer {
    fn render(
        &self,
        node: markdown_it::NodeRef<'_>,
        _: &TestContainer,
        ctx: &mut markdown_it::DocumentRenderContext<'_>,
        out: &mut markdown_it::DocumentWriter,
    ) {
        out.write_str("{");
        ctx.render_children(node.id(), out);
        out.write_str("}");
    }
}
