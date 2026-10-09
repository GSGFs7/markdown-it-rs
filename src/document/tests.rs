use super::*;
use crate::document::text::{TextProjection, TextProjectionKind};
use crate::document::{Root, Text};
use crate::plugins::cmark::block::paragraph::Paragraph;
use crate::{MarkdownIt, plugins};

#[test]
fn draft_insertion_preserves_tree_and_payloads() {
    let mut root = NodeDraft::new(Root::new("hello".to_owned()));
    let mut paragraph = NodeDraft::new(Paragraph);
    paragraph.push_child(NodeDraft::new(Text {
        content: "hello".to_owned(),
    }));
    root.push_child(paragraph);

    let document = Document::from_draft("hello", root);
    let root_id = document.root();
    let paragraph_id = document.children(root_id)[0];
    let text_id = document.children(paragraph_id)[0];

    assert_eq!(document.source(), "hello");
    assert_eq!(document.len(), 3);
    assert_eq!(document.parent(root_id), None);
    assert_eq!(document.parent(text_id), Some(paragraph_id));
    assert_eq!(
        document.node(text_id).cast::<Text>().unwrap().content,
        "hello"
    );
}

#[test]
fn structural_events_are_ordered_and_balanced() {
    let mut root = NodeDraft::new(Root::new("hello".to_owned()));
    let mut paragraph = NodeDraft::new(Paragraph);
    paragraph.push_child(NodeDraft::new(Text {
        content: "first".to_owned(),
    }));
    paragraph.push_child(NodeDraft::new(Text {
        content: "second".to_owned(),
    }));
    root.push_child(paragraph);

    let document = Document::from_draft("hello", root);
    let root = document.root();
    let paragraph = document.children(root)[0];
    let children = document.children(paragraph);
    let events = document.events(document.root());
    let actual: Vec<_> = events
        .map(|event| match event {
            StructuralEvent::Enter(node) => ("enter", node.id()),
            StructuralEvent::Leaf(node) => ("leaf", node.id()),
            StructuralEvent::Exit(node) => ("exit", node.id()),
        })
        .collect();

    assert_eq!(
        actual,
        [
            ("enter", root),
            ("enter", paragraph),
            ("leaf", children[0]),
            ("leaf", children[1]),
            ("exit", paragraph),
            ("exit", root),
        ]
    );
}

#[test]
fn structural_events_can_start_at_a_subtree_or_leaf() {
    let mut root = NodeDraft::new(Root::new("hello".to_owned()));
    let mut paragraph = NodeDraft::new(Paragraph);
    paragraph.push_child(NodeDraft::new(Text {
        content: "hello".to_owned(),
    }));
    root.push_child(paragraph);

    let document = Document::from_draft("hello", root);
    let paragraph = document.children(document.root())[0];
    let text = document.children(paragraph)[0];

    assert!(matches!(
        document.events(paragraph).next(),
        Some(StructuralEvent::Enter(node)) if node.id() == paragraph
    ));
    assert!(matches!(
        document.events(text).collect::<Vec<_>>().as_slice(),
        [StructuralEvent::Leaf(node)] if node.id() == text
    ));
}

#[test]
fn invalid_node_access_panics_in_all_builds() {
    fn transparent_text_projection(_: NodeRef<'_>) -> TextProjectionKind<'_> {
        TextProjectionKind::Transparent
    }

    let mut root = NodeDraft::new(Root::new(String::new()));
    root.push_child(NodeDraft::new(Text {
        content: "old".into(),
    }));
    root.push_child(NodeDraft::new(Text {
        content: "kept".into(),
    }));
    let mut document = Document::from_draft("", root);
    let old = document.children(document.root())[0];
    let kept = document.children(document.root())[1];

    let mut edits = crate::EditBatch::new();
    edits.remove_node(old);
    edits.commit(&mut document);
    let mut edits = crate::EditBatch::new();
    edits.insert_before(
        kept,
        NodeDraft::new(Text {
            content: "new".into(),
        }),
    );
    edits.commit(&mut document);
    let new = document.children(document.root())[0];
    assert_eq!(old.slot(), new.slot());
    assert_ne!(old.generation(), new.generation());
    assert!(document.get_node(new).is_some());
    assert_eq!(document.parent(document.root()), None);

    let unknown = NodeId {
        slot: u32::MAX,
        generation: 0,
    };
    let accessors: [fn(&Document, NodeId); 5] = [
        |d, id| {
            let _ = d.node(id);
        },
        |d, id| {
            let _ = d.parent(id);
        },
        |d, id| {
            let _ = d.children(id);
        },
        |d, id| {
            let _ = d.events(id);
        },
        |d, id| {
            let _ = d.text_events_from(id, TextProjection::new(transparent_text_projection));
        },
    ];
    for id in [old, unknown] {
        assert!(document.get_node(id).is_none());
        for access in accessors {
            let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                access(&document, id);
            }))
            .expect_err("invalid node access must panic");
            let message = panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("");
            assert!(message.contains("invalid or stale node ID"));
        }
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = document.node_mut(id);
            }))
            .is_err()
        );
    }
    assert_eq!(document.node(new).cast::<Text>().unwrap().content, "new");
}

#[test]
fn parser_facade_preserves_rendering_and_source() {
    let mut md = MarkdownIt::empty();
    plugins::cmark::add(&mut md);
    plugins::html::add(&mut md);
    let source = "# 雪\n\nA *small* document.\n";

    let expected = md.render(source);
    let document = md.parse_document(source);

    assert_eq!(document.source(), source);
    assert_eq!(md.render_document(&document), expected);
}

#[test]
fn parsed_document_events_visit_every_node_once() {
    use std::collections::HashSet;

    let mut md = MarkdownIt::empty();
    plugins::cmark::add(&mut md);
    plugins::html::add(&mut md);
    let document = md.parse_document("# Heading\n\nA *small* [link](url).\n\n---\n");
    let mut stack = Vec::new();
    let mut visited = HashSet::new();

    for event in document.events(document.root()) {
        let node = event.node();
        match event {
            StructuralEvent::Enter(_) => {
                assert_eq!(node.parent(), stack.last().copied());
                assert!(visited.insert(node.id()));
                stack.push(node.id());
            }
            StructuralEvent::Leaf(_) => {
                assert_eq!(node.parent(), stack.last().copied());
                assert!(visited.insert(node.id()));
            }
            StructuralEvent::Exit(_) => {
                assert_eq!(stack.pop(), Some(node.id()));
            }
        }
    }

    assert!(stack.is_empty());
    assert_eq!(visited.len(), document.len());
}

#[test]
fn node_data_survives_draft_insertion() {
    #[derive(Debug)]
    struct Payload(String);
    impl crate::NodeValue for Payload {}

    #[derive(Debug)]
    struct Metadata(String);

    fn draft() -> super::NodeDraft {
        let mut draft = super::NodeDraft::new(Payload("payload".into()));
        draft.set_srcmap(Some(crate::common::sourcemap::SourcePos::new(0, 3)));
        draft.attrs_mut().push(("class".into(), "kept".into()));
        draft.ext_mut().insert(Metadata("metadata".into()));
        draft.push_child(super::NodeDraft::new(Text {
            content: "abc".into(),
        }));
        draft
    }

    fn assert_document(document: &Document) {
        let root = document.node(document.root());
        assert_eq!(root.cast::<Payload>().unwrap().0, "payload");
        assert!(root.cast::<Text>().is_none());
        assert_eq!(root.srcmap().unwrap().get_byte_offsets(), (0, 3));
        assert_eq!(root.attrs(), &vec![("class".into(), "kept".into())]);
        assert_eq!(root.ext().get::<Metadata>().unwrap().0, "metadata");
        assert_eq!(root.children().len(), 1);
        let child = document.node(root.children()[0]);
        assert_eq!(child.parent(), Some(root.id()));
        assert_eq!(child.cast::<Text>().unwrap().content, "abc");
    }

    let document = Document::from_draft("abc", draft());
    assert_document(&document);
    let document = Document::from_draft("abc", draft());
    assert_document(&document);
}

#[test]
fn arena_builder_moves_ids_and_metadata_without_recreating_nodes() {
    let mut document = Document::new("hello", Root::new("hello"));
    let left = document.create_node(Paragraph);
    let right = document.create_node(Paragraph);
    let text = document.create_node(Text {
        content: "hello".into(),
    });
    let map = crate::common::sourcemap::SourcePos::new(0, 5);
    document.node_mut(text).set_srcmap(Some(map));
    document
        .node_mut(text)
        .attrs_mut()
        .push(("class".into(), "kept".into()));
    document.push_child(left, text);
    document.push_child(document.root(), left);
    document.push_child(document.root(), right);

    let children = document.take_children(left);
    assert_eq!(document.parent(text), None);
    document.attach_children(right, children);
    assert_eq!(document.children(right), &[text]);
    assert_eq!(document.parent(text), Some(right));
    assert_eq!(document.node(text).srcmap(), Some(map));
    assert_eq!(document.node(text).attrs()[0].1, "kept");

    let removed = document.take_children(document.root());
    document.discard_node(removed[0]);
    document.discard_node(removed[1]);
    assert_eq!(document.len(), 1);
    assert!(document.get_node(left).is_none());
    assert!(document.get_node(right).is_none());
    assert!(document.get_node(text).is_none());
}

#[test]
fn arena_builder_rejects_cycles_before_modifying_links() {
    let mut document = Document::new("", Root::new(""));
    let outer = document.create_node(Paragraph);
    let inner = document.create_node(Paragraph);
    document.push_child(outer, inner);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        document.push_child(inner, outer);
    }));
    assert!(result.is_err());
    assert_eq!(document.parent(outer), None);
    assert_eq!(document.parent(inner), Some(outer));
    assert!(document.children(inner).is_empty());
}
