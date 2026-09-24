use std::fmt::Write;

use super::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    DocumentRenderError,
    DocumentRendererRegistry,
    DocumentWriter,
};
use crate::parser::inline::Text;
use crate::{Document, MarkdownIt, Node, NodeRef, NodeValue, RenderOptions};

#[derive(Debug)]
struct UnknownContainer;
impl NodeValue for UnknownContainer {}

#[derive(Debug)]
struct UnknownLeaf(&'static str);
impl NodeValue for UnknownLeaf {}

struct UnknownLeafRenderer(&'static str);

impl DocumentNodeRenderer<UnknownLeaf> for UnknownLeafRenderer {
    fn render(
        &self,
        _: NodeRef<'_>,
        value: &UnknownLeaf,
        _: &mut DocumentRenderContext<'_>,
        output: &mut DocumentWriter,
    ) -> Result<(), DocumentRenderError> {
        write!(output, "{}:{}", self.0, value.0)?;
        Ok(())
    }
}

struct DirectWriteAndCrRenderer;

impl DocumentNodeRenderer<UnknownLeaf> for DirectWriteAndCrRenderer {
    fn render(
        &self,
        _: NodeRef<'_>,
        _: &UnknownLeaf,
        context: &mut DocumentRenderContext<'_>,
        output: &mut DocumentWriter,
    ) -> Result<(), DocumentRenderError> {
        output.write_str("first")?;
        context.cr(output)?;
        context.cr(output)?;
        output.write_str("second\n")?;
        context.cr(output)
    }
}

#[test]
fn renders_minimal_html_directly_without_consuming_document() {
    let md = MarkdownIt::empty();
    let document = md.parse_document("hello <world>");

    assert_eq!(
        md.render_document(&document).unwrap(),
        "hello &lt;world&gt;\n"
    );
    assert_eq!(
        md.render_document_as(&document, "text").unwrap(),
        "hello <world>\n"
    );
    assert_eq!(
        md.render_document(&document).unwrap(),
        "hello &lt;world&gt;\n"
    );

    let nul = md.parse_document("\0");
    assert_eq!(md.render_document(&nul).unwrap(), "\u{FFFD}\n");
    assert_eq!(md.render_document_as(&nul, "text").unwrap(), "\u{FFFD}\n");
}

#[test]
fn cr_observes_direct_renderer_writes_without_duplicate_line_endings() {
    let mut registry = DocumentRendererRegistry::new();
    registry.add::<UnknownLeaf, _>("html", DirectWriteAndCrRenderer);
    let document = Document::from_legacy("", Node::new(UnknownLeaf("unused")));

    assert_eq!(
        registry
            .render(&document, "html", &RenderOptions::default())
            .unwrap(),
        "first\nsecond\n"
    );
}

#[test]
fn html_attrs_preserve_grouping_order_and_escaping_on_both_paths() {
    let small = vec![
        ("class".into(), "first".into()),
        ("id".into(), "one".into()),
        ("class".into(), "second".into()),
        ("style".into(), "color:<red>".into()),
        ("title".into(), "<&>".into()),
        ("style".into(), "display:block".into()),
        ("id".into(), "two".into()),
    ];
    let mut output = DocumentWriter::new();
    super::write_html_attrs(&mut output, &small).unwrap();
    assert_eq!(
        output.finish(),
        " class=\"first second\" id=\"one\" id=\"two\" style=\"color:&lt;red&gt;;display:block\" title=\"&lt;&amp;&gt;\""
    );

    let mut large = small;
    large.extend([("data-a".into(), "a".into()), ("data-b".into(), "b".into())]);
    let mut output = DocumentWriter::new();
    super::write_html_attrs(&mut output, &large).unwrap();
    assert_eq!(
        output.finish(),
        " class=\"first second\" id=\"one\" id=\"two\" style=\"color:&lt;red&gt;;display:block\" title=\"&lt;&amp;&gt;\" data-a=\"a\" data-b=\"b\""
    );
}

#[test]
fn registered_paragraph_renderer_preserves_attributes() {
    let md = MarkdownIt::new();
    let mut root = md.parse("hello");
    root.children[0].attrs.extend([
        ("class".into(), "one".into()),
        ("class".into(), "two".into()),
    ]);
    let document = Document::from_legacy("hello", root);

    assert_eq!(
        md.render_document(&document).unwrap(),
        "<p class=\"one two\">hello</p>\n"
    );
}

#[test]
fn commonmark_block_renderers_match_legacy_html() {
    let sources = [
        "# atx\n\nsetext\n------\n",
        "> quoted\n>\n> second\n",
        "> [label]: /destination\n",
        "1. first\n2. second\n\n7. seven\n",
        "- outer\n  - inner\n",
        "---\n",
        "    <indented> & code\n",
        "```rust extra\nfn main() { <tag> }\n```\n",
        "[label]: /destination \"title\"\n",
    ];

    for mut md in [
        MarkdownIt::new(),
        MarkdownIt::with_preset(crate::Preset::CommonMark),
    ] {
        md.render_options.lang_prefix = Some("lang-".into());
        for source in sources {
            let expected = md.parse(source).render();
            let document = md.parse_document(source);
            assert_eq!(
                md.render_document(&document).unwrap(),
                expected,
                "direct renderer differs for {source:?} with options {:?}",
                md.render_options
            );
        }
    }
}

#[test]
fn block_renderers_preserve_document_transform_attributes() {
    let source = "# heading\n\n> quote\n\n3. item\n\n---\n\n    code\n\n```rs\nfenced\n```\n";

    let mut legacy = MarkdownIt::empty();
    crate::plugins::cmark::add(&mut legacy);
    crate::plugins::sourcepos::add(&mut legacy);
    let expected = legacy.render(source);

    let mut direct = MarkdownIt::empty();
    crate::plugins::cmark::add(&mut direct);
    crate::plugins::sourcepos::add_document(&mut direct);
    let mut document = direct.parse_document(source);
    direct.run_document_transforms(&mut document).unwrap();

    assert_eq!(direct.render_document(&document).unwrap(), expected);
}

#[test]
fn commonmark_inline_renderers_match_legacy_html() {
    let sources = [
        "plain *em **strong** text* end",
        "`<code> & value`",
        "soft\nbreak and hard  \nbreak",
        "[label *em*](https://example.com/?a=1&b=2 \"a title\")",
        "![alt *em* <b>raw</b>](image.png \"image title\")",
        "<https://example.com/?a=1&b=2> <hello@example.com>",
    ];

    for xhtml_out in [false, true] {
        for breaks in [false, true] {
            let mut md = MarkdownIt::empty();
            crate::plugins::cmark::add(&mut md);
            crate::plugins::html::add(&mut md);
            md.render_options.xhtml_out = xhtml_out;
            md.render_options.breaks = breaks;

            for source in sources {
                let expected = md.parse(source).render();
                let document = md.parse_document(source);
                assert_eq!(
                    md.render_document(&document).unwrap(),
                    expected,
                    "direct renderer differs for {source:?} with options {:?}",
                    md.render_options
                );
            }
        }
    }
}

#[test]
fn inline_renderers_preserve_document_transform_attributes() {
    let source = "**strong** [link](https://example.com) ![alt](image.png)  \nnext";

    let mut legacy = MarkdownIt::empty();
    crate::plugins::cmark::add(&mut legacy);
    crate::plugins::sourcepos::add(&mut legacy);
    let expected = legacy.render(source);

    let mut direct = MarkdownIt::empty();
    crate::plugins::cmark::add(&mut direct);
    crate::plugins::sourcepos::add_document(&mut direct);
    let mut document = direct.parse_document(source);
    direct.run_document_transforms(&mut document).unwrap();

    assert_eq!(direct.render_document(&document).unwrap(), expected);
}

#[test]
fn unknown_container_transparently_renders_children() {
    let md = MarkdownIt::empty();
    let mut root = Node::new(UnknownContainer);
    root.children.push(Node::new(Text {
        content: "child".into(),
    }));
    let document = Document::from_legacy("", root);

    assert_eq!(md.render_document(&document).unwrap(), "child");
    assert_eq!(md.render_document_as(&document, "text").unwrap(), "child");
}

#[test]
fn unknown_leaf_returns_structured_error() {
    let md = MarkdownIt::empty();
    let document = Document::from_legacy("", Node::new(UnknownLeaf("value")));

    assert!(matches!(
        md.render_document(&document),
        Err(DocumentRenderError::MissingRenderer {
            format,
            node_name,
            ..
        }) if format == "html" && node_name == std::any::type_name::<UnknownLeaf>()
    ));
}

#[test]
fn custom_renderer_can_be_registered_and_overridden_per_format() {
    let document = Document::from_legacy("", Node::new(UnknownLeaf("value")));
    let mut registry = DocumentRendererRegistry::new();

    assert!(!registry.add::<UnknownLeaf, _>("plain", UnknownLeafRenderer("first")));
    assert!(registry.contains::<UnknownLeaf>("plain"));
    assert_eq!(
        registry
            .render(&document, "plain", &RenderOptions::default())
            .unwrap(),
        "first:value"
    );

    assert!(registry.add::<UnknownLeaf, _>("plain", UnknownLeafRenderer("second")));
    assert_eq!(
        registry
            .render(&document, "plain", &RenderOptions::default())
            .unwrap(),
        "second:value"
    );
    assert!(registry.remove::<UnknownLeaf>("plain"));
    assert!(!registry.contains::<UnknownLeaf>("plain"));
}

#[test]
fn markdown_it_selects_and_isolates_renderer_formats() {
    let document = Document::from_legacy("", Node::new(UnknownLeaf("value")));
    let mut md = MarkdownIt::empty();
    md.add_document_renderer::<UnknownLeaf, _>("html", UnknownLeafRenderer("html"));
    md.add_document_renderer::<UnknownLeaf, _>("text", UnknownLeafRenderer("text"));

    assert_eq!(md.render_document(&document).unwrap(), "html:value");
    assert_eq!(
        md.render_document_as(&document, "text").unwrap(),
        "text:value"
    );

    md.add_document_renderer::<UnknownLeaf, _>("text", UnknownLeafRenderer("override"));
    assert_eq!(md.render_document(&document).unwrap(), "html:value");
    assert_eq!(
        md.render_document_as(&document, "text").unwrap(),
        "override:value"
    );

    assert!(matches!(
        md.render_document_as(&document, "missing"),
        Err(DocumentRenderError::MissingRenderer {
            format,
            node_name,
            ..
        }) if format == "missing" && node_name == std::any::type_name::<UnknownLeaf>()
    ));
}
