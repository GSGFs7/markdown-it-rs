use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use markdown_it::{
    Document,
    EditBatch,
    MarkdownIt,
    NodeDraft,
    NodeRef,
    NodeRenderer,
    RenderContext,
    Root,
    StructuralEvent,
    Text,
    TextEvent,
    TextProjectionKind,
    TextSpecial,
    Writer,
};

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

fn document_projection<'a>(doc: &'a Document, node: NodeRef<'a>) -> TextProjectionKind<'a> {
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
        md.render_html(&doc),
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

fn source_projection<'a>(doc: &'a Document, node: NodeRef<'a>) -> TextProjectionKind<'a> {
    if let Some(context) = doc.value::<SourceContext>(node.id()) {
        TextProjectionKind::ReadOnly(&doc.source()[context.0.clone()])
    } else {
        document_projection(doc, node)
    }
}

#[test]
fn external_classifier_can_borrow_document_source_as_read_only_context() {
    let mut root = NodeDraft::new(Root::new("前雪尾"));
    root.push_child(NodeDraft::new(SourceContext(3..6)));
    root.push_child(NodeDraft::new(Text {
        content: "edited".into(),
    }));
    let doc = Document::from_draft("前雪尾", root);
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
        node: NodeRef<'_>,
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
    let mut doc = Document::from_draft("", NodeDraft::new(Root::new("")));
    let root = doc.root();
    let id = doc.append_child(root, NodeDraft::new(badge()));
    let child = doc.append_child(
        id,
        NodeDraft::new(Text {
            content: "!".into(),
        }),
    );
    doc.node_mut(id)
        .attrs_mut()
        .push(("data-kind".into(), "badge".into()));
    doc.value_mut::<Badge>(id).unwrap().label = "edited".into();
    assert_eq!(md.render_html(&doc), "edited!");
    assert_eq!(drops.load(Ordering::SeqCst), 0);

    let mut edits = EditBatch::new();
    edits.replace_value(id, badge());
    edits.commit(&mut doc);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(doc.children(id), &[child]);
    assert_eq!(doc.parent(child), Some(id));
    assert_eq!(doc.node(id).attrs()[0].1, "badge");
    assert_eq!(md.render_html(&doc), "old!");

    let mut edits = EditBatch::new();
    edits.remove_node(id);
    edits.commit(&mut doc);
    assert_eq!(drops.load(Ordering::SeqCst), 2);
    let reused = doc.append_child(root, NodeDraft::new(badge()));
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
    assert_eq!(md.render_html(&doc), "<p>新雨 &amp; &lt;tag&gt;</p>\n");
    assert_eq!(md.render_as(&doc, "text"), "新雨 & <tag>\n");
    assert_eq!(doc.source(), source);
    #[cfg(feature = "sourcemap")]
    assert_eq!(doc.node(id).srcmap(), srcmap);
}

#[test]
fn owned_text_snapshot_and_generated_draft_outlive_the_source_document() {
    let snapshot = {
        let doc = Document::from_draft(
            "unrelated source",
            NodeDraft::new(Text {
                content: "decoded & edited 雪".into(),
            }),
        );
        doc.text(doc.root()).unwrap().to_owned()
    };
    let mut doc = Document::from_draft("", NodeDraft::new(Text { content: snapshot }));
    let id = doc.root();
    assert_eq!(doc.text(id), Some("decoded & edited 雪"));
    let md = MarkdownIt::empty();
    assert_eq!(md.render_html(&doc), "decoded &amp; edited 雪");
    assert_eq!(md.render_as(&doc, "text"), "decoded & edited 雪");
    let mut edits = EditBatch::new();
    edits.replace_value(
        id,
        TextSpecial {
            content: "&".into(),
            markup: "&amp;".into(),
            info: "entity",
        },
    );
    edits.commit(&mut doc);
    assert!(doc.text(id).is_none());
    assert!(doc.materialize_text(id).is_none());
    assert!(doc.value::<Text>(id).is_none());
    assert_eq!(doc.value::<TextSpecial>(id).unwrap().content, "&");
    assert_eq!(md.render_html(&doc), "&amp;");
    assert_eq!(md.render_as(&doc, "text"), "&");
    doc.value_mut::<TextSpecial>(id).unwrap().content = "雪<&amp;>".into();
    assert_eq!(md.render_html(&doc), "雪&lt;&amp;amp;&gt;");
    assert_eq!(md.render_as(&doc, "text"), "雪<&amp;>");
}
