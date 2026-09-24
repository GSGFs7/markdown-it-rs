//! Lazy, read-only text projections over arena-backed documents.

use std::iter::FusedIterator;
use std::str::CharIndices;

use crate::document::{Document, NodeId, NodeRef};

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
    #[track_caller]
    fn new(document: &'a Document, root: NodeId, projection: TextProjection) -> Self {
        let mut events = Self {
            document,
            classifier: projection.classifier,
            stack: Vec::with_capacity(16),
        };
        let root = document.node(root);
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
                        let child = self.document.node(child);
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

    /// Project one document subtree into text events.
    ///
    /// # Panics
    /// Panics if the root ID is invalid or stale.
    #[track_caller]
    pub fn text_events_from(&self, root: NodeId, projection: TextProjection) -> TextEvents<'_> {
        TextEvents::new(self, root, projection)
    }
}

#[cfg(test)]
mod tests;
