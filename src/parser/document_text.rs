//! Lazy, read-only text projections over arena-backed documents.

use std::iter::FusedIterator;
use std::str::CharIndices;

use crate::parser::document::{Document, InvalidNodeId, NodeId, NodeRef};

/// A semantic boundary between projected text regions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextBoundary {
    /// Treat the boundary as whitespace between adjacent characters.
    Space,
    /// Do not carry text-transform context across this boundary.
    Hard,
}

/// One item in a document's text projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextEvent {
    Char {
        node: NodeId,
        byte_offset: usize,
        ch: char,
        writable: bool,
        nesting_level: u32,
    },
    Boundary(TextBoundary),
    Enter {
        node: NodeId,
        nesting_level: u32,
    },
    Exit {
        node: NodeId,
        nesting_level: u32,
    },
}

/// How one node participates in a text projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextProjectionKind<'a> {
    /// Emit characters that a later edit batch may target.
    Writable(&'a str),
    /// Emit characters for context, but do not allow edits to target them.
    ReadOnly(&'a str),
    /// Emit one semantic boundary before visiting the node's children.
    Boundary(TextBoundary),
    /// Emit no text for this node while still visiting its children.
    Transparent,
}

/// Classifies nodes without restricting third-party [`crate::NodeValue`] types.
pub type TextClassifier = for<'a> fn(NodeRef<'a>) -> TextProjectionKind<'a>;

/// Reusable policy for projecting a document into text events.
#[derive(Clone, Copy)]
pub struct TextProjection {
    classifier: TextClassifier,
}

impl TextProjection {
    pub const fn new(classifier: TextClassifier) -> Self {
        Self { classifier }
    }
}

enum FrameProjection<'a> {
    Writable(CharIndices<'a>),
    ReadOnly(CharIndices<'a>),
    Boundary(TextBoundary),
    Transparent,
}

struct Frame<'a> {
    node: NodeId,
    projection: FrameProjection<'a>,
    children: std::slice::Iter<'a, NodeId>,
    phase: Phase,
    nesting_level: u32,
}

#[derive(Clone, Copy)]
enum Phase {
    Enter,
    Content,
    Children,
    Exit,
}

/// Lazy iterator over one text projection.
pub struct TextEvents<'a> {
    document: &'a Document,
    classifier: TextClassifier,
    stack: Vec<Frame<'a>>,
}

impl<'a> TextEvents<'a> {
    fn new(document: &'a Document, root: NodeId, projection: TextProjection) -> Self {
        let mut events = Self {
            document,
            classifier: projection.classifier,
            stack: Vec::with_capacity(16),
        };
        let root = document
            .node(root)
            .expect("text event root was validated before iterator construction");
        events.push(root, 0);
        events
    }

    fn push(&mut self, node: NodeRef<'a>, nesting_level: u32) {
        let projection = match (self.classifier)(node) {
            TextProjectionKind::Writable(content) => {
                FrameProjection::Writable(content.char_indices())
            }
            TextProjectionKind::ReadOnly(content) => {
                FrameProjection::ReadOnly(content.char_indices())
            }
            TextProjectionKind::Boundary(boundary) => FrameProjection::Boundary(boundary),
            TextProjectionKind::Transparent => FrameProjection::Transparent,
        };
        self.stack.push(Frame {
            node: node.id(),
            projection,
            children: node.children().iter(),
            phase: Phase::Enter,
            nesting_level,
        });
    }
}

impl Iterator for TextEvents<'_> {
    type Item = TextEvent;

    // explicit stack + node state machine
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let frame = self.stack.last_mut()?;
            match frame.phase {
                Phase::Enter => {
                    frame.phase = Phase::Content;
                    return Some(TextEvent::Enter {
                        node: frame.node,
                        nesting_level: frame.nesting_level,
                    });
                }
                Phase::Content => match &mut frame.projection {
                    FrameProjection::Writable(chars) => {
                        if let Some((byte_offset, ch)) = chars.next() {
                            return Some(TextEvent::Char {
                                node: frame.node,
                                byte_offset,
                                ch,
                                writable: true,
                                nesting_level: frame.nesting_level,
                            });
                        }
                        frame.phase = Phase::Children;
                    }
                    FrameProjection::ReadOnly(chars) => {
                        if let Some((byte_offset, ch)) = chars.next() {
                            return Some(TextEvent::Char {
                                node: frame.node,
                                byte_offset,
                                ch,
                                writable: false,
                                nesting_level: frame.nesting_level,
                            });
                        }
                        frame.phase = Phase::Children;
                    }
                    FrameProjection::Boundary(boundary) => {
                        let boundary = *boundary;
                        frame.phase = Phase::Children;
                        return Some(TextEvent::Boundary(boundary));
                    }
                    FrameProjection::Transparent => frame.phase = Phase::Children,
                },
                Phase::Children => {
                    if let Some(&child) = frame.children.next() {
                        let nesting_level = frame
                            .nesting_level
                            .checked_add(1)
                            .expect("document nesting level exceeds u32");
                        let child = self
                            .document
                            .node(child)
                            .expect("document tree is internally valid");
                        self.push(child, nesting_level);
                    } else {
                        frame.phase = Phase::Exit;
                    }
                }
                Phase::Exit => {
                    let event = TextEvent::Exit {
                        node: frame.node,
                        nesting_level: frame.nesting_level,
                    };
                    self.stack.pop();
                    return Some(event);
                }
            }
        }
    }
}

impl FusedIterator for TextEvents<'_> {}

impl Document {
    /// Project the whole document into a lazy text event stream.
    pub fn text_events(&self, projection: TextProjection) -> TextEvents<'_> {
        TextEvents::new(self, self.root(), projection)
    }

    /// Project one document subtree, rejecting an invalid or stale root ID.
    pub fn text_events_from(
        &self,
        root: NodeId,
        projection: TextProjection,
    ) -> Result<TextEvents<'_>, InvalidNodeId> {
        self.node(root)?;
        Ok(TextEvents::new(self, root, projection))
    }
}

#[cfg(test)]
mod tests {
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
        let paragraph = document.children(document.root()).unwrap()[0];
        let text = document.children(paragraph).unwrap()[0];
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
        let paragraph = document.children(root).unwrap()[0];
        let children = document.children(paragraph).unwrap();
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
        let paragraph = document.children(document.root()).unwrap()[0];
        let mut events = document
            .text_events_from(paragraph, TextProjection::new(classify))
            .unwrap();

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
}
