//! Lazy text projection and deferred text editing

// such complex ?!?!?

use std::ops::Range;

use crate::parser::inline::Text;
use crate::parser::node::Node;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct TextNodeKey(u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TextBoundary {
    Space,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum TextEvent {
    Char {
        node: Option<TextNodeKey>,
        byte_offset: usize,
        ch: char,
        writable: bool,
        nesting_level: u32,
    },
    Boundary(TextBoundary),
    Enter {
        nesting_level: u32,
    },
    Exit {
        nesting_level: u32,
    },
}

#[derive(Clone, Copy)]
pub(crate) enum TextProjectionKind<'a> {
    Writable(&'a Text),
    ReadOnly(&'a str),
    Boundary(TextBoundary),
    Transparent,
}

type Classifier = for<'a> fn(&'a Node) -> TextProjectionKind<'a>;

pub(crate) struct TextProjection {
    classifier: Classifier,
}

impl TextProjection {
    pub(crate) fn new(classifier: Classifier) -> Self {
        Self { classifier }
    }

    pub(crate) fn events<'a>(&self, root: &'a Node) -> TextEvents<'a> {
        TextEvents::new(root, self.classifier)
    }
}

#[derive(Clone, Copy)]
enum FrameProjection<'a> {
    Writable(&'a str, TextNodeKey),
    ReadOnly(&'a str),
    Boundary(TextBoundary),
    Transparent,
}

struct Frame<'a> {
    node: &'a Node,
    projection: FrameProjection<'a>,
    phase: Phase,
    child: usize,
    byte_offset: usize,
    nesting_level: u32,
}

#[derive(Clone, Copy)]
enum Phase {
    Enter,
    Content,
    Children,
    Exit,
}

pub(crate) struct TextEvents<'a> {
    classifier: Classifier,
    stack: Vec<Frame<'a>>,
    next_key: u32,
}

impl<'a> TextEvents<'a> {
    fn new(root: &'a Node, classifier: Classifier) -> Self {
        let mut result = Self {
            classifier,
            stack: Vec::new(),
            next_key: 0,
        };
        result.push(root, 0);
        result
    }

    fn push(&mut self, node: &'a Node, nesting_level: u32) {
        let projection = match (self.classifier)(node) {
            TextProjectionKind::Writable(text) => {
                let key = TextNodeKey(self.next_key);
                self.next_key = self
                    .next_key
                    .checked_add(1)
                    .expect("too many projected text nodes");
                FrameProjection::Writable(&text.content, key)
            }
            TextProjectionKind::ReadOnly(content) => FrameProjection::ReadOnly(content),
            TextProjectionKind::Boundary(boundary) => FrameProjection::Boundary(boundary),
            TextProjectionKind::Transparent => FrameProjection::Transparent,
        };
        self.stack.push(Frame {
            node,
            projection,
            phase: Phase::Enter,
            child: 0,
            byte_offset: 0,
            nesting_level,
        });
    }
}

impl Iterator for TextEvents<'_> {
    type Item = TextEvent;

    /// visit AST, return a `TextEvents` state machine, return None if stack is empty.
    ///
    /// e.g.
    /// root → paragraph → text("a雪")
    /// 0. enter root
    /// 1. enter child (becasue root is `FrameProjection::Transparent`)
    /// 2. push child (paragraph)
    /// 3. enter paragraph
    /// 4. push child (text)
    /// 5. enter child (because paragraph is `FrameProjection::Boundary`)
    /// 6. return `TextEvent::Char{...,'a',...}`
    /// 7. return `TextEvent::Char{...,'雪',...}`
    /// 8. exit text
    /// 9. exit paragraph
    /// 10. exit root
    /// 11. return None
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let frame = self.stack.last_mut()?;
            match frame.phase {
                Phase::Enter => {
                    // switch state machine to `Phase::Content`
                    frame.phase = Phase::Content;
                    return Some(TextEvent::Enter {
                        nesting_level: frame.nesting_level,
                    });
                }
                Phase::Content => match frame.projection {
                    FrameProjection::Writable(content, key) => {
                        if let Some(ch) = content[frame.byte_offset..].chars().next() {
                            let byte_offset = frame.byte_offset;
                            frame.byte_offset += ch.len_utf8();
                            return Some(TextEvent::Char {
                                node: Some(key),
                                byte_offset,
                                ch,
                                writable: true,
                                nesting_level: frame.nesting_level,
                            });
                        }
                        frame.phase = Phase::Children;
                    }
                    FrameProjection::ReadOnly(content) => {
                        if let Some(ch) = content[frame.byte_offset..].chars().next() {
                            let byte_offset = frame.byte_offset;
                            frame.byte_offset += ch.len_utf8();
                            return Some(TextEvent::Char {
                                node: None,
                                byte_offset,
                                ch,
                                writable: false,
                                nesting_level: frame.nesting_level,
                            });
                        }
                        frame.phase = Phase::Children;
                    }
                    FrameProjection::Boundary(boundary) => {
                        frame.phase = Phase::Children;
                        return Some(TextEvent::Boundary(boundary));
                    }
                    FrameProjection::Transparent => frame.phase = Phase::Children,
                },
                Phase::Children => {
                    if let Some(child) = frame.node.children.get(frame.child) {
                        // push child to the top of the stack.
                        // it will be enter in next calling.
                        frame.child += 1;
                        let nesting_level = frame.nesting_level + 1;
                        self.push(child, nesting_level);
                    } else {
                        frame.phase = Phase::Exit;
                    }
                }
                Phase::Exit => {
                    let nesting_level = frame.nesting_level;
                    self.stack.pop();
                    return Some(TextEvent::Exit { nesting_level });
                }
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct ReplaceText {
    pub(crate) node: TextNodeKey,
    pub(crate) range: Range<usize>,
    replacement: TextReplacement,
}

#[derive(Debug)]
#[allow(dead_code)] // Kept for the generic ReplaceText prototype; smartquotes uses Char.
enum TextReplacement {
    String(String),
    Char(char),
}

impl TextReplacement {
    fn len(&self) -> usize {
        match self {
            Self::String(value) => value.len(),
            Self::Char(value) => value.len_utf8(),
        }
    }

    fn push_to(&self, output: &mut String) {
        match self {
            Self::String(value) => output.push_str(value),
            Self::Char(value) => output.push(*value),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum TextEditError {
    UnknownNode(TextNodeKey),
    InvalidRange(TextNodeKey, Range<usize>),
    OverlappingEdits(TextNodeKey),
}

#[derive(Debug)]
pub(crate) struct TextEditBatch {
    classifier: Classifier,
    edits: Vec<ReplaceText>,
}

impl TextEditBatch {
    pub(crate) fn new(classifier: Classifier) -> Self {
        Self {
            classifier,
            edits: Vec::new(),
        }
    }

    #[allow(dead_code)] // Exercised by unit tests and future text transforms.
    pub(crate) fn replace(
        &mut self,
        node: TextNodeKey,
        range: Range<usize>,
        replacement: impl Into<String>,
    ) {
        self.edits.push(ReplaceText {
            node,
            range,
            replacement: TextReplacement::String(replacement.into()),
        });
    }

    pub(crate) fn replace_char(
        &mut self,
        node: TextNodeKey,
        range: Range<usize>,
        replacement: char,
    ) {
        self.edits.push(ReplaceText {
            node,
            range,
            replacement: TextReplacement::Char(replacement),
        });
    }

    pub(crate) fn commit(mut self, root: &mut Node) -> Result<(), TextEditError> {
        if self.edits.is_empty() {
            return Ok(());
        }

        // 0. sort
        self.edits
            .sort_unstable_by_key(|edit| (edit.node, edit.range.start, edit.range.end));

        // 1. validate
        let mut validation_error: Option<TextEditError> = None;
        let mut edit_index = 0; // globel cursor
        let mut key = 0_u32;
        root.walk(|node, _| {
            let TextProjectionKind::Writable(text) = (self.classifier)(node) else {
                return;
            };

            let node_key = TextNodeKey(key);
            key += 1;
            let edit_start = edit_index; // current edit group's start point (snapshot)
            while self
                .edits
                .get(edit_index)
                .is_some_and(|edit| edit.node == node_key)
            {
                // find the end of current edit group
                edit_index += 1;
            }
            // current edit group: &self.edits[edit_start..edit_index]

            let mut previous_end = 0;
            for edit in &self.edits[edit_start..edit_index] {
                if edit.range.start > edit.range.end
                    || edit.range.end > text.content.len()
                    || !text.content.is_char_boundary(edit.range.start)
                    || !text.content.is_char_boundary(edit.range.end)
                {
                    validation_error =
                        Some(TextEditError::InvalidRange(node_key, edit.range.clone()));
                    return;
                }
                if edit.range.start < previous_end {
                    validation_error = Some(TextEditError::OverlappingEdits(node_key));
                    return;
                }
                previous_end = edit.range.end;
            }
        });
        // raise the error
        if let Some(error) = validation_error {
            return Err(error);
        }
        if let Some(edit) = self.edits.get(edit_index) {
            // has a edit not be consume
            return Err(TextEditError::UnknownNode(edit.node));
        }

        // 2. apply edits
        let mut edit_index = 0;
        let mut key = 0_u32;
        root.walk_mut(|node, _| {
            let writable = matches!((self.classifier)(node), TextProjectionKind::Writable(_));
            if !writable {
                return;
            }
            let Some(text) = node.cast_mut::<Text>() else {
                unreachable!("TextProjectionKind::Writable can only contain Text nodes");
            };

            let node_key = TextNodeKey(key);
            key += 1;
            let edit_start = edit_index;
            while self
                .edits
                .get(edit_index)
                .is_some_and(|edit| edit.node == node_key)
            {
                edit_index += 1;
            }
            let edits = &self.edits[edit_start..edit_index];
            if edits.is_empty() {
                return;
            }

            // apply
            let replacement_bytes = edits
                .iter()
                .map(|edit| edit.replacement.len())
                .sum::<usize>();
            let replaced_bytes = edits
                .iter()
                .map(|edit| edit.range.end - edit.range.start)
                .sum::<usize>();
            let mut content = String::with_capacity(
                text.content.len() + replacement_bytes.saturating_sub(replaced_bytes),
            );
            let mut copied_until = 0;
            for edit in edits {
                // finish a edit
                //
                // "abcd" -- edit (1,2) to "cb"
                // content="acb", copied_until=2
                content.push_str(&text.content[copied_until..edit.range.start]);
                edit.replacement.push_to(&mut content);
                copied_until = edit.range.end;
            }
            content.push_str(&text.content[copied_until..]);
            text.content = content;
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::node::NodeEmpty;

    fn text(content: &str) -> Node {
        Node::new(Text {
            content: content.to_owned(),
        })
    }

    fn classify(node: &Node) -> TextProjectionKind<'_> {
        node.cast::<Text>()
            .map(TextProjectionKind::Writable)
            .unwrap_or(TextProjectionKind::Transparent)
    }

    fn classify_second_text(node: &Node) -> TextProjectionKind<'_> {
        let Some(text) = node.cast::<Text>() else {
            return TextProjectionKind::Transparent;
        };
        if text.content == "write" {
            TextProjectionKind::Writable(text)
        } else {
            TextProjectionKind::ReadOnly(&text.content)
        }
    }

    #[test]
    fn projection_is_lazy_and_uses_byte_offsets() {
        let mut root = Node::new(NodeEmpty);
        root.children.push(text("a雪"));
        let events: Vec<_> = TextProjection::new(classify).events(&root).collect();
        assert!(events.iter().any(|event| matches!(
            event,
            TextEvent::Char {
                byte_offset: 1,
                ch: '雪',
                writable: true,
                ..
            }
        )));
    }

    #[test]
    fn batch_rebuilds_each_text_node_once() {
        let mut root = Node::new(NodeEmpty);
        root.children.push(text("a雪c"));
        let mut batch = TextEditBatch::new(classify);
        batch.replace(TextNodeKey(0), 0..1, "A");
        batch.replace(TextNodeKey(0), 1..4, "雨");
        batch.commit(&mut root).unwrap();
        assert_eq!(root.children[0].cast::<Text>().unwrap().content, "A雨c");
    }

    #[test]
    fn keys_count_only_writable_text_nodes() {
        let mut root = Node::new(NodeEmpty);
        root.children.push(text("read"));
        root.children.push(text("write"));
        let mut batch = TextEditBatch::new(classify_second_text);
        batch.replace(TextNodeKey(0), 0..5, "changed");
        batch.commit(&mut root).unwrap();
        assert_eq!(root.children[0].cast::<Text>().unwrap().content, "read");
        assert_eq!(root.children[1].cast::<Text>().unwrap().content, "changed");
    }

    #[test]
    fn invalid_batch_is_atomic() {
        let mut root = Node::new(NodeEmpty);
        root.children.push(text("雪"));
        let mut batch = TextEditBatch::new(classify);
        batch.replace(TextNodeKey(0), 0..3, "雨");
        batch.replace(TextNodeKey(0), 1..2, "x");
        assert!(matches!(
            batch.commit(&mut root),
            Err(TextEditError::InvalidRange(TextNodeKey(0), _))
        ));
        assert_eq!(root.children[0].cast::<Text>().unwrap().content, "雪");
    }

    #[test]
    fn rejects_overlapping_and_unknown_edits() {
        let mut root = Node::new(NodeEmpty);
        root.children.push(text("abcd"));
        let mut overlap = TextEditBatch::new(classify);
        overlap.replace(TextNodeKey(0), 0..2, "x");
        overlap.replace(TextNodeKey(0), 1..3, "y");
        assert_eq!(
            overlap.commit(&mut root),
            Err(TextEditError::OverlappingEdits(TextNodeKey(0)))
        );

        let mut unknown = TextEditBatch::new(classify);
        unknown.replace(TextNodeKey(1), 0..0, "x");
        assert_eq!(
            unknown.commit(&mut root),
            Err(TextEditError::UnknownNode(TextNodeKey(1)))
        );
    }
}
