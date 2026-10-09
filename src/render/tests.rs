use std::panic::{AssertUnwindSafe, catch_unwind};

use super::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    DocumentRendererRegistry,
    DocumentWriter,
};
use crate::document::Text;
use crate::{Document, MarkdownIt, NodeDraft, NodeRef, NodeValue, RenderOptions};

#[derive(Debug)]
struct UnknownContainer;
impl NodeValue for UnknownContainer {}

#[derive(Debug)]
struct UnknownLeaf(&'static str);
impl NodeValue for UnknownLeaf {}

#[derive(Debug)]
struct FailingDisplay;

impl std::fmt::Display for FailingDisplay {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Err(std::fmt::Error)
    }
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else {
        "<non-string panic payload>".to_owned()
    }
}

struct UnknownLeafRenderer(&'static str);

impl DocumentNodeRenderer<UnknownLeaf> for UnknownLeafRenderer {
    fn render(
        &self,
        _: NodeRef<'_>,
        value: &UnknownLeaf,
        _: &mut DocumentRenderContext<'_>,
        output: &mut DocumentWriter,
    ) {
        write!(output, "{}:{}", self.0, value.0);
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
    ) {
        output.write_str("first");
        context.cr(output);
        context.cr(output);
        output.write_str("second\n");
        context.cr(output);
    }
}

#[test]
fn renders_minimal_html_directly_without_consuming_document() {
    let md = MarkdownIt::empty();
    let document = md.parse_document("hello <world>");

    assert_eq!(md.render_document(&document), "hello &lt;world&gt;\n");
    assert_eq!(md.render_document_as(&document, "text"), "hello <world>\n");
    assert_eq!(md.render_document(&document), "hello &lt;world&gt;\n");

    let nul = md.parse_document("\0");
    assert_eq!(md.render_document(&nul), "\u{FFFD}\n");
    assert_eq!(md.render_document_as(&nul, "text"), "\u{FFFD}\n");
}

#[test]
fn cr_observes_direct_renderer_writes_without_duplicate_line_endings() {
    let mut registry = DocumentRendererRegistry::new();
    registry.add::<UnknownLeaf, _>("html", DirectWriteAndCrRenderer);
    let document = Document::from_draft("", NodeDraft::new(UnknownLeaf("unused")));

    assert_eq!(
        registry.render(&document, "html", &RenderOptions::default()),
        "first\nsecond\n"
    );
}

#[test]
fn escaped_writes_match_legacy_encoding_across_fast_path_boundary() {
    for len in [0, 1, 31, 32, 33, 64, 1024] {
        let plain = "a".repeat(len);
        for marker in ['&', '<', '>', '"', '\'', '/', '\0', '雪', '🦀'] {
            for pos in [0, len / 2, len] {
                let mut input = plain.clone();
                input.insert(pos, marker);
                let mut output = DocumentWriter::new();
                output.write_str("prefix:");
                output.write_escaped_html(&input);
                output.write_escaped_html("&\"");
                output.write_str(":suffix");
                assert_eq!(
                    output.finish(),
                    format!(
                        "prefix:{}&amp;&quot;:suffix",
                        crate::common::utils::escape_html(&input),
                    ),
                    "escaping differs for {input:?}",
                );
            }
        }
    }
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
    super::write_html_attrs(&mut output, &small);
    assert_eq!(
        output.finish(),
        " class=\"first second\" id=\"one\" id=\"two\" style=\"color:&lt;red&gt;;display:block\" title=\"&lt;&amp;&gt;\""
    );

    let mut large = small;
    large.extend([("data-a".into(), "a".into()), ("data-b".into(), "b".into())]);
    let mut output = DocumentWriter::new();
    super::write_html_attrs(&mut output, &large);
    assert_eq!(
        output.finish(),
        " class=\"first second\" id=\"one\" id=\"two\" style=\"color:&lt;red&gt;;display:block\" title=\"&lt;&amp;&gt;\" data-a=\"a\" data-b=\"b\""
    );
}

#[test]
fn registered_paragraph_renderer_preserves_attributes() {
    let md = MarkdownIt::new();
    let mut document = md.parse_document("hello");
    let paragraph = document.children(document.root())[0];
    let mut edits = crate::EditBatch::new();
    edits.set_attribute(paragraph, "class", "one two");
    edits.commit(&mut document);

    assert_eq!(
        md.render_document(&document),
        "<p class=\"one two\">hello</p>\n"
    );
}

#[test]
fn commonmark_block_renderers_match_render_api() {
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
            let expected = md.render(source);
            let document = md.parse_document(source);
            assert_eq!(
                md.render_document(&document),
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

    let mut parsed = MarkdownIt::empty();
    crate::plugins::cmark::add(&mut parsed);
    crate::plugins::sourcepos::add(&mut parsed);
    let expected = parsed.render(source);

    let mut direct = MarkdownIt::empty();
    crate::plugins::cmark::add(&mut direct);
    crate::plugins::sourcepos::add(&mut direct);
    let document = direct.parse_document(source);

    assert_eq!(direct.render_document(&document), expected);
}

#[test]
fn commonmark_inline_renderers_match_render_api() {
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
                let expected = md.render(source);
                let document = md.parse_document(source);
                assert_eq!(
                    md.render_document(&document),
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

    let mut parsed = MarkdownIt::empty();
    crate::plugins::cmark::add(&mut parsed);
    crate::plugins::sourcepos::add(&mut parsed);
    let expected = parsed.render(source);

    let mut direct = MarkdownIt::empty();
    crate::plugins::cmark::add(&mut direct);
    crate::plugins::sourcepos::add(&mut direct);
    let document = direct.parse_document(source);

    assert_eq!(direct.render_document(&document), expected);
}

#[test]
fn unknown_container_transparently_renders_children() {
    let md = MarkdownIt::empty();
    let mut root = NodeDraft::new(UnknownContainer);
    root.push_child(NodeDraft::new(Text {
        content: "child".into(),
    }));
    let document = Document::from_draft("", root);

    assert_eq!(md.render_document(&document), "child");
    assert_eq!(md.render_document_as(&document, "text"), "child");
}

#[test]
fn custom_renderer_can_be_registered_and_overridden_per_format() {
    let document = Document::from_draft("", NodeDraft::new(UnknownLeaf("value")));
    let mut registry = DocumentRendererRegistry::new();

    assert!(!registry.add::<UnknownLeaf, _>("plain", UnknownLeafRenderer("first")));
    assert!(registry.contains::<UnknownLeaf>("plain"));
    assert_eq!(
        registry.render(&document, "plain", &RenderOptions::default()),
        "first:value"
    );

    assert!(registry.add::<UnknownLeaf, _>("plain", UnknownLeafRenderer("second")));
    assert_eq!(
        registry.render(&document, "plain", &RenderOptions::default()),
        "second:value"
    );
    assert!(registry.remove::<UnknownLeaf>("plain"));
    assert!(!registry.contains::<UnknownLeaf>("plain"));
}

#[test]
fn markdown_it_selects_and_isolates_renderer_formats() {
    let document = Document::from_draft("", NodeDraft::new(UnknownLeaf("value")));
    let mut md = MarkdownIt::empty();
    md.add_document_renderer::<UnknownLeaf, _>("html", UnknownLeafRenderer("html"));
    md.add_document_renderer::<UnknownLeaf, _>("text", UnknownLeafRenderer("text"));

    assert_eq!(md.render_document(&document), "html:value");
    assert_eq!(md.render_document_as(&document, "text"), "text:value");

    md.add_document_renderer::<UnknownLeaf, _>("text", UnknownLeafRenderer("override"));
    assert_eq!(md.render_document(&document), "html:value");
    assert_eq!(md.render_document_as(&document, "text"), "override:value");
}

#[test]
fn unknown_leaf_panics_with_format_type_and_node_id() {
    let md = MarkdownIt::empty();
    let document = Document::from_draft("", NodeDraft::new(UnknownLeaf("value")));

    let panic = catch_unwind(AssertUnwindSafe(|| {
        let _ = md.render_document(&document);
    }))
    .expect_err("rendering an unregistered leaf must panic");
    let message = panic_message(panic);

    assert!(message.contains("html"), "{message}");
    assert!(
        message.contains(std::any::type_name::<UnknownLeaf>()),
        "{message}"
    );
    assert!(message.contains("NodeId(0:0)"), "{message}");
}

#[test]
fn missing_format_panics_with_requested_format() {
    let md = MarkdownIt::empty();
    let document = Document::from_draft("", NodeDraft::new(UnknownLeaf("value")));

    let panic = catch_unwind(AssertUnwindSafe(|| {
        let _ = md.render_document_as(&document, "missing");
    }))
    .expect_err("rendering without a registered format must panic");
    let message = panic_message(panic);

    assert!(message.contains("missing"), "{message}");
    assert!(
        message.contains(std::any::type_name::<UnknownLeaf>()),
        "{message}"
    );
    assert!(message.contains("NodeId(0:0)"), "{message}");
}

#[test]
fn writer_supports_inherent_string_char_and_format_writes() {
    let mut output = DocumentWriter::new();
    output.write_str("hé");
    output.write_char('🦀');
    write!(output, "{}", 42);
    let suffix = "end";
    writeln!(output, "-{suffix}");

    assert_eq!(output.finish(), "hé🦀42-end\n");
}

#[test]
fn inherent_write_fmt_panics_on_formatting_error() {
    let mut output = DocumentWriter::new();

    let panic = catch_unwind(AssertUnwindSafe(|| {
        write!(output, "{}", FailingDisplay);
    }))
    .expect_err("inherent write_fmt must panic when formatting fails");
    let message = panic_message(panic);

    assert!(message.contains("failed"), "{message}");
}

fn write_through_generic<W: std::fmt::Write>(writer: &mut W) {
    writer.write_str("generic").unwrap();
    write!(writer, "-{}", 7).unwrap();
}

#[test]
fn writer_interoperates_with_generic_and_trait_object_fmt_write() {
    let mut output = DocumentWriter::new();
    write_through_generic(&mut output);
    output.write_char('|');

    let dyn_writer: &mut dyn std::fmt::Write = &mut output;
    dyn_writer.write_str("dyn").unwrap();
    write!(dyn_writer, "-{}", 9).unwrap();

    assert_eq!(output.finish(), "generic-7|dyn-9");
}

#[test]
fn trait_write_fmt_propagates_formatting_error() {
    let mut output = DocumentWriter::new();

    let result = <DocumentWriter as std::fmt::Write>::write_fmt(
        &mut output,
        format_args!("{}", FailingDisplay),
    );

    assert!(result.is_err());
}
