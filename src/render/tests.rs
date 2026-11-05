use std::panic::{AssertUnwindSafe, catch_unwind};

use super::{NodeRenderer, RenderContext, RendererRegistry, Writer};
use crate::MarkdownIt;
use crate::document::{Document, NodeDraft, NodeRef, Text};
use crate::render::RenderOptions;

#[derive(Debug)]
pub(super) struct FailingDisplay;

impl std::fmt::Display for FailingDisplay {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Err(std::fmt::Error)
    }
}

pub(super) fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else {
        "<non-string panic payload>".to_owned()
    }
}

#[derive(Debug)]
struct UnknownContainer;

#[derive(Debug)]
struct UnknownLeaf(&'static str);

struct UnknownLeafRenderer(&'static str);

impl NodeRenderer<UnknownLeaf> for UnknownLeafRenderer {
    fn render(
        &self,
        _: NodeRef<'_>,
        value: &UnknownLeaf,
        _: &mut RenderContext<'_>,
        output: &mut Writer,
    ) {
        write!(output, "{}:{}", self.0, value.0);
    }
}

struct DirectWriteAndCrRenderer;

impl NodeRenderer<UnknownLeaf> for DirectWriteAndCrRenderer {
    fn render(
        &self,
        _: NodeRef<'_>,
        _: &UnknownLeaf,
        context: &mut RenderContext<'_>,
        output: &mut Writer,
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
    let document = md.parse("hello <world>");

    assert_eq!(
        md.render_document(&document, "html"),
        "hello &lt;world&gt;\n"
    );
    assert_eq!(md.render_document(&document, "text"), "hello <world>\n");
    assert_eq!(
        md.render_document(&document, "html"),
        "hello &lt;world&gt;\n"
    );

    let nul = md.parse("\0");
    assert_eq!(md.render_document(&nul, "html"), "\u{FFFD}\n");
    assert_eq!(md.render_document(&nul, "text"), "\u{FFFD}\n");
}

#[test]
fn cr_observes_direct_renderer_writes_without_duplicate_line_endings() {
    let mut registry = RendererRegistry::new();
    registry.add::<UnknownLeaf, _>("html", DirectWriteAndCrRenderer);
    let document = Document::from_draft("", NodeDraft::new(UnknownLeaf("unused")));

    assert_eq!(
        registry.render(&document, "html", &RenderOptions::default()),
        "first\nsecond\n"
    );
}

#[test]
fn generated_link_and_image_attrs_preserve_duplicates_and_escaping() {
    use crate::plugins::cmark::inline::image::Image;
    use crate::plugins::cmark::inline::link::Link;

    for xhtml in [false, true] {
        for padding in [0, 8] {
            let mut md = MarkdownIt::new();
            md.render_options.xhtml_out = xhtml;
            for image in [false, true] {
                let mut draft = if image {
                    NodeDraft::new(Image {
                        url: "/image?a&b".into(),
                        title: Some("generated <title>".into()),
                    })
                } else {
                    NodeDraft::new(Link {
                        url: "/link?a&b".into(),
                        title: Some("generated <title>".into()),
                    })
                };
                draft.push_child(NodeDraft::new(Text {
                    content: "a & b".into(),
                }));
                let target = if image { "src" } else { "href" };
                draft.attrs_mut().extend([
                    ("title".into(), "custom & title".into()),
                    (target.into(), "custom <url>".into()),
                    ("class".into(), "one".into()),
                    ("class".into(), "two".into()),
                ]);
                if image {
                    draft
                        .attrs_mut()
                        .push(("alt".into(), "custom <alt>".into()));
                }
                let mut extra_html = String::new();
                for index in 0..padding {
                    draft
                        .attrs_mut()
                        .push((format!("data-{index}"), "value".into()));
                    extra_html.push_str(&format!(" data-{index}=\"value\""));
                }
                let document = Document::from_draft("", draft);
                let tag = if image { "img" } else { "a" };
                let url = if image { "image" } else { "link" };
                let alt = if image {
                    " alt=\"custom &lt;alt&gt;\" alt=\"a &amp; b\""
                } else {
                    ""
                };
                let end = if image {
                    if xhtml { " />" } else { ">" }
                } else {
                    ">a &amp; b</a>"
                };
                assert_eq!(
                    md.render_document(&document, "html"),
                    format!(
                        "<{tag} title=\"custom &amp; title\" title=\"generated &lt;title&gt;\" {target}=\"custom &lt;url&gt;\" {target}=\"/{url}?a&amp;b\" class=\"one two\"{alt}{extra_html}{end}"
                    )
                );
            }
        }
    }
}

#[test]
fn registered_paragraph_renderer_preserves_attributes() {
    let md = MarkdownIt::new();
    let mut document = md.parse("hello");
    let paragraph = document.children(document.root())[0];
    let mut edits = crate::document::edit::EditBatch::new();
    edits.set_attribute(paragraph, "class", "one two");
    edits.commit(&mut document);

    assert_eq!(
        md.render_document(&document, "html"),
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
        MarkdownIt::with_preset(crate::plugins::presets::Preset::CommonMark),
    ] {
        md.render_options.lang_prefix = Some("lang-".into());
        for source in sources {
            let expected = md.render(source);
            let document = md.parse(source);
            assert_eq!(
                md.render_document(&document, "html"),
                expected,
                "direct renderer differs for {source:?} with options {:?}",
                md.render_options
            );
        }
    }
}

#[test]
fn block_renderers_preserve_transform_attributes() {
    let source = "# heading\n\n> quote\n\n3. item\n\n---\n\n    code\n\n```rs\nfenced\n```\n";

    let mut parsed = MarkdownIt::empty();
    crate::plugins::cmark::add(&mut parsed);
    #[cfg(feature = "sourcemap")]
    crate::plugins::sourcepos::add(&mut parsed);
    let expected = parsed.render(source);

    let mut direct = MarkdownIt::empty();
    crate::plugins::cmark::add(&mut direct);
    #[cfg(feature = "sourcemap")]
    crate::plugins::sourcepos::add(&mut direct);
    let document = direct.parse(source);

    assert_eq!(direct.render_document(&document, "html"), expected);
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
                let document = md.parse(source);
                assert_eq!(
                    md.render_document(&document, "html"),
                    expected,
                    "direct renderer differs for {source:?} with options {:?}",
                    md.render_options
                );
            }
        }
    }
}

#[test]
fn inline_renderers_preserve_transform_attributes() {
    let source = "**strong** [link](https://example.com) ![alt](image.png)  \nnext";

    let mut parsed = MarkdownIt::empty();
    crate::plugins::cmark::add(&mut parsed);
    #[cfg(feature = "sourcemap")]
    crate::plugins::sourcepos::add(&mut parsed);
    let expected = parsed.render(source);

    let mut direct = MarkdownIt::empty();
    crate::plugins::cmark::add(&mut direct);
    #[cfg(feature = "sourcemap")]
    crate::plugins::sourcepos::add(&mut direct);
    let document = direct.parse(source);

    assert_eq!(direct.render_document(&document, "html"), expected);
}

#[test]
fn unknown_container_transparently_renders_children() {
    let md = MarkdownIt::empty();
    let mut root = NodeDraft::new(UnknownContainer);
    root.push_child(NodeDraft::new(Text {
        content: "child".into(),
    }));
    let document = Document::from_draft("", root);

    assert_eq!(md.render_document(&document, "html"), "child");
    assert_eq!(md.render_document(&document, "text"), "child");
}

#[test]
fn custom_renderer_can_be_registered_and_overridden_per_format() {
    let document = Document::from_draft("", NodeDraft::new(UnknownLeaf("value")));
    let mut registry = RendererRegistry::new();

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
    md.add_renderer::<UnknownLeaf, _>("html", UnknownLeafRenderer("html"));
    md.add_renderer::<UnknownLeaf, _>("text", UnknownLeafRenderer("text"));

    assert_eq!(md.render_document(&document, "html"), "html:value");
    assert_eq!(md.render_document(&document, "text"), "text:value");

    md.add_renderer::<UnknownLeaf, _>("text", UnknownLeafRenderer("override"));
    assert_eq!(md.render_document(&document, "html"), "html:value");
    assert_eq!(md.render_document(&document, "text"), "override:value");
}

#[test]
fn unknown_leaf_panics_with_format_type_and_node_id() {
    let md = MarkdownIt::empty();
    let document = Document::from_draft("", NodeDraft::new(UnknownLeaf("value")));

    let panic = catch_unwind(AssertUnwindSafe(|| {
        let _ = md.render_document(&document, "html");
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
        let _ = md.render_document(&document, "missing");
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
