use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use markdown_it::{
    Document,
    EditBatch,
    MarkdownIt,
    NodeDraft,
    NodeRef,
    NodeRenderer,
    NodeValue,
    RenderContext,
    Root,
    StructuralEvent,
    Text,
    TextSpecial,
    Writer,
};

#[derive(Debug)]
struct Badge {
    label: String,
    drops: Arc<AtomicUsize>,
}

impl NodeValue for Badge {}

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
    assert_eq!(MarkdownIt::empty().render_html(&doc), "&amp;");
}
