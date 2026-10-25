use markdown_it::{
    MarkdownIt,
    NodeRef,
    NodeRenderer,
    NodeValue,
    PluginSpec,
    RenderContext,
    Writer,
};

#[derive(Debug)]
struct Label(String);

struct LabelRenderer;

impl NodeRenderer<Label> for LabelRenderer {
    fn render(&self, _: NodeRef<'_>, value: &Label, _: &mut RenderContext<'_>, out: &mut Writer) {
        out.write_str(&value.0);
    }
}

struct Literal(&'static str);

impl<T: NodeValue> NodeRenderer<T> for Literal {
    fn render(&self, _: NodeRef<'_>, _: &T, _: &mut RenderContext<'_>, out: &mut Writer) {
        out.write_str(self.0);
    }
}

#[test]
fn inline_renderers_preserve_formats_and_override_in_registration_order() {
    for custom_last in [false, true] {
        // Register before the parser to verify that parser configuration keeps
        // every renderer, and mix shortcuts with the generic registration API.
        let leaf = PluginSpec::new("label")
            .inline_leaf::<Label>("label")
            .render_with("text", LabelRenderer)
            .marker('@')
            .parse(|cx| cx.consume("@").then(|| Label("<&>".into())));
        let leaf = if custom_last {
            leaf.render_html(|label, html| {
                html.text(&label.0);
            })
            .render_with("html", Literal("custom"))
        } else {
            leaf.render_with("html", Literal("custom"))
                .render_html(|label, html| {
                    html.text(&label.0);
                })
        };
        let mut md = MarkdownIt::builder().plugin(leaf.finish()).build();
        let document = md.parse("@");
        assert_eq!(
            md.render_html(&document),
            if custom_last {
                "<p>custom</p>\n"
            } else {
                "<p>&lt;&amp;&gt;</p>\n"
            }
        );
        assert_eq!(md.render_as(&document, "text"), "<&>\n");

        // The installed renderer is in the ordinary registry and can still be
        // replaced through the low-level API without affecting other formats.
        md.add_renderer::<Label, _>("html", Literal("replaced"));
        assert_eq!(md.render_html(&document), "<p>replaced</p>\n");
        assert_eq!(md.render_as(&document, "text"), "<&>\n");
    }
}

#[test]
fn leaf_can_register_only_a_non_html_renderer() {
    let plugin = PluginSpec::new("text-only")
        .inline_leaf::<Label>("label")
        .marker('@')
        .parse(|cx| cx.consume("@").then(|| Label("plain".into())))
        .render_with("text", LabelRenderer)
        .render_with(String::from("labels"), LabelRenderer)
        .finish();
    let md = MarkdownIt::builder().plugin(plugin).build();
    assert_eq!(md.render_as(&md.parse("@"), "text"), "plain\n");
    assert_eq!(md.render_as(&md.parse("@"), "labels"), "plain");
    assert!(!md.renderers.contains::<Label>("html"));
}

#[test]
fn block_custom_renderer_controls_output_without_automatic_line_breaks() {
    let leaf = PluginSpec::new("block")
        .block_leaf::<Label>("block")
        .render_with("html", Literal("first"))
        .render_with("text", LabelRenderer)
        .parse(|cx| {
            let content = cx.line(0)?.strip_prefix("!!! ")?.to_owned();
            cx.advance_lines(1);
            Some(Label(content))
        })
        .render_html(|label, html| {
            html.element("aside").text(&label.0);
        })
        .render_with("html", Literal("custom"))
        .finish();
    let md = MarkdownIt::builder().plugin(leaf).build();
    let document = md.parse("!!! payload");
    assert_eq!(md.render_html(&document), "custom");
    assert_eq!(md.render_as(&document, "text"), "payload");
}

struct PairRenderer;

#[derive(Debug, Default)]
struct Visits(usize);

impl NodeRenderer<Label> for PairRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        value: &Label,
        cx: &mut RenderContext<'_>,
        out: &mut Writer,
    ) {
        let visits = cx.ext().get_or_insert_default::<Visits>();
        visits.0 += 1;
        // Render state is new for each render, while the document, node
        // attributes, payload and active format are passed through unchanged.
        assert_eq!(visits.0, 1);
        assert_eq!(
            cx.document().node(node.id()).cast::<Label>().unwrap().0,
            value.0
        );
        if cx.format() == "html" {
            out.write_str("<span class=\"");
            out.write_str(&markdown_it::common::utils::escape_html(&value.0));
            out.write_str("\" data-id=\"");
            out.write_str(&markdown_it::common::utils::escape_html(&node.attrs()[0].1));
            out.write_str("\">");
            cx.render_children(node.id(), out);
            out.write_str("</span>");
        } else {
            out.write_str("[");
            cx.render_children(node.id(), out);
            out.write_str("]");
        }
    }
}

#[test]
fn pair_renderer_receives_payload_node_context_and_children_in_each_format() {
    let plugin = PluginSpec::new("spoiler")
        .inline_pair::<Label>("spoiler")
        .render_with("text", PairRenderer)
        .value(|| Label("spoiler&more".into()))
        .delimiter::<'=', 2, true>()
        .html_tag("mark")
        .render_with("html", PairRenderer)
        .finish();
    let md = MarkdownIt::builder().plugin(plugin).build();
    let mut document = md.parse("==secret **bold**==");
    let paragraph = document.children(document.root())[0];
    let pair = document.children(paragraph)[0];
    document
        .node_mut(pair)
        .attrs_mut()
        .push(("data-id".into(), "n&1".into()));

    assert_eq!(
        md.render_html(&document),
        "<p><span class=\"spoiler&amp;more\" data-id=\"n&amp;1\">secret <strong>bold</strong></span></p>\n"
    );
    assert_eq!(md.render_as(&document, "text"), "[secret bold]\n");
}

#[test]
fn pair_html_tag_can_replace_a_custom_html_renderer_and_keep_other_formats() {
    let plugin = PluginSpec::new("pair")
        .inline_pair::<Label>("pair")
        .delimiter::<'=', 2, true>()
        .value(|| Label("unused".into()))
        .render_with("html", Literal("old"))
        .render_with("text", Literal("plain"))
        .html_tag("mark")
        .finish();
    let md = MarkdownIt::builder().plugin(plugin).build();
    let document = md.parse("==content==");
    assert_eq!(md.render_html(&document), "<p><mark>content</mark></p>\n");
    assert_eq!(md.render_as(&document, "text"), "plain\n");
}
