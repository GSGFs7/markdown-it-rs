use std::mem::size_of;

use super::text::TextProjectionKind;
use super::*;
use crate::parser::core::Root;
use crate::parser::inline::Text;
use crate::plugins::cmark::block::paragraph::Paragraph;
use crate::{MarkdownIt, Node, TextProjection, plugins};

fn transparent_text_projection(_: super::NodeRef<'_>) -> TextProjectionKind<'_> {
    TextProjectionKind::Transparent
}

#[test]
fn stale_id_cannot_access_reused_slot() {
    let mut arena = Arena::new();
    let old = arena.insert_with(|_| "old");
    assert_eq!(arena.remove(old), Some("old"));

    let new = arena.insert_with(|_| "new");
    assert_eq!(old.slot(), new.slot());
    assert_ne!(old.generation(), new.generation());
    assert_eq!(arena.get(old), None);
    assert_eq!(arena.get(new), Some(&"new"));
}

#[test]
fn maximum_generation_slot_is_retired_instead_of_wrapping() {
    let old = super::NodeId {
        slot: 0,
        generation: u32::MAX,
    };
    let mut arena = Arena {
        slots: vec![super::Slot {
            generation: u32::MAX,
            next_free: None,
            value: Some("old"),
        }],
        free_head: None,
        len: 1,
    };

    assert_eq!(arena.remove(old), Some("old"));
    let new = arena.insert_with(|_| "new");

    assert_eq!(new.slot(), 1);
    assert_eq!(arena.get(old), None);
}

#[test]
fn legacy_roundtrip_preserves_tree_and_payloads() {
    let mut root = Node::new(Root::new("hello".to_owned()));
    let mut paragraph = Node::new(Paragraph);
    paragraph.children.push(Node::new(Text {
        content: "hello".to_owned(),
    }));
    root.children.push(paragraph);

    let document = Document::from_legacy("hello", root);
    let root_id = document.root();
    let paragraph_id = document.children(root_id).unwrap()[0];
    let text_id = document.children(paragraph_id).unwrap()[0];

    assert_eq!(document.source(), "hello");
    assert_eq!(document.len(), 3);
    assert_eq!(document.parent(root_id).unwrap(), None);
    assert_eq!(document.parent(text_id).unwrap(), Some(paragraph_id));
    assert_eq!(
        document
            .node(text_id)
            .unwrap()
            .cast::<Text>()
            .unwrap()
            .content,
        "hello"
    );

    let legacy = document.into_legacy();
    assert!(legacy.is::<Root>());
    assert!(legacy.children[0].is::<Paragraph>());
    assert_eq!(
        legacy.children[0].children[0]
            .cast::<Text>()
            .unwrap()
            .content,
        "hello"
    );
}

#[test]
fn structural_events_are_ordered_and_balanced() {
    let mut root = Node::new(Root::new("hello".to_owned()));
    let mut paragraph = Node::new(Paragraph);
    paragraph.children.push(Node::new(Text {
        content: "first".to_owned(),
    }));
    paragraph.children.push(Node::new(Text {
        content: "second".to_owned(),
    }));
    root.children.push(paragraph);

    let document = Document::from_legacy("hello", root);
    let root = document.root();
    let paragraph = document.children(root).unwrap()[0];
    let children = document.children(paragraph).unwrap();
    let events = document.events(document.root()).unwrap();
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
    let mut root = Node::new(Root::new("hello".to_owned()));
    let mut paragraph = Node::new(Paragraph);
    paragraph.children.push(Node::new(Text {
        content: "hello".to_owned(),
    }));
    root.children.push(paragraph);

    let document = Document::from_legacy("hello", root);
    let paragraph = document.children(document.root()).unwrap()[0];
    let text = document.children(paragraph).unwrap()[0];

    assert!(matches!(
        document.events(paragraph).unwrap().next(),
        Some(StructuralEvent::Enter(node)) if node.id() == paragraph
    ));
    assert!(matches!(
        document.events(text).unwrap().collect::<Vec<_>>().as_slice(),
        [StructuralEvent::Leaf(node)] if node.id() == text
    ));
}

#[test]
fn structural_events_reject_a_stale_root() {
    let mut root = Node::new(Root::new("text".to_owned()));
    root.children.push(Node::new(Text {
        content: "text".to_owned(),
    }));
    let mut document = Document::from_legacy("", root);
    let root = document.root();
    let text = document.children(root).unwrap()[0];
    assert!(document.arena.remove(root).is_some());

    assert_eq!(
        document.events(root).unwrap_err(),
        super::InvalidNodeId(root)
    );
    assert!(matches!(
        document.text_events_from(root, TextProjection::new(transparent_text_projection)),
        Err(error) if error == super::InvalidNodeId(root)
    ));
    assert_eq!(
        document.node(text).unwrap().cast::<Text>().unwrap().content,
        "text"
    );
}

#[test]
fn layout_sizes_are_visible_to_the_arena_design() {
    // Keep this measurement close to the storage definition so future
    // layout changes cannot happen without an explicit review point.
    eprintln!(
        "Node={} DocumentNode={} Slot<DocumentNode>={}",
        size_of::<Node>(),
        size_of::<DocumentNode>(),
        size_of::<super::Slot<DocumentNode>>()
    );
    assert!(size_of::<super::Slot<DocumentNode>>() <= 256);
}

#[test]
fn parser_facade_preserves_rendering_and_source() {
    let mut md = MarkdownIt::empty();
    plugins::cmark::add(&mut md);
    plugins::html::add(&mut md);
    let source = "# 雪\n\nA *small* document.\n";

    let expected = md.parse(source).render();
    let document = md.parse_document(source);

    assert_eq!(document.source(), source);
    assert_eq!(document.into_legacy().render(), expected);
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

    for event in document.events(document.root()).unwrap() {
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
fn node_data_survives_draft_and_legacy_transfers() {
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
        let root = document.node(document.root()).unwrap();
        assert_eq!(root.cast::<Payload>().unwrap().0, "payload");
        assert!(root.cast::<Text>().is_none());
        assert_eq!(root.srcmap().unwrap().get_byte_offsets(), (0, 3));
        assert_eq!(root.attrs(), &vec![("class".into(), "kept".into())]);
        assert_eq!(root.ext().get::<Metadata>().unwrap().0, "metadata");
        assert_eq!(root.children().len(), 1);
        let child = document.node(root.children()[0]).unwrap();
        assert_eq!(child.parent(), Some(root.id()));
        assert_eq!(child.cast::<Text>().unwrap().content, "abc");
    }

    let document = Document::from_draft("abc", draft());
    assert_document(&document);
    let document = Document::from_legacy("abc", document.into_legacy());
    assert_document(&document);
    let document = Document::from_legacy("abc", draft().into_legacy());
    assert_document(&document);
}
