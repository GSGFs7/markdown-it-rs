use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::parser::core::Root;
use crate::parser::inline::Text;
use crate::plugins::cmark::block::paragraph::Paragraph;
use crate::plugins::cmark::inline::newline::{Hardbreak, Softbreak};
use crate::plugins::html::html_inline::HtmlInline;
use crate::{MarkdownIt, Node, NodeValue, plugins};

#[derive(Debug)]
struct ReadOnlyText(String);

impl NodeValue for ReadOnlyText {}

static CLASSIFICATIONS: AtomicUsize = AtomicUsize::new(0);

fn classify(node: NodeRef<'_>) -> TextProjectionKind<'_> {
    if let Some(text) = node.cast::<Text>() {
        TextProjectionKind::Writable(&text.content)
    } else if let Some(text) = node.cast::<ReadOnlyText>() {
        TextProjectionKind::ReadOnly(&text.0)
    } else if node.is::<Paragraph>() {
        TextProjectionKind::Boundary(TextBoundary::Space)
    } else {
        TextProjectionKind::Transparent
    }
}

fn smartquotes_like(node: NodeRef<'_>) -> TextProjectionKind<'_> {
    if let Some(text) = node.cast::<Text>() {
        TextProjectionKind::Writable(&text.content)
    } else if let Some(html) = node.cast::<HtmlInline>() {
        TextProjectionKind::ReadOnly(&html.content)
    } else if node.is::<Paragraph>() || node.is::<Hardbreak>() || node.is::<Softbreak>() {
        TextProjectionKind::Boundary(TextBoundary::Space)
    } else {
        TextProjectionKind::Transparent
    }
}

fn count_classifications(_: NodeRef<'_>) -> TextProjectionKind<'_> {
    CLASSIFICATIONS.fetch_add(1, Ordering::Relaxed);
    TextProjectionKind::Transparent
}

fn document() -> Document {
    let mut root = Node::new(Root::new("a雪<q>".to_owned()));
    let mut paragraph = Node::new(Paragraph);
    paragraph.children.push(Node::new(Text {
        content: "a雪".to_owned(),
    }));
    paragraph
        .children
        .push(Node::new(ReadOnlyText("<q>".to_owned())));
    root.children.push(paragraph);
    Document::from_legacy("a雪<q>", root)
}

#[test]
fn projection_uses_node_ids_and_utf8_byte_offsets() {
    let document = document();
    let paragraph = document.children(document.root())[0];
    let text = document.children(paragraph)[0];
    let chars: Vec<_> = document
        .text_events(TextProjection::new(classify))
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

    assert_eq!(chars[0], (text, 0, 'a', true));
    assert_eq!(chars[1], (text, 1, '雪', true));
    assert_eq!(chars[2].1, 0);
    assert_eq!(chars[2].2, '<');
    assert!(!chars[2].3);
}

#[test]
fn projection_order_depth_and_boundaries_are_stable() {
    let document = document();
    let root = document.root();
    let paragraph = document.children(root)[0];
    let children = document.children(paragraph);
    let events: Vec<_> = document
        .text_events(TextProjection::new(classify))
        .collect();

    assert_eq!(
        events[0],
        TextEvent::Enter {
            node: root,
            nesting_level: 0
        }
    );
    assert_eq!(
        events[1],
        TextEvent::Enter {
            node: paragraph,
            nesting_level: 1,
        }
    );
    assert_eq!(events[2], TextEvent::Boundary(TextBoundary::Space));
    assert!(matches!(
        events[3],
        TextEvent::Enter { node, nesting_level: 2 } if node == children[0]
    ));
    assert_eq!(
        events.last(),
        Some(&TextEvent::Exit {
            node: root,
            nesting_level: 0,
        })
    );
}

#[test]
fn subtree_projection_uses_the_subtree_as_depth_zero() {
    let document = document();
    let paragraph = document.children(document.root())[0];
    let mut events = document.text_events_from(paragraph, TextProjection::new(classify));

    assert_eq!(
        events.next(),
        Some(TextEvent::Enter {
            node: paragraph,
            nesting_level: 0,
        })
    );
}

#[test]
fn projection_is_lazy_and_fused() {
    let document = document();
    CLASSIFICATIONS.store(0, Ordering::Relaxed);
    let mut events = document.text_events(TextProjection::new(count_classifications));

    assert_eq!(CLASSIFICATIONS.load(Ordering::Relaxed), 1);
    assert!(matches!(events.next(), Some(TextEvent::Enter { .. })));
    assert_eq!(CLASSIFICATIONS.load(Ordering::Relaxed), 1);
    assert!(matches!(events.next(), Some(TextEvent::Enter { .. })));
    assert_eq!(CLASSIFICATIONS.load(Ordering::Relaxed), 2);

    while events.next().is_some() {}
    assert_eq!(events.next(), None);
    assert_eq!(events.next(), None);
}

#[test]
fn real_markdown_preserves_text_html_and_linebreak_semantics() {
    let mut md = MarkdownIt::empty();
    plugins::cmark::add(&mut md);
    plugins::html::add(&mut md);
    let document = md.parse_document("A *雪*\n<b title=\"q\">B</b>  \nC");
    let events: Vec<_> = document
        .text_events(TextProjection::new(smartquotes_like))
        .collect();

    let writable: String = events
        .iter()
        .filter_map(|event| match event {
            TextEvent::Char {
                ch, writable: true, ..
            } => Some(*ch),
            _ => None,
        })
        .collect();
    let read_only: String = events
        .iter()
        .filter_map(|event| match event {
            TextEvent::Char {
                ch,
                writable: false,
                ..
            } => Some(*ch),
            _ => None,
        })
        .collect();
    let spaces = events
        .iter()
        .filter(|event| matches!(event, TextEvent::Boundary(TextBoundary::Space)))
        .count();

    assert_eq!(writable, "A 雪BC");
    assert_eq!(read_only, "<b title=\"q\"></b>");
    assert_eq!(spaces, 3);
}
