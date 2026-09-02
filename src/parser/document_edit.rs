//! Transactional edits for arena-backed documents.

use std::collections::HashSet;
use std::fmt;
use std::ops::Range;

use crate::parser::document::{Document, InvalidNodeId, NodeDraft, NodeId, SiblingPosition};
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

#[derive(Debug)]
struct InsertSibling {
    target: NodeId,
    position: SiblingPosition,
    draft: NodeDraft,
}

#[derive(Debug)]
struct ReplaceNode {
    target: NodeId,
    draft: NodeDraft,
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
    CannotRemoveRoot(NodeId),
    DuplicateNodeRemoval(NodeId),
    OverlappingNodeRemovals {
        ancestor: NodeId,
        descendant: NodeId,
    },
    EditTargetsRemovedNode {
        removed: NodeId,
        edited: NodeId,
    },
    CannotInsertSiblingOfRoot(NodeId),
    InsertionTargetsRemovedNode {
        removed: NodeId,
        target: NodeId,
    },
    CannotReplaceRoot(NodeId),
    DuplicateNodeReplacement(NodeId),
    OverlappingNodeReplacements {
        ancestor: NodeId,
        descendant: NodeId,
    },
    ConflictingNodeRemovalAndReplacement {
        removed: NodeId,
        replaced: NodeId,
    },
    EditTargetsReplacedNode {
        replaced: NodeId,
        edited: NodeId,
    },
    InsertionTargetsReplacedNode {
        replaced: NodeId,
        target: NodeId,
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
            Self::CannotRemoveRoot(node) => {
                write!(f, "cannot remove document root {node:?}")
            }
            Self::DuplicateNodeRemoval(node) => {
                write!(f, "node {node:?} is removed more than once")
            }
            Self::OverlappingNodeRemovals {
                ancestor,
                descendant,
            } => write!(
                f,
                "cannot remove both ancestor {ancestor:?} and descendant {descendant:?}"
            ),
            Self::EditTargetsRemovedNode { removed, edited } => write!(
                f,
                "edit targets node {edited:?} inside removed subtree {removed:?}"
            ),
            Self::CannotInsertSiblingOfRoot(node) => {
                write!(f, "cannot insert a sibling of document root {node:?}")
            }
            Self::InsertionTargetsRemovedNode { removed, target } => write!(
                f,
                "insertion targets node {target:?} inside removed subtree {removed:?}"
            ),
            Self::CannotReplaceRoot(node) => {
                write!(f, "cannot replace document root {node:?}")
            }
            Self::DuplicateNodeReplacement(node) => {
                write!(f, "node {node:?} is replaced more than once")
            }
            Self::OverlappingNodeReplacements {
                ancestor,
                descendant,
            } => write!(
                f,
                "cannot replace both ancestor {ancestor:?} and descendant {descendant:?}"
            ),
            Self::ConflictingNodeRemovalAndReplacement { removed, replaced } => write!(
                f,
                "cannot remove subtree {removed:?} and replace overlapping node {replaced:?}"
            ),
            Self::EditTargetsReplacedNode { replaced, edited } => write!(
                f,
                "edit targets node {edited:?} inside replaced subtree {replaced:?}"
            ),
            Self::InsertionTargetsReplacedNode { replaced, target } => write!(
                f,
                "insertion targets node {target:?} inside replaced subtree {replaced:?}"
            ),
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
#[derive(Debug, Default)]
pub struct EditBatch {
    text_edits: Vec<ReplaceText>,
    attribute_edits: Vec<EditAttribute>,
    removed_nodes: Vec<NodeId>,
    sibling_insertions: Vec<InsertSibling>,
    node_replacements: Vec<ReplaceNode>,
}

impl EditBatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.text_edits.len()
            + self.attribute_edits.len()
            + self.removed_nodes.len()
            + self.sibling_insertions.len()
            + self.node_replacements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.text_edits.is_empty()
            && self.attribute_edits.is_empty()
            && self.removed_nodes.is_empty()
            && self.sibling_insertions.is_empty()
            && self.node_replacements.is_empty()
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

    /// Remove a non-root node and its complete subtree.
    pub fn remove_node(&mut self, node: NodeId) {
        self.removed_nodes.push(node);
    }

    /// Insert an owned draft immediately before `target` when the batch
    /// commits. Insertions at the same target preserve call order.
    pub fn insert_before(&mut self, target: NodeId, draft: NodeDraft) {
        self.sibling_insertions.push(InsertSibling {
            target,
            position: SiblingPosition::Before,
            draft,
        });
    }

    /// Insert an owned draft immediately after `target` when the batch
    /// commits. Insertions at the same target preserve call order.
    pub fn insert_after(&mut self, target: NodeId, draft: NodeDraft) {
        self.sibling_insertions.push(InsertSibling {
            target,
            position: SiblingPosition::After,
            draft,
        });
    }

    /// Replace a non-root node and its complete subtree with an owned draft.
    pub fn replace_node(&mut self, target: NodeId, draft: NodeDraft) {
        self.node_replacements.push(ReplaceNode { target, draft });
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

        if self.attribute_edits.is_empty()
            && self.removed_nodes.is_empty()
            && self.sibling_insertions.is_empty()
            && self.node_replacements.is_empty()
        {
            self.sort_text_edits();
            self.validate_text(document)?;
            self.apply_text(document);
            return Ok(());
        }
        if self.text_edits.is_empty()
            && self.removed_nodes.is_empty()
            && self.sibling_insertions.is_empty()
            && self.node_replacements.is_empty()
        {
            self.sort_attribute_edits();
            self.validate_attributes(document)?;
            self.apply_attributes(document);
            return Ok(());
        }
        if self.text_edits.is_empty()
            && self.attribute_edits.is_empty()
            && self.sibling_insertions.is_empty()
            && self.node_replacements.is_empty()
        {
            self.sort_removed_nodes();
            self.validate_removals(document)?;
            self.apply_removals(document);
            return Ok(());
        }
        if self.text_edits.is_empty()
            && self.attribute_edits.is_empty()
            && self.removed_nodes.is_empty()
            && self.node_replacements.is_empty()
        {
            self.validate_insertions(document)?;
            self.apply_insertions(document);
            return Ok(());
        }
        if self.text_edits.is_empty()
            && self.attribute_edits.is_empty()
            && self.removed_nodes.is_empty()
            && self.sibling_insertions.is_empty()
        {
            self.sort_node_replacements();
            self.validate_replacements(document)?;
            self.apply_replacements(document);
            return Ok(());
        }

        self.sort_text_edits();
        self.sort_attribute_edits();
        self.sort_removed_nodes();
        self.sort_node_replacements();
        self.validate_text(document)?;
        self.validate_attributes(document)?;
        if !self.removed_nodes.is_empty() {
            self.validate_removals(document)?;
        }
        if !self.sibling_insertions.is_empty() {
            self.validate_insertions(document)?;
        }
        if !self.node_replacements.is_empty() {
            self.validate_replacements(document)?;
        }
        self.apply_text(document);
        self.apply_removals(document);
        self.apply_replacements(document);
        self.apply_insertions(document);
        self.apply_attributes(document);
        Ok(())
    }

    fn sort_text_edits(&mut self) {
        self.text_edits.sort_unstable_by_key(|edit| {
            (
                edit.node.slot(),
                edit.node.generation(),
                edit.range.start,
                edit.range.end,
                edit.sequence,
            )
        });
    }

    fn sort_attribute_edits(&mut self) {
        self.attribute_edits.sort_unstable_by(|left, right| {
            (left.node.slot(), left.node.generation(), &left.name).cmp(&(
                right.node.slot(),
                right.node.generation(),
                &right.name,
            ))
        });
    }

    fn sort_removed_nodes(&mut self) {
        self.removed_nodes
            .sort_unstable_by_key(|node| (node.slot(), node.generation()));
    }

    fn sort_node_replacements(&mut self) {
        self.node_replacements.sort_unstable_by_key(|replacement| {
            (replacement.target.slot(), replacement.target.generation())
        });
    }

    fn validate_text(&self, document: &Document) -> Result<(), EditError> {
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
        Ok(())
    }

    fn validate_attributes(&self, document: &Document) -> Result<(), EditError> {
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

    fn validate_removals(&self, document: &Document) -> Result<(), EditError> {
        let mut removals = HashSet::with_capacity(self.removed_nodes.len());
        for &node in &self.removed_nodes {
            document.node(node)?;
            if node == document.root() {
                return Err(EditError::CannotRemoveRoot(node));
            }
            if !removals.insert(node) {
                return Err(EditError::DuplicateNodeRemoval(node));
            }
        }

        for &descendant in &self.removed_nodes {
            let mut ancestor = document.parent(descendant)?;
            while let Some(node) = ancestor {
                if removals.contains(&node) {
                    return Err(EditError::OverlappingNodeRemovals {
                        ancestor: node,
                        descendant,
                    });
                }
                ancestor = document.parent(node)?;
            }
        }

        for edited in self
            .text_edits
            .iter()
            .map(|edit| edit.node)
            .chain(self.attribute_edits.iter().map(|edit| edit.node))
        {
            let mut current = Some(edited);
            while let Some(node) = current {
                if removals.contains(&node) {
                    return Err(EditError::EditTargetsRemovedNode {
                        removed: node,
                        edited,
                    });
                }
                current = document.parent(node)?;
            }
        }
        Ok(())
    }

    fn validate_insertions(&self, document: &Document) -> Result<(), EditError> {
        let removals: HashSet<_> = self.removed_nodes.iter().copied().collect();
        for insertion in &self.sibling_insertions {
            let target = document.node(insertion.target)?;
            if target.parent().is_none() {
                return Err(EditError::CannotInsertSiblingOfRoot(insertion.target));
            }

            let mut current = Some(insertion.target);
            while let Some(node) = current {
                if removals.contains(&node) {
                    return Err(EditError::InsertionTargetsRemovedNode {
                        removed: node,
                        target: insertion.target,
                    });
                }
                current = document.parent(node)?;
            }
        }
        Ok(())
    }

    fn validate_replacements(&self, document: &Document) -> Result<(), EditError> {
        let mut replacements = HashSet::with_capacity(self.node_replacements.len());
        for replacement in &self.node_replacements {
            document.node(replacement.target)?; // check InvalidNode
            if replacement.target == document.root() {
                return Err(EditError::CannotReplaceRoot(replacement.target));
            }
            if !replacements.insert(replacement.target) {
                return Err(EditError::DuplicateNodeReplacement(replacement.target));
            }
        }

        // not allow ancestor/descendant overlap
        for replacement in &self.node_replacements {
            let mut ancestor = document.parent(replacement.target)?;
            while let Some(node) = ancestor {
                if replacements.contains(&node) {
                    return Err(EditError::OverlappingNodeReplacements {
                        ancestor: node,
                        descendant: replacement.target,
                    });
                }
                ancestor = document.parent(node)?;
            }
        }

        // bidirectional detection with delete
        let removals: HashSet<_> = self.removed_nodes.iter().copied().collect();
        for replacement in &self.node_replacements {
            let mut current = Some(replacement.target);
            while let Some(node) = current {
                if removals.contains(&node) {
                    return Err(EditError::ConflictingNodeRemovalAndReplacement {
                        removed: node,
                        replaced: replacement.target,
                    });
                }
                current = document.parent(node)?;
            }
        }
        for &removed in &self.removed_nodes {
            let mut current = Some(removed);
            while let Some(node) = current {
                if replacements.contains(&node) {
                    return Err(EditError::ConflictingNodeRemovalAndReplacement {
                        removed,
                        replaced: node,
                    });
                }
                current = document.parent(node)?;
            }
        }

        for edited in self
            .text_edits
            .iter()
            .map(|edit| edit.node)
            .chain(self.attribute_edits.iter().map(|edit| edit.node))
        {
            let mut current = Some(edited);
            while let Some(node) = current {
                if replacements.contains(&node) {
                    return Err(EditError::EditTargetsReplacedNode {
                        replaced: node,
                        edited,
                    });
                }
                current = document.parent(node)?;
            }
        }

        for insertion in &self.sibling_insertions {
            let mut current = Some(insertion.target);
            while let Some(node) = current {
                if replacements.contains(&node) {
                    return Err(EditError::InsertionTargetsReplacedNode {
                        replaced: node,
                        target: insertion.target,
                    });
                }
                current = document.parent(node)?;
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

    fn apply_removals(&self, document: &mut Document) {
        if !self.removed_nodes.is_empty() {
            document.remove_subtrees(&self.removed_nodes);
        }
    }

    fn apply_insertions(&mut self, document: &mut Document) {
        if !self.sibling_insertions.is_empty() {
            let insertions = std::mem::take(&mut self.sibling_insertions)
                .into_iter()
                .map(|insertion| (insertion.target, insertion.position, insertion.draft))
                .collect();
            document.insert_siblings(insertions);
        }
    }

    fn apply_replacements(&mut self, document: &mut Document) {
        if !self.node_replacements.is_empty() {
            let replacements = std::mem::take(&mut self.node_replacements)
                .into_iter()
                .map(|replacement| (replacement.target, replacement.draft))
                .collect();
            document.replace_subtrees(replacements);
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

    fn branched_document() -> Document {
        let mut root = Node::new(Root::new("ab".to_owned()));
        for content in ["a", "b"] {
            let mut paragraph = Node::new(Paragraph);
            paragraph.children.push(Node::new(Text {
                content: content.to_owned(),
            }));
            root.children.push(paragraph);
        }
        Document::from_legacy("ab", root)
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

    #[test]
    fn removes_a_complete_subtree_and_invalidates_all_ids() {
        let mut document = branched_document();
        let root = document.root();
        let branches = document.children(root).unwrap();
        let removed = branches[0];
        let kept = branches[1];
        let removed_text = document.children(removed).unwrap()[0];
        let mut batch = EditBatch::new();
        batch.remove_node(removed);

        batch.commit(&mut document).unwrap();

        assert_eq!(document.len(), 3);
        assert_eq!(document.children(root).unwrap(), &[kept]);
        assert_eq!(document.parent(kept).unwrap(), Some(root));
        assert_eq!(document.node(removed).unwrap_err(), InvalidNodeId(removed));
        assert_eq!(
            document.node(removed_text).unwrap_err(),
            InvalidNodeId(removed_text)
        );
        let legacy = document.into_legacy();
        assert_eq!(legacy.children.len(), 1);
        assert_eq!(
            legacy.children[0].children[0]
                .cast::<Text>()
                .unwrap()
                .content,
            "b"
        );
    }

    #[test]
    fn multiple_sibling_removals_commit_together() {
        let mut document = branched_document();
        let root = document.root();
        let branches = document.children(root).unwrap().to_vec();
        let mut batch = EditBatch::new();
        batch.remove_node(branches[1]);
        batch.remove_node(branches[0]);

        assert_eq!(batch.len(), 2);
        batch.commit(&mut document).unwrap();

        assert_eq!(document.len(), 1);
        assert!(document.children(root).unwrap().is_empty());
    }

    #[test]
    fn rejects_root_duplicate_and_overlapping_removals_atomically() {
        let mut document = branched_document();
        let root = document.root();
        let branch = document.children(root).unwrap()[0];
        let text = document.children(branch).unwrap()[0];

        let mut root_removal = EditBatch::new();
        root_removal.replace_text(text, 0..1, "A");
        root_removal.remove_node(root);
        assert_eq!(
            root_removal.commit(&mut document),
            Err(EditError::CannotRemoveRoot(root))
        );
        assert_eq!(content(&document, text), "a");

        let mut duplicate = EditBatch::new();
        duplicate.set_attribute(root, "class", "changed");
        duplicate.remove_node(branch);
        duplicate.remove_node(branch);
        assert_eq!(
            duplicate.commit(&mut document),
            Err(EditError::DuplicateNodeRemoval(branch))
        );
        assert!(document.node(root).unwrap().attrs().is_empty());

        let mut overlap = EditBatch::new();
        overlap.remove_node(branch);
        overlap.remove_node(text);
        assert_eq!(
            overlap.commit(&mut document),
            Err(EditError::OverlappingNodeRemovals {
                ancestor: branch,
                descendant: text,
            })
        );
        assert_eq!(document.len(), 5);
    }

    #[test]
    fn rejects_edits_inside_a_removed_subtree() {
        let mut document = branched_document();
        let branch = document.children(document.root()).unwrap()[0];
        let text = document.children(branch).unwrap()[0];
        let mut text_conflict = EditBatch::new();
        text_conflict.remove_node(branch);
        text_conflict.replace_text(text, 0..1, "A");
        assert_eq!(
            text_conflict.commit(&mut document),
            Err(EditError::EditTargetsRemovedNode {
                removed: branch,
                edited: text,
            })
        );
        assert_eq!(content(&document, text), "a");

        let mut attribute_conflict = EditBatch::new();
        attribute_conflict.remove_node(branch);
        attribute_conflict.set_attribute(branch, "class", "changed");
        assert_eq!(
            attribute_conflict.commit(&mut document),
            Err(EditError::EditTargetsRemovedNode {
                removed: branch,
                edited: branch,
            })
        );
        assert!(document.node(branch).unwrap().attrs().is_empty());
    }

    #[test]
    fn edits_outside_a_removed_subtree_commit_normally() {
        let mut document = branched_document();
        let root = document.root();
        let branches = document.children(root).unwrap();
        let removed = branches[0];
        let kept = branches[1];
        let kept_text = document.children(kept).unwrap()[0];
        let mut batch = EditBatch::new();
        batch.remove_node(removed);
        batch.replace_text(kept_text, 0..1, "B");
        batch.set_attribute(root, "class", "edited");

        batch.commit(&mut document).unwrap();

        assert_eq!(content(&document, kept_text), "B");
        assert_eq!(document.children(root).unwrap(), &[kept]);
        assert_eq!(
            document.node(root).unwrap().attrs(),
            &[("class".to_owned(), "edited".to_owned())]
        );
    }

    #[test]
    fn stale_removal_target_is_rejected() {
        let mut document = branched_document();
        let removed = document.children(document.root()).unwrap()[0];
        let mut first = EditBatch::new();
        first.remove_node(removed);
        first.commit(&mut document).unwrap();

        let mut stale = EditBatch::new();
        stale.remove_node(removed);
        assert_eq!(
            stale.commit(&mut document),
            Err(EditError::InvalidNode(InvalidNodeId(removed)))
        );
    }

    #[test]
    fn deeply_nested_subtrees_are_removed_iteratively() {
        let mut subtree = Node::new(Text {
            content: "leaf".to_owned(),
        });
        for _ in 0..10_000 {
            let mut parent = Node::new(Paragraph);
            parent.children.push(subtree);
            subtree = parent;
        }
        let mut root = Node::new(Root::new("leaf".to_owned()));
        root.children.push(subtree);
        let mut document = Document::from_legacy("leaf", root);
        let subtree = document.children(document.root()).unwrap()[0];
        let mut batch = EditBatch::new();
        batch.remove_node(subtree);

        batch.commit(&mut document).unwrap();

        assert_eq!(document.len(), 1);
        assert!(document.children(document.root()).unwrap().is_empty());
    }

    fn text_draft(content: &str) -> NodeDraft {
        NodeDraft::new(Text {
            content: content.to_owned(),
        })
    }

    #[test]
    fn inserts_siblings_in_stable_call_order() {
        let mut document = document(&["a", "b"]);
        let root = document.root();
        let original = document.children(root).unwrap().to_vec();
        let target = original[1];
        let mut batch = EditBatch::new();
        batch.insert_before(target, text_draft("before-1"));
        batch.insert_after(target, text_draft("after-1"));
        batch.insert_before(target, text_draft("before-2"));
        batch.insert_after(target, text_draft("after-2"));

        assert_eq!(batch.len(), 4);
        batch.commit(&mut document).unwrap();

        let children = document.children(root).unwrap();
        let contents: Vec<_> = children
            .iter()
            .map(|&node| content(&document, node))
            .collect();
        assert_eq!(
            contents,
            ["a", "before-1", "before-2", "b", "after-1", "after-2"]
        );
        assert_eq!(children[0], original[0]);
        assert_eq!(children[3], target);
        for &inserted in [&children[1], &children[2], &children[4], &children[5]] {
            assert_eq!(document.parent(inserted).unwrap(), Some(root));
            assert!(document.node(inserted).unwrap().srcmap().is_none());
        }
    }

    #[test]
    fn inserts_an_owned_subtree_without_cloning_payloads() {
        #[derive(Debug)]
        struct NonClonePayload(&'static str);
        impl crate::NodeValue for NonClonePayload {}

        let mut document = document(&["anchor"]);
        let root = document.root();
        let anchor = document.children(root).unwrap()[0];
        let mut draft = NodeDraft::new(NonClonePayload("parent"));
        draft
            .attrs_mut()
            .push(("data-generated".to_owned(), "yes".to_owned()));
        draft.push_child(text_draft("child-1"));
        draft.push_child(text_draft("child-2"));
        let mut batch = EditBatch::new();
        batch.insert_before(anchor, draft);

        batch.commit(&mut document).unwrap();

        let inserted = document.children(root).unwrap()[0];
        let node = document.node(inserted).unwrap();
        assert_eq!(node.cast::<NonClonePayload>().unwrap().0, "parent");
        assert_eq!(
            node.attrs(),
            &[("data-generated".to_owned(), "yes".to_owned())]
        );
        let children = node.children();
        assert_eq!(content(&document, children[0]), "child-1");
        assert_eq!(content(&document, children[1]), "child-2");
        assert!(
            children
                .iter()
                .all(|&child| document.parent(child).unwrap() == Some(inserted))
        );
    }

    #[test]
    fn rejects_root_and_stale_insertion_targets_atomically() {
        let mut document = document(&["abc"]);
        let root = document.root();
        let text = document.children(root).unwrap()[0];
        let mut root_target = EditBatch::new();
        root_target.replace_text(text, 0..1, "A");
        root_target.insert_before(root, text_draft("invalid"));
        assert_eq!(
            root_target.commit(&mut document),
            Err(EditError::CannotInsertSiblingOfRoot(root))
        );
        assert_eq!(content(&document, text), "abc");
        assert_eq!(document.len(), 2);

        let mut removal = EditBatch::new();
        removal.remove_node(text);
        removal.commit(&mut document).unwrap();
        let mut stale = EditBatch::new();
        stale.insert_after(text, text_draft("invalid"));
        assert_eq!(
            stale.commit(&mut document),
            Err(EditError::InvalidNode(InvalidNodeId(text)))
        );
        assert_eq!(document.len(), 1);
    }

    #[test]
    fn rejects_insertions_inside_removed_subtrees() {
        let mut document = branched_document();
        let removed = document.children(document.root()).unwrap()[0];
        let target = document.children(removed).unwrap()[0];
        let mut batch = EditBatch::new();
        batch.remove_node(removed);
        batch.insert_before(target, text_draft("invalid"));

        assert_eq!(
            batch.commit(&mut document),
            Err(EditError::InsertionTargetsRemovedNode { removed, target })
        );
        assert_eq!(document.len(), 5);
    }

    #[test]
    fn insertion_next_to_a_kept_subtree_can_commit_with_removal() {
        let mut document = branched_document();
        let root = document.root();
        let branches = document.children(root).unwrap().to_vec();
        let mut batch = EditBatch::new();
        batch.remove_node(branches[0]);
        batch.insert_before(branches[1], text_draft("inserted"));

        batch.commit(&mut document).unwrap();

        let children = document.children(root).unwrap();
        assert_eq!(children.len(), 2);
        assert_eq!(content(&document, children[0]), "inserted");
        assert_eq!(children[1], branches[1]);
    }

    #[test]
    fn deeply_nested_drafts_are_inserted_iteratively() {
        let mut draft = text_draft("leaf");
        for _ in 0..10_000 {
            let mut parent = NodeDraft::new(Paragraph);
            parent.push_child(draft);
            draft = parent;
        }
        let mut document = document(&["anchor"]);
        let anchor = document.children(document.root()).unwrap()[0];
        let mut batch = EditBatch::new();
        batch.insert_before(anchor, draft);

        batch.commit(&mut document).unwrap();

        assert_eq!(document.len(), 10_003);
        assert_eq!(document.children(document.root()).unwrap().len(), 2);
    }

    #[test]
    fn replaces_a_complete_subtree_in_place_and_invalidates_old_ids() {
        let mut document = branched_document();
        let root = document.root();
        let original = document.children(root).unwrap().to_vec();
        let replaced = original[0];
        let replaced_child = document.children(replaced).unwrap()[0];
        let mut draft = NodeDraft::new(Paragraph);
        draft.push_child(text_draft("replacement"));
        let mut batch = EditBatch::new();
        batch.replace_node(replaced, draft);

        assert_eq!(batch.len(), 1);
        batch.commit(&mut document).unwrap();

        let children = document.children(root).unwrap();
        assert_eq!(children.len(), 2);
        assert_eq!(children[1], original[1]);
        let replacement = children[0];
        assert_ne!(replacement, replaced);
        assert!(document.node(replacement).unwrap().is::<Paragraph>());
        assert_eq!(document.parent(replacement).unwrap(), Some(root));
        let replacement_child = document.children(replacement).unwrap()[0];
        assert_eq!(content(&document, replacement_child), "replacement");
        assert_eq!(
            document.parent(replacement_child).unwrap(),
            Some(replacement)
        );
        assert_eq!(
            document.node(replaced).unwrap_err(),
            InvalidNodeId(replaced)
        );
        assert_eq!(
            document.node(replaced_child).unwrap_err(),
            InvalidNodeId(replaced_child)
        );

        let legacy = document.into_legacy();
        assert_eq!(legacy.children.len(), 2);
        assert_eq!(
            legacy.children[0].children[0]
                .cast::<Text>()
                .unwrap()
                .content,
            "replacement"
        );
    }

    #[test]
    fn multiple_sibling_replacements_preserve_structural_order() {
        let mut document = document(&["a", "b", "c"]);
        let root = document.root();
        let original = document.children(root).unwrap().to_vec();
        let mut batch = EditBatch::new();
        batch.replace_node(original[2], text_draft("C"));
        batch.replace_node(original[0], text_draft("A"));

        batch.commit(&mut document).unwrap();

        let children = document.children(root).unwrap();
        let contents: Vec<_> = children
            .iter()
            .map(|&node| content(&document, node))
            .collect();
        assert_eq!(contents, ["A", "b", "C"]);
        assert_eq!(children[1], original[1]);
        assert!(document.node(original[0]).is_err());
        assert!(document.node(original[2]).is_err());
    }

    #[test]
    fn rejects_root_stale_duplicate_and_overlapping_replacements_atomically() {
        let mut document = branched_document();
        let root = document.root();
        let branch = document.children(root).unwrap()[0];
        let text = document.children(branch).unwrap()[0];

        let mut root_replacement = EditBatch::new();
        root_replacement.replace_text(text, 0..1, "A");
        root_replacement.replace_node(root, text_draft("invalid"));
        assert_eq!(
            root_replacement.commit(&mut document),
            Err(EditError::CannotReplaceRoot(root))
        );
        assert_eq!(content(&document, text), "a");

        let mut duplicate = EditBatch::new();
        duplicate.set_attribute(root, "class", "changed");
        duplicate.replace_node(branch, text_draft("first"));
        duplicate.replace_node(branch, text_draft("second"));
        assert_eq!(
            duplicate.commit(&mut document),
            Err(EditError::DuplicateNodeReplacement(branch))
        );
        assert!(document.node(root).unwrap().attrs().is_empty());

        let mut overlap = EditBatch::new();
        overlap.replace_node(branch, text_draft("ancestor"));
        overlap.replace_node(text, text_draft("descendant"));
        assert_eq!(
            overlap.commit(&mut document),
            Err(EditError::OverlappingNodeReplacements {
                ancestor: branch,
                descendant: text,
            })
        );
        assert_eq!(document.len(), 5);

        let mut replace = EditBatch::new();
        replace.replace_node(branch, text_draft("replacement"));
        replace.commit(&mut document).unwrap();
        let mut stale = EditBatch::new();
        stale.replace_node(branch, text_draft("invalid"));
        assert_eq!(
            stale.commit(&mut document),
            Err(EditError::InvalidNode(InvalidNodeId(branch)))
        );
    }

    #[test]
    fn rejects_edits_and_insertions_inside_replaced_subtrees() {
        let mut document = branched_document();
        let branch = document.children(document.root()).unwrap()[0];
        let text = document.children(branch).unwrap()[0];
        let mut text_conflict = EditBatch::new();
        text_conflict.replace_node(branch, text_draft("replacement"));
        text_conflict.replace_text(text, 0..1, "A");
        assert_eq!(
            text_conflict.commit(&mut document),
            Err(EditError::EditTargetsReplacedNode {
                replaced: branch,
                edited: text,
            })
        );
        assert_eq!(content(&document, text), "a");

        let mut attribute_conflict = EditBatch::new();
        attribute_conflict.replace_node(branch, text_draft("replacement"));
        attribute_conflict.set_attribute(branch, "class", "changed");
        assert_eq!(
            attribute_conflict.commit(&mut document),
            Err(EditError::EditTargetsReplacedNode {
                replaced: branch,
                edited: branch,
            })
        );
        assert!(document.node(branch).unwrap().attrs().is_empty());

        let mut insertion_conflict = EditBatch::new();
        insertion_conflict.replace_node(branch, text_draft("replacement"));
        insertion_conflict.insert_before(text, text_draft("inserted"));
        assert_eq!(
            insertion_conflict.commit(&mut document),
            Err(EditError::InsertionTargetsReplacedNode {
                replaced: branch,
                target: text,
            })
        );
        assert_eq!(document.len(), 5);
    }

    #[test]
    fn rejects_overlapping_removal_and_replacement_in_both_directions() {
        let mut document = branched_document();
        let branch = document.children(document.root()).unwrap()[0];
        let text = document.children(branch).unwrap()[0];
        let mut same_target = EditBatch::new();
        same_target.remove_node(branch);
        same_target.replace_node(branch, text_draft("replacement"));
        assert_eq!(
            same_target.commit(&mut document),
            Err(EditError::ConflictingNodeRemovalAndReplacement {
                removed: branch,
                replaced: branch,
            })
        );
        assert_eq!(document.len(), 5);

        let mut remove_ancestor = EditBatch::new();
        remove_ancestor.remove_node(branch);
        remove_ancestor.replace_node(text, text_draft("replacement"));
        assert_eq!(
            remove_ancestor.commit(&mut document),
            Err(EditError::ConflictingNodeRemovalAndReplacement {
                removed: branch,
                replaced: text,
            })
        );
        assert_eq!(document.len(), 5);

        let mut replace_ancestor = EditBatch::new();
        replace_ancestor.replace_node(branch, text_draft("replacement"));
        replace_ancestor.remove_node(text);
        assert_eq!(
            replace_ancestor.commit(&mut document),
            Err(EditError::ConflictingNodeRemovalAndReplacement {
                removed: text,
                replaced: branch,
            })
        );
        assert_eq!(document.len(), 5);
    }

    #[test]
    fn disjoint_structural_and_value_edits_commit_together() {
        let mut document = document(&["a", "b", "c"]);
        let root = document.root();
        let original = document.children(root).unwrap().to_vec();
        let mut batch = EditBatch::new();
        batch.remove_node(original[0]);
        batch.replace_node(original[1], text_draft("replacement"));
        batch.insert_before(original[2], text_draft("inserted"));
        batch.replace_text(original[2], 0..1, "C");
        batch.set_attribute(root, "class", "edited");

        batch.commit(&mut document).unwrap();

        let children = document.children(root).unwrap();
        let contents: Vec<_> = children
            .iter()
            .map(|&node| content(&document, node))
            .collect();
        assert_eq!(contents, ["replacement", "inserted", "C"]);
        assert_eq!(children[2], original[2]);
        assert_eq!(
            document.node(root).unwrap().attrs(),
            &[("class".to_owned(), "edited".to_owned())]
        );
    }

    #[test]
    fn deeply_nested_subtrees_can_be_replaced_iteratively() {
        let mut old_subtree = Node::new(Text {
            content: "old".to_owned(),
        });
        let mut draft = text_draft("new");
        for _ in 0..10_000 {
            let mut old_parent = Node::new(Paragraph);
            old_parent.children.push(old_subtree);
            old_subtree = old_parent;
            let mut draft_parent = NodeDraft::new(Paragraph);
            draft_parent.push_child(draft);
            draft = draft_parent;
        }
        let mut root = Node::new(Root::new("old".to_owned()));
        root.children.push(old_subtree);
        let mut document = Document::from_legacy("old", root);
        let replaced = document.children(document.root()).unwrap()[0];
        let mut batch = EditBatch::new();
        batch.replace_node(replaced, draft);

        batch.commit(&mut document).unwrap();

        assert_eq!(document.len(), 10_002);
        assert_eq!(document.children(document.root()).unwrap().len(), 1);
        assert_eq!(
            document.node(replaced).unwrap_err(),
            InvalidNodeId(replaced)
        );
    }
}
