//! Text payloads and lazy, read-only text projections over documents.

use std::iter::FusedIterator;
use std::str::CharIndices;

use crate::document::{Document, Node, NodeId};

/// Plain text AST node.
///
/// ```rust
/// use markdown_it::document::{Document, Root, Text};
///
/// let mut document = Document::new("", Root::new(""));
/// let id = document.create_node(Text::new("hello"));
/// assert_eq!(document.value::<Text>(id).unwrap().as_str(), "hello");
///
/// document.value_mut::<Text>(id).unwrap().set_content("world");
/// assert_eq!(document.text(id), Some("world"));
/// ```
#[derive(Debug)]
pub struct Text {
    content: String,
}

impl Text {
    /// Create a text payload that owns `content`.
    #[inline]
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
        }
    }

    /// Borrow the decoded UTF-8 content without allocating.
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.content
    }

    /// Replace the content while preserving source map, attributes, and extensions.
    ///
    /// Prefer [`EditBatch::replace_text`](crate::document::edit::EditBatch::replace_text)
    /// for byte-range edits collected during traversal.
    #[inline]
    pub fn set_content(&mut self, content: impl Into<String>) {
        self.content = content.into();
    }
}

impl Document {
    /// Borrow the current content of a built-in [`Text`] node without copying.
    ///
    /// # Panics
    /// Panics if the ID is invalid or stale. It must originate from this document.
    #[inline]
    #[track_caller]
    pub fn text(&self, id: NodeId) -> Option<&str> {
        self.value::<Text>(id).map(|text| text.content.as_str())
    }

    /// Obtain mutable owned content for a built-in [`Text`] node (may allocate).
    ///
    /// Prefer [`Document::text`] for reads and [`Text::set_content`] or
    /// [`EditBatch`](crate::document::edit::EditBatch) for ordinary edits. Leaves `srcmap` unchanged.
    ///
    /// # Panics
    /// Panics if the ID is invalid or stale. It must originate from this document.
    #[inline]
    #[track_caller]
    pub fn materialize_text(&mut self, id: NodeId) -> Option<&mut String> {
        self.value_mut::<Text>(id).map(|text| &mut text.content)
    }
}

/// Escaped text AST node (backslash escapes and entities).
///
/// ```rust
/// use markdown_it::document::TextSpecial;
///
/// let amp = TextSpecial::new("&", "&amp;", "entity");
/// assert_eq!((amp.content(), amp.markup(), amp.info()), ("&", "&amp;", "entity"));
/// ```
#[derive(Debug)]
pub struct TextSpecial {
    content: String,
    markup: String,
    info: &'static str,
}

impl TextSpecial {
    /// Create an escaped text payload from decoded `content`, original `markup`, and escape `info`.
    #[inline]
    pub fn new(content: impl Into<String>, markup: impl Into<String>, info: &'static str) -> Self {
        Self {
            content: content.into(),
            markup: markup.into(),
            info,
        }
    }

    /// Borrow the decoded content. This never allocates.
    #[inline]
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Borrow the original markup that produced this node.
    #[inline]
    pub fn markup(&self) -> &str {
        &self.markup
    }

    /// The kind of escape that produced this node.
    #[inline]
    pub fn info(&self) -> &'static str {
        self.info
    }

    /// Replace the decoded content, keeping markup and info.
    ///
    /// The node's source map, attributes, and extensions are left unchanged.
    #[inline]
    pub fn set_content(&mut self, content: impl Into<String>) {
        self.content = content.into();
    }
}

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

/// Classifies a node, optionally using the document for text, payloads, or source.
/// Writable content must match the node's editable text and byte offsets.
pub type TextClassifier = for<'a> fn(&'a Document, &'a Node) -> TextProjectionKind<'a>;

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
    fn new(document: &'a Document, root: NodeId, classifier: TextClassifier) -> Self {
        let mut events = Self {
            document,
            classifier,
            stack: Vec::with_capacity(16),
        };
        let root = document.node(root);
        events.push(root, 0);
        events
    }

    fn push(&mut self, node: &'a Node, nesting_level: u32) {
        let projection = match (self.classifier)(self.document, node) {
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
    ///
    /// ```
    /// use markdown_it::document::{Document, Node};
    /// use markdown_it::MarkdownIt;
    /// use markdown_it::document::text::{TextEvent, TextProjectionKind};
    ///
    /// fn classify<'a>(doc: &'a Document, node: &'a Node) -> TextProjectionKind<'a> {
    ///     match doc.text(node.id()) {
    ///         Some(text) => TextProjectionKind::Writable(text),
    ///         None => TextProjectionKind::Transparent,
    ///     }
    /// }
    ///
    /// let doc = MarkdownIt::new().parse("a *雪*");
    /// let text: String = doc.text_events(classify).filter_map(|event| match event {
    ///     TextEvent::Char { ch, .. } => Some(ch),
    ///     _ => None,
    /// }).collect();
    /// assert_eq!(text, "a 雪");
    /// ```
    pub fn text_events(&self, classifier: TextClassifier) -> TextEvents<'_> {
        TextEvents::new(self, self.root(), classifier)
    }

    /// Project one document subtree into text events.
    ///
    /// # Panics
    /// Panics if the root ID is invalid or stale.
    #[track_caller]
    pub fn text_events_from(&self, root: NodeId, classifier: TextClassifier) -> TextEvents<'_> {
        TextEvents::new(self, root, classifier)
    }
}

#[cfg(test)]
mod tests;
