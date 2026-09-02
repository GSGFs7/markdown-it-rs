//! Transactional edits for arena-backed documents.

use std::fmt;
use std::ops::Range;

use crate::parser::document::{Document, InvalidNodeId, NodeId};
use crate::parser::inline::Text;

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
struct ReplaceText {
    node: NodeId,
    range: Range<usize>,
    replacement: TextReplacement,
    sequence: usize,
}

#[derive(Clone, Debug)]
enum AttributeChange {
    Set(String),
    Remove,
}

#[derive(Clone, Debug)]
struct EditAttribute {
    node: NodeId,
    name: String,
    change: AttributeChange,
}

/// A validation failure that leaves the document unchanged.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EditError {
    InvalidNode(InvalidNodeId),
    NotEditableText(NodeId),
    InvalidTextRange {
        node: NodeId,
        range: Range<usize>,
    },
    OverlappingTextEdits {
        node: NodeId,
        first: Range<usize>,
        second: Range<usize>,
    },
    TextLengthOverflow(NodeId),
    ConflictingAttributeEdits {
        node: NodeId,
        name: String,
    },
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidNode(error) => error.fmt(f),
            Self::NotEditableText(node) => {
                write!(f, "node {node:?} is not an editable Text node")
            }
            Self::InvalidTextRange { node, range } => {
                write!(f, "invalid UTF-8 text range {range:?} for node {node:?}")
            }
            Self::OverlappingTextEdits {
                node,
                first,
                second,
            } => write!(
                f,
                "overlapping text ranges {first:?} and {second:?} for node {node:?}"
            ),
            Self::TextLengthOverflow(node) => {
                write!(f, "edited text length overflows usize for node {node:?}")
            }
            Self::ConflictingAttributeEdits { node, name } => {
                write!(
                    f,
                    "conflicting edits for attribute {name:?} on node {node:?}"
                )
            }
        }
    }
}

impl std::error::Error for EditError {}

impl From<InvalidNodeId> for EditError {
    fn from(value: InvalidNodeId) -> Self {
        Self::InvalidNode(value)
    }
}

/// A batch of document edits that is validated and committed atomically.
#[derive(Clone, Debug, Default)]
pub struct EditBatch {
    text_edits: Vec<ReplaceText>,
    attribute_edits: Vec<EditAttribute>,
}

impl EditBatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.text_edits.len() + self.attribute_edits.len()
    }

    pub fn is_empty(&self) -> bool {
        self.text_edits.is_empty() && self.attribute_edits.is_empty()
    }

    /// Replace one byte range in a built-in `Text` node.
    pub fn replace_text(
        &mut self,
        node: NodeId,
        range: Range<usize>,
        replacement: impl Into<String>,
    ) {
        self.push(node, range, TextReplacement::String(replacement.into()));
    }

    /// Replace one byte range without allocating a temporary replacement
    /// string. This is useful for character-oriented transforms.
    pub fn replace_char(&mut self, node: NodeId, range: Range<usize>, replacement: char) {
        self.push(node, range, TextReplacement::Char(replacement));
    }

    /// Set an attribute, replacing every existing value with the same exact
    /// name with one value.
    pub fn set_attribute(
        &mut self,
        node: NodeId,
        name: impl Into<String>,
        value: impl Into<String>,
    ) {
        self.attribute_edits.push(EditAttribute {
            node,
            name: name.into(),
            change: AttributeChange::Set(value.into()),
        });
    }

    /// Remove every attribute with the same exact name.
    pub fn remove_attribute(&mut self, node: NodeId, name: impl Into<String>) {
        self.attribute_edits.push(EditAttribute {
            node,
            name: name.into(),
            change: AttributeChange::Remove,
        });
    }

    fn push(&mut self, node: NodeId, range: Range<usize>, replacement: TextReplacement) {
        self.text_edits.push(ReplaceText {
            node,
            range,
            replacement,
            sequence: self.text_edits.len(),
        });
    }

    /// Validate every edit, then apply the whole batch.
    ///
    /// Any returned error leaves `document` unchanged.
    pub fn commit(mut self, document: &mut Document) -> Result<(), EditError> {
        if self.is_empty() {
            return Ok(());
        }

        self.text_edits.sort_unstable_by_key(|edit| {
            (
                edit.node.slot(),
                edit.node.generation(),
                edit.range.start,
                edit.range.end,
                edit.sequence,
            )
        });
        self.attribute_edits.sort_unstable_by(|left, right| {
            (left.node.slot(), left.node.generation(), &left.name).cmp(&(
                right.node.slot(),
                right.node.generation(),
                &right.name,
            ))
        });

        self.validate(document)?;
        self.apply_text(document);
        self.apply_attributes(document);
        Ok(())
    }

    fn validate(&self, document: &Document) -> Result<(), EditError> {
        for edits in text_groups(&self.text_edits) {
            let node_id = edits[0].node;
            let node = document.node(node_id)?;
            let Some(text) = node.cast::<Text>() else {
                return Err(EditError::NotEditableText(node_id));
            };

            let mut previous: Option<&ReplaceText> = None;
            let mut final_len = text.content.len();
            for edit in edits {
                if edit.range.start > edit.range.end
                    || edit.range.end > text.content.len()
                    || !text.content.is_char_boundary(edit.range.start)
                    || !text.content.is_char_boundary(edit.range.end)
                {
                    return Err(EditError::InvalidTextRange {
                        node: node_id,
                        range: edit.range.clone(),
                    });
                }
                if let Some(previous) = previous
                    && edit.range.start < previous.range.end
                {
                    return Err(EditError::OverlappingTextEdits {
                        node: node_id,
                        first: previous.range.clone(),
                        second: edit.range.clone(),
                    });
                }
                final_len = final_len
                    .checked_sub(edit.range.end - edit.range.start)
                    .and_then(|len| len.checked_add(edit.replacement.len()))
                    .ok_or(EditError::TextLengthOverflow(node_id))?;
                previous = Some(edit);
            }
        }

        // reject problematic edit
        for edits in attribute_groups(&self.attribute_edits) {
            let edit = &edits[0];
            document.node(edit.node)?;
            if edits.len() > 1 {
                return Err(EditError::ConflictingAttributeEdits {
                    node: edit.node,
                    name: edit.name.clone(),
                });
            }
        }
        Ok(())
    }

    fn apply_text(&self, document: &mut Document) {
        for edits in text_groups(&self.text_edits) {
            let node_id = edits[0].node;
            let text = document
                .node_mut(node_id)
                .expect("validated edit node remains present")
                .cast_mut::<Text>()
                .expect("validated edit node remains a Text node");

            let capacity = edits.iter().fold(text.content.len(), |len, edit| {
                len - (edit.range.end - edit.range.start) + edit.replacement.len()
            });
            let mut content = String::with_capacity(capacity);
            let mut copied_until = 0;
            for edit in edits {
                content.push_str(&text.content[copied_until..edit.range.start]);
                edit.replacement.push_to(&mut content);
                copied_until = edit.range.end;
            }
            content.push_str(&text.content[copied_until..]);
            text.content = content;
        }
    }

    fn apply_attributes(self, document: &mut Document) {
        for edit in self.attribute_edits {
            let attrs = document
                .node_mut(edit.node)
                .expect("validated attribute edit node remains present")
                .attrs_mut();
            match edit.change {
                AttributeChange::Set(value) => {
                    if let Some(index) = attrs.iter().position(|attr| attr.0 == edit.name) {
                        attrs[index].1 = value;
                        let mut kept = false;
                        attrs.retain(|attr| {
                            if attr.0 == edit.name {
                                let keep = !kept;
                                kept = true;
                                keep
                            } else {
                                true
                            }
                        });
                    } else {
                        attrs.push((edit.name, value));
                    }
                }
                AttributeChange::Remove => attrs.retain(|attr| attr.0 != edit.name),
            }
        }
    }
}

fn text_groups(edits: &[ReplaceText]) -> impl Iterator<Item = &[ReplaceText]> {
    let mut remaining = edits;
    std::iter::from_fn(move || {
        let first = remaining.first()?;
        let end = remaining
            .iter()
            .position(|edit| edit.node != first.node)
            .unwrap_or(remaining.len());
        let (group, rest) = remaining.split_at(end);
        remaining = rest;
        Some(group)
    })
}

fn attribute_groups(edits: &[EditAttribute]) -> impl Iterator<Item = &[EditAttribute]> {
    let mut remaining = edits;
    std::iter::from_fn(move || {
        let first = remaining.first()?;
        let end = remaining
            .iter()
            .position(|edit| edit.node != first.node || edit.name != first.name)
            .unwrap_or(remaining.len());
        let (group, rest) = remaining.split_at(end);
        remaining = rest;
        Some(group)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Node;
    use crate::parser::core::Root;
    use crate::plugins::cmark::block::paragraph::Paragraph;

    fn document(texts: &[&str]) -> Document {
        let mut root = Node::new(Root::new(texts.join("")));
        for content in texts {
            root.children.push(Node::new(Text {
                content: (*content).to_owned(),
            }));
        }
        Document::from_legacy(texts.join(""), root)
    }

    fn content(document: &Document, node: NodeId) -> &str {
        &document.node(node).unwrap().cast::<Text>().unwrap().content
    }

    #[test]
    fn applies_multiple_edits_to_each_text_node_once() {
        let mut document = document(&["a雪c", "def"]);
        let children = document.children(document.root()).unwrap();
        let first = children[0];
        let second = children[1];
        let mut batch = EditBatch::new();
        batch.replace_text(second, 1..2, "E");
        batch.replace_char(first, 1..4, '雨');
        batch.replace_text(first, 0..1, "A");

        batch.commit(&mut document).unwrap();

        assert_eq!(content(&document, first), "A雨c");
        assert_eq!(content(&document, second), "dEf");
    }

    #[test]
    fn validation_failure_is_atomic_across_nodes() {
        let mut document = document(&["abc", "雪"]);
        let children = document.children(document.root()).unwrap();
        let first = children[0];
        let second = children[1];
        let mut batch = EditBatch::new();
        batch.replace_text(first, 0..1, "A");
        batch.replace_text(second, 1..2, "invalid UTF-8 boundary");

        assert!(matches!(
            batch.commit(&mut document),
            Err(EditError::InvalidTextRange { node, .. }) if node == second
        ));
        assert_eq!(content(&document, first), "abc");
        assert_eq!(content(&document, second), "雪");
    }

    #[test]
    fn rejects_non_text_and_overlapping_ranges() {
        let mut document = document(&["abcd"]);
        let text = document.children(document.root()).unwrap()[0];
        let mut overlap = EditBatch::new();
        overlap.replace_text(text, 0..2, "x");
        overlap.replace_text(text, 1..3, "y");
        assert!(matches!(
            overlap.commit(&mut document),
            Err(EditError::OverlappingTextEdits { node, .. }) if node == text
        ));

        let root = document.root();
        let mut wrong_type = EditBatch::new();
        wrong_type.replace_text(root, 0..0, "x");
        assert_eq!(
            wrong_type.commit(&mut document),
            Err(EditError::NotEditableText(root))
        );

        let mut paragraph = Document::from_legacy("", Node::new(Paragraph));
        let paragraph_id = paragraph.root();
        let mut wrong_type = EditBatch::new();
        wrong_type.replace_text(paragraph_id, 0..0, "x");
        assert_eq!(
            wrong_type.commit(&mut paragraph),
            Err(EditError::NotEditableText(paragraph_id))
        );
    }

    #[test]
    fn adjacent_ranges_and_same_position_insertions_are_deterministic() {
        let mut document = document(&["abcd"]);
        let text = document.children(document.root()).unwrap()[0];
        let mut batch = EditBatch::new();
        batch.replace_text(text, 0..1, "A");
        batch.replace_text(text, 1..2, "B");
        batch.replace_text(text, 2..2, "first");
        batch.replace_text(text, 2..2, "second");

        batch.commit(&mut document).unwrap();
        assert_eq!(content(&document, text), "ABfirstsecondcd");
    }

    #[test]
    fn supports_deletion_and_rejects_reversed_ranges() {
        let mut document = document(&["abcdef"]);
        let text = document.children(document.root()).unwrap()[0];
        let mut delete = EditBatch::new();
        delete.replace_text(text, 1..3, "");
        delete.commit(&mut document).unwrap();
        assert_eq!(content(&document, text), "adef");

        let mut reversed = EditBatch::new();
        let start = 3;
        let end = 1;
        reversed.replace_text(text, start..end, "x");
        assert_eq!(
            reversed.commit(&mut document),
            Err(EditError::InvalidTextRange {
                node: text,
                range: start..end,
            })
        );
        assert_eq!(content(&document, text), "adef");
    }

    #[test]
    fn empty_batch_is_a_no_op() {
        let mut document = document(&["unchanged"]);
        EditBatch::new().commit(&mut document).unwrap();
        let text = document.children(document.root()).unwrap()[0];
        assert_eq!(content(&document, text), "unchanged");
    }

    #[test]
    fn sets_removes_and_normalizes_attributes() {
        let mut root = Node::new(Root::new("text".to_owned()));
        let mut text = Node::new(Text {
            content: "text".to_owned(),
        });
        text.attrs = vec![
            ("class".to_owned(), "old".to_owned()),
            ("id".to_owned(), "remove-me".to_owned()),
            ("class".to_owned(), "duplicate".to_owned()),
            ("data-key".to_owned(), "kept".to_owned()),
        ];
        root.children.push(text);
        let mut document = Document::from_legacy("text", root);
        let text = document.children(document.root()).unwrap()[0];
        let mut batch = EditBatch::new();
        batch.set_attribute(text, "class", "new");
        batch.remove_attribute(text, "id");
        batch.set_attribute(text, "title", "added");

        batch.commit(&mut document).unwrap();

        assert_eq!(
            document.node(text).unwrap().attrs(),
            &[
                ("class".to_owned(), "new".to_owned()),
                ("data-key".to_owned(), "kept".to_owned()),
                ("title".to_owned(), "added".to_owned()),
            ]
        );
    }

    #[test]
    fn text_and_attribute_edits_commit_together() {
        let mut document = document(&["abc"]);
        let text = document.children(document.root()).unwrap()[0];
        let mut batch = EditBatch::new();
        batch.replace_text(text, 0..1, "A");
        batch.set_attribute(text, "data-state", "edited");

        assert_eq!(batch.len(), 2);
        batch.commit(&mut document).unwrap();

        assert_eq!(content(&document, text), "Abc");
        assert_eq!(
            document.node(text).unwrap().attrs(),
            &[("data-state".to_owned(), "edited".to_owned())]
        );
    }

    #[test]
    fn conflicting_attribute_edits_leave_text_and_attributes_unchanged() {
        let mut document = document(&["abc"]);
        let text = document.children(document.root()).unwrap()[0];
        let mut batch = EditBatch::new();
        batch.replace_text(text, 0..1, "A");
        batch.set_attribute(text, "class", "first");
        batch.remove_attribute(text, "class");

        assert_eq!(
            batch.commit(&mut document),
            Err(EditError::ConflictingAttributeEdits {
                node: text,
                name: "class".to_owned(),
            })
        );
        assert_eq!(content(&document, text), "abc");
        assert!(document.node(text).unwrap().attrs().is_empty());
    }

    #[test]
    fn invalid_text_edit_leaves_attributes_unchanged() {
        let mut document = document(&["雪"]);
        let text = document.children(document.root()).unwrap()[0];
        let mut batch = EditBatch::new();
        batch.set_attribute(text, "class", "new");
        batch.replace_text(text, 1..2, "invalid UTF-8 boundary");

        assert!(matches!(
            batch.commit(&mut document),
            Err(EditError::InvalidTextRange { node, .. }) if node == text
        ));
        assert_eq!(content(&document, text), "雪");
        assert!(document.node(text).unwrap().attrs().is_empty());
    }

    #[test]
    fn different_attribute_names_and_case_are_independent() {
        let mut document = document(&["text"]);
        let text = document.children(document.root()).unwrap()[0];
        let mut batch = EditBatch::new();
        batch.set_attribute(text, "class", "lower");
        batch.set_attribute(text, "CLASS", "upper");

        batch.commit(&mut document).unwrap();

        assert_eq!(document.node(text).unwrap().attrs().len(), 2);
    }
}
