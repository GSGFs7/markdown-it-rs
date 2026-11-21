use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use markdown_it::MarkdownIt;
use markdown_it::document::edit::EditBatch;
use markdown_it::document::text::{TextEvent, TextProjectionKind};
use markdown_it::document::{Document, Node, Root, StructuralEvent, Text, TextSpecial};
use markdown_it::render::{NodeRenderer, RenderContext, Writer};

#[test]
fn blanket_node_values_preserve_concrete_types() {
    #[derive(Debug)]
    struct Payload<T>(T);

    let mut doc = MarkdownIt::new().parse("");
    let generic = doc.create_node(Payload(vec![String::from("first")]));
    let boxed = doc.create_node(Box::new(String::from("boxed")));

    doc.value_mut::<Payload<Vec<String>>>(generic)
        .unwrap()
        .0
        .push(String::from("second"));
    assert_eq!(
        doc.value::<Payload<Vec<String>>>(generic).unwrap().0,
        ["first", "second"]
    );
    assert!(doc.value::<Payload<String>>(generic).is_none());
    assert_eq!(doc.value::<Box<String>>(boxed).unwrap().as_str(), "boxed");
    assert!(doc.value::<String>(boxed).is_none());
}

#[derive(Debug)]
struct Badge {
    label: String,
    drops: Arc<AtomicUsize>,
}

fn document_projection<'a>(doc: &'a Document, node: &'a Node) -> TextProjectionKind<'a> {
    if let Some(text) = doc.text(node.id()) {
        TextProjectionKind::Writable(text)
    } else {
        TextProjectionKind::Transparent
    }
}

#[test]
fn document_projection_preserves_subtree_depth_and_edit_offsets() {
    let md = MarkdownIt::new();
    let mut doc = md.parse("a *雪* and **b**");
    let projection = document_projection;
    let subtree = doc.children(doc.root())[0];
    let events: Vec<_> = doc.text_events_from(subtree, projection).collect();
    assert_eq!(
        events.first(),
        Some(&TextEvent::Enter {
            node: subtree,
            nesting_level: 0
        })
    );
    assert_eq!(
        events.last(),
        Some(&TextEvent::Exit {
            node: subtree,
            nesting_level: 0
        })
    );
    let chars: String = events
        .iter()
        .filter_map(|event| match event {
            TextEvent::Char { ch, .. } => Some(*ch),
            _ => None,
        })
        .collect();
    assert_eq!(chars, "a 雪 and b");

    let mut edits = EditBatch::new();
    for event in doc.text_events(projection) {
        if let TextEvent::Char {
            node,
            byte_offset,
            ch: '雪',
            writable: true,
            ..
        } = event
        {
            edits.replace_char(node, byte_offset..byte_offset + '雪'.len_utf8(), '雨');
        }
    }
    edits.commit(&mut doc);
    assert_eq!(
        md.render_document(&doc, "html"),
        "<p>a <em>雨</em> and <strong>b</strong></p>\n"
    );
    let chars: String = doc
        .text_events(projection)
        .filter_map(|event| {
            if let TextEvent::Char { ch, .. } = event {
                Some(ch)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(chars, "a 雨 and b");
}

#[derive(Debug)]
struct SourceContext(std::ops::Range<usize>);

fn source_projection<'a>(doc: &'a Document, node: &'a Node) -> TextProjectionKind<'a> {
    if let Some(context) = doc.value::<SourceContext>(node.id()) {
        TextProjectionKind::ReadOnly(&doc.source()[context.0.clone()])
    } else {
        document_projection(doc, node)
    }
}

#[test]
fn external_classifier_can_borrow_document_source_as_read_only_context() {
    let mut doc = Document::new("前雪尾", Root::new("前雪尾"));
    let context = doc.create_node(SourceContext(3..6));
    doc.push_child(doc.root(), context);
    let text = doc.create_node(Text::new("edited"));
    doc.push_child(doc.root(), text);
    let context = doc.children(doc.root())[0];
    let text = doc.children(doc.root())[1];
    let chars: Vec<_> = doc
        .text_events(source_projection)
        .filter_map(|event| match event {
            TextEvent::Char {
                node,
                byte_offset,
                ch,
                writable,
                ..
            } => Some((node, byte_offset, ch, writable)),
            _ => None,
        })
        .collect();
    assert_eq!(chars[0], (context, 0, '雪', false));
    assert_eq!(chars[1], (text, 0, 'e', true));
    let mut events = doc.text_events_from(context, source_projection);
    assert!(matches!(
        events.next(),
        Some(TextEvent::Enter {
            nesting_level: 0,
            ..
        })
    ));
    assert!(matches!(
        events.next(),
        Some(TextEvent::Char {
            ch: '雪',
            writable: false,
            ..
        })
    ));
    assert!(matches!(
        events.next(),
        Some(TextEvent::Exit {
            nesting_level: 0,
            ..
        })
    ));
    assert_eq!(events.next(), None);
    assert_eq!(events.next(), None);
}

impl Drop for Badge {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

struct BadgeRenderer;

impl NodeRenderer<Badge> for BadgeRenderer {
    fn render(
        &self,
        node: &Node,
        value: &Badge,
        context: &mut RenderContext<'_>,
        output: &mut Writer,
    ) {
        assert!(std::ptr::eq(
            value,
            context.document().value::<Badge>(node.id()).unwrap()
        ));
        output.write_str(&value.label);
        context.render_children(node.id(), output);
    }
}

#[test]
fn external_payload_mutation_replacement_and_removal_preserve_ownership() {
    let drops = Arc::new(AtomicUsize::new(0));
    let badge = || Badge {
        label: "old".into(),
        drops: Arc::clone(&drops),
    };
    let mut md = MarkdownIt::empty();
    md.add_renderer::<Badge, _>("html", BadgeRenderer);
    let mut doc = Document::new("", Root::new(""));
    let root = doc.root();
    let id = doc.create_node(badge());
    doc.push_child(root, id);
    let child = doc.create_node(Text::new("!"));
    doc.push_child(id, child);
    doc.node_mut(id)
        .attrs_mut()
        .push(("data-kind".into(), "badge".into()));
    doc.value_mut::<Badge>(id).unwrap().label = "edited".into();
    assert_eq!(md.render_document(&doc, "html"), "edited!");
    assert_eq!(drops.load(Ordering::SeqCst), 0);

    let mut edits = EditBatch::new();
    edits.replace_value(id, badge());
    edits.commit(&mut doc);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(doc.children(id), &[child]);
    assert_eq!(doc.parent(child), Some(id));
    assert_eq!(doc.node(id).attrs()[0].1, "badge");
    assert_eq!(md.render_document(&doc, "html"), "old!");

    doc.remove_node(id);
    assert_eq!(drops.load(Ordering::SeqCst), 2);
    let reused = doc.create_node(badge());
    doc.push_child(root, reused);
    assert_ne!(reused, id);
    assert!(doc.get_node(id).is_none());
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| doc.value::<Badge>(id))).is_err()
    );
    drop(doc);
    assert_eq!(drops.load(Ordering::SeqCst), 3);
}

#[test]
fn borrowed_unicode_text_can_produce_edits_without_copying_the_input() {
    let md = MarkdownIt::new();
    let mut doc = md.parse("雪 &amp; <tag>");
    let source = doc.source().to_owned();
    let mut edits = EditBatch::new();
    let mut edited = None;
    for event in doc.events(doc.root()) {
        if matches!(event, StructuralEvent::Exit(_)) {
            continue;
        }
        let id = event.node().id();
        if let Some(text) = doc.text(id)
            && let Some(offset) = text.find('雪')
        {
            edits.replace_text(id, offset..offset + '雪'.len_utf8(), "雨");
            edited = Some(id);
        }
    }
    let id = edited.unwrap();
    #[cfg(feature = "sourcemap")]
    let srcmap = doc.node(id).srcmap();
    edits.commit(&mut doc);
    doc.materialize_text(id).unwrap().insert(0, '新');
    assert_eq!(
        md.render_document(&doc, "html"),
        "<p>新雨 &amp; &lt;tag&gt;</p>\n"
    );
    assert_eq!(md.render_document(&doc, "text"), "新雨 & <tag>\n");
    assert_eq!(doc.source(), source);
    #[cfg(feature = "sourcemap")]
    assert_eq!(doc.node(id).srcmap(), srcmap);
}

#[test]
fn owned_text_snapshot_and_generated_node_outlive_the_source_document() {
    let snapshot = {
        let doc = Document::new("unrelated source", Text::new("decoded & edited 雪"));
        doc.text(doc.root()).unwrap().to_owned()
    };
    let mut doc = Document::new("", Text::new(snapshot));
    let id = doc.root();
    assert_eq!(doc.text(id), Some("decoded & edited 雪"));
    let md = MarkdownIt::empty();
    assert_eq!(md.render_document(&doc, "html"), "decoded &amp; edited 雪");
    assert_eq!(md.render_document(&doc, "text"), "decoded & edited 雪");
    let mut edits = EditBatch::new();
    edits.replace_value(id, TextSpecial::new("&", "&amp;", "entity"));
    edits.commit(&mut doc);
    assert!(doc.text(id).is_none());
    assert!(doc.materialize_text(id).is_none());
    assert!(doc.value::<Text>(id).is_none());
    assert_eq!(doc.value::<TextSpecial>(id).unwrap().content(), "&");
    assert_eq!(md.render_document(&doc, "html"), "&amp;");
    assert_eq!(md.render_document(&doc, "text"), "&");
    doc.value_mut::<TextSpecial>(id)
        .unwrap()
        .set_content("雪<&amp;>");
    assert_eq!(md.render_document(&doc, "html"), "雪&lt;&amp;amp;&gt;");
    assert_eq!(md.render_document(&doc, "text"), "雪<&amp;>");
}

#[test]
fn set_content_preserves_metadata_and_read_access_covers_normalized_text() {
    let md = MarkdownIt::new();
    let source = ">   hello &amp; \\*world\\*\r\n>   next";
    let mut doc = md.parse(source);

    let mut plain_texts = Vec::new();
    let mut specials = Vec::new();
    for event in doc.events(doc.root()) {
        let StructuralEvent::Leaf(node) = event else {
            continue;
        };
        if let Some(text) = node.cast::<Text>() {
            assert_eq!(doc.text(node.id()), Some(text.as_str()));
            plain_texts.push((node.id(), text.as_str().to_owned()));
        } else if let Some(special) = node.cast::<TextSpecial>() {
            assert!(doc.text(node.id()).is_none());
            specials.push((
                node.id(),
                special.content().to_owned(),
                special.markup().to_owned(),
                special.info(),
            ));
        }
    }

    assert!(plain_texts.iter().any(|(_, text)| text == "hello "));
    assert!(plain_texts.iter().any(|(_, text)| text == "next"));
    assert!(
        specials
            .iter()
            .any(|(_, content, markup, info)| content == "&"
                && markup == "&amp;"
                && *info == "entity")
    );
    assert!(
        specials
            .iter()
            .any(|(_, content, markup, info)| content == "*"
                && markup == "\\*"
                && *info == "escape")
    );

    let text_id = plain_texts[0].0;
    doc.node_mut(text_id)
        .attrs_mut()
        .push(("data-k".into(), "v".into()));
    doc.node_mut(text_id).ext_mut().insert(7_u32);
    #[cfg(feature = "sourcemap")]
    let text_srcmap = doc.node(text_id).srcmap();

    doc.value_mut::<Text>(text_id)
        .unwrap()
        .set_content("replaced");
    assert_eq!(doc.text(text_id), Some("replaced"));
    assert_eq!(doc.node(text_id).attrs()[0].1, "v");
    assert_eq!(doc.node(text_id).ext().get::<u32>(), Some(&7));
    #[cfg(feature = "sourcemap")]
    assert_eq!(doc.node(text_id).srcmap(), text_srcmap);

    let special_id = specials[0].0;
    doc.node_mut(special_id)
        .attrs_mut()
        .push(("data-s".into(), "1".into()));
    doc.node_mut(special_id).ext_mut().insert(9_u32);
    #[cfg(feature = "sourcemap")]
    let special_srcmap = doc.node(special_id).srcmap();

    doc.value_mut::<TextSpecial>(special_id)
        .unwrap()
        .set_content("and");
    assert_eq!(
        doc.value::<TextSpecial>(special_id).unwrap().content(),
        "and"
    );
    assert_eq!(
        doc.value::<TextSpecial>(special_id).unwrap().markup(),
        "&amp;"
    );
    assert_eq!(
        doc.value::<TextSpecial>(special_id).unwrap().info(),
        "entity"
    );
    assert_eq!(doc.node(special_id).attrs()[0].1, "1");
    assert_eq!(doc.node(special_id).ext().get::<u32>(), Some(&9));
    #[cfg(feature = "sourcemap")]
    assert_eq!(doc.node(special_id).srcmap(), special_srcmap);
}

#[test]
fn built_in_text_payloads_expose_accessors_instead_of_storage_fields() {
    use markdown_it::plugins::cmark::block::code::CodeBlock;
    use markdown_it::plugins::cmark::block::fence::CodeFence;
    use markdown_it::plugins::extra::front_matter::{FrontMatter, FrontMatterKind};
    use markdown_it::plugins::html::html_block::HtmlBlock;
    use markdown_it::plugins::html::html_inline::HtmlInline;

    let mut html_inline = HtmlInline::new("<b>");
    assert_eq!(html_inline.as_str(), "<b>");
    html_inline.set_content("<i>");
    assert_eq!(html_inline.as_str(), "<i>");

    let mut html_block = HtmlBlock::new("<div>\n");
    assert_eq!(html_block.as_str(), "<div>\n");
    html_block.set_content("<p>\n");
    assert_eq!(html_block.as_str(), "<p>\n");

    let mut code_block = CodeBlock::new("    x\n");
    assert_eq!(code_block.as_str(), "    x\n");
    code_block.set_content("y\n");
    assert_eq!(code_block.as_str(), "y\n");

    let mut fence = CodeFence::new("rust", '`', 3, "fn main() {}\n", "language-");
    assert_eq!(fence.as_str(), "fn main() {}\n");
    assert_eq!(
        (
            fence.info.as_str(),
            fence.marker,
            fence.marker_len,
            fence.lang_prefix.as_str()
        ),
        ("rust", '`', 3, "language-")
    );
    fence.set_content("let x = 1;\n");
    assert_eq!(fence.as_str(), "let x = 1;\n");

    let mut front_matter = FrontMatter::new(FrontMatterKind::Yaml, "a: b", 0, 2);
    assert_eq!(front_matter.raw(), "a: b");
    assert_eq!(
        front_matter.parse_with(|kind, raw| Ok::<_, ()>((kind, raw.to_owned()))),
        Ok((FrontMatterKind::Yaml, "a: b".to_owned()))
    );
    front_matter.set_raw("c: d");
    assert_eq!(front_matter.raw(), "c: d");
    assert_eq!(front_matter.kind, FrontMatterKind::Yaml);
    assert_eq!((front_matter.start_line, front_matter.end_line), (0, 2));

    let doc = markdown_it::MarkdownIt::new().parse("source");
    let root = doc.value::<Root>(doc.root()).unwrap();
    assert_eq!(root.content(), "source");
}

#[test]
fn image_alt_is_derived_from_children_and_text_edits_keep_metadata() {
    let md = MarkdownIt::new();
    let mut doc = md.parse("![old](/img)");
    let paragraph = doc.children(doc.root())[0];
    let image = doc.children(paragraph)[0];
    let text = doc.children(image)[0];
    #[cfg(feature = "sourcemap")]
    let text_srcmap = doc.node(text).srcmap();
    #[cfg(feature = "sourcemap")]
    let image_srcmap = doc.node(image).srcmap();

    let mut edits = EditBatch::new();
    edits.replace_text(text, 0..3, "new");
    edits.commit(&mut doc);

    assert_eq!(doc.text(text), Some("new"));
    assert_eq!(doc.children(image), &[text]);
    #[cfg(feature = "sourcemap")]
    {
        // Text edits keep the original map instead of shifting it.
        assert_eq!(doc.node(text).srcmap(), text_srcmap);
        assert_eq!(doc.node(image).srcmap(), image_srcmap);
    }
    assert_eq!(
        md.render_document(&doc, "html"),
        "<p><img src=\"/img\" alt=\"new\"></p>\n"
    );

    // Alt is not cached: removing the children updates it on the next render.
    doc.remove_node(text);
    assert!(doc.children(image).is_empty());
    assert_eq!(
        md.render_document(&doc, "html"),
        "<p><img src=\"/img\" alt=\"\"></p>\n"
    );
}
