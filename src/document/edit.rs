//! Transactional edits for arena-backed documents.

use std::collections::HashSet;
use std::fmt;
use std::ops::Range;

use crate::common::sourcemap::SourcePos;
use crate::document::{Document, InvalidNodeId, NodeDraft, NodeId, SiblingPosition};
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

#[derive(Clone, Debug)]
struct EditSourceMap {
    node: NodeId,
    source_map: Option<SourcePos>,
}

#[derive(Debug, Default)]
struct NodePatchSet {
    text: Vec<ReplaceText>,
    attributes: Vec<EditAttribute>,
    source_maps: Vec<EditSourceMap>,
}

impl NodePatchSet {
    fn len(&self) -> usize {
        self.text.len() + self.attributes.len() + self.source_maps.len()
    }

    fn is_empty(&self) -> bool {
        self.text.is_empty() && self.attributes.is_empty() && self.source_maps.is_empty()
    }

    fn edited_nodes(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.text
            .iter()
            .map(|edit| edit.node)
            .chain(self.attributes.iter().map(|edit| edit.node))
            .chain(self.source_maps.iter().map(|edit| edit.node))
    }
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

#[derive(Debug)]
struct WrapRange {
    first: NodeId,
    last: NodeId,
    wrapper: NodeDraft,
}

#[derive(Debug, Default)]
struct StructuralEditSet {
    removals: Vec<NodeId>,
    insertions: Vec<InsertSibling>,
    replacements: Vec<ReplaceNode>,
    wraps: Vec<WrapRange>,
}

impl StructuralEditSet {
    fn len(&self) -> usize {
        self.removals.len() + self.insertions.len() + self.replacements.len() + self.wraps.len()
    }

    fn is_empty(&self) -> bool {
        self.removals.is_empty()
            && self.insertions.is_empty()
            && self.replacements.is_empty()
            && self.wraps.is_empty()
    }
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
    ConflictingSourceMapEdits {
        node: NodeId,
    },
    InvalidSourceMap {
        node: NodeId,
        start: usize,
        end: usize,
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
    CannotWrapRoot(NodeId),
    WrapEndpointsHaveDifferentParents {
        first: NodeId,
        last: NodeId,
    },
    ReversedWrapRange {
        first: NodeId,
        last: NodeId,
    },
    WrapperDraftHasChildren {
        first: NodeId,
        last: NodeId,
    },
    OverlappingWrapRanges {
        first_range: (NodeId, NodeId),
        second_range: (NodeId, NodeId),
    },
    WrapRangeTargetsRemovedNode {
        removed: NodeId,
        first: NodeId,
        last: NodeId,
    },
    WrapRangeTargetsReplacedNode {
        replaced: NodeId,
        first: NodeId,
        last: NodeId,
    },
    InsertionTargetsWrappedNode {
        first: NodeId,
        last: NodeId,
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
            Self::ConflictingSourceMapEdits { node } => {
                write!(f, "conflicting source map edits on node {node:?}")
            }
            Self::InvalidSourceMap { node, start, end } => {
                write!(f, "invalid source map {start}..{end} for node {node:?}")
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
            Self::CannotWrapRoot(node) => {
                write!(f, "cannot wrap document root {node:?}")
            }
            Self::WrapEndpointsHaveDifferentParents { first, last } => write!(
                f,
                "wrap endpoints {first:?} and {last:?} have different parents"
            ),
            Self::ReversedWrapRange { first, last } => {
                write!(f, "wrap range {first:?} through {last:?} is reversed")
            }
            Self::WrapperDraftHasChildren { first, last } => write!(
                f,
                "wrapper draft for range {first:?} through {last:?} already has children"
            ),
            Self::OverlappingWrapRanges {
                first_range,
                second_range,
            } => write!(
                f,
                "wrap ranges {:?} through {:?} and {:?} through {:?} overlap",
                first_range.0, first_range.1, second_range.0, second_range.1
            ),
            Self::WrapRangeTargetsRemovedNode {
                removed,
                first,
                last,
            } => write!(
                f,
                "wrap range {first:?} through {last:?} is inside removed subtree {removed:?}"
            ),
            Self::WrapRangeTargetsReplacedNode {
                replaced,
                first,
                last,
            } => write!(
                f,
                "wrap range {first:?} through {last:?} is inside replaced subtree {replaced:?}"
            ),
            Self::InsertionTargetsWrappedNode {
                first,
                last,
                target,
            } => write!(
                f,
                "insertion targets wrapped sibling {target:?} in range {first:?} through {last:?}"
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
    node_patches: NodePatchSet,
    structural_edits: StructuralEditSet,
}

impl EditBatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.node_patches.len() + self.structural_edits.len()
    }

    pub fn is_empty(&self) -> bool {
        self.node_patches.is_empty() && self.structural_edits.is_empty()
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
        self.node_patches.attributes.push(EditAttribute {
            node,
            name: name.into(),
            change: AttributeChange::Set(value.into()),
        });
    }

    /// Remove every attribute with the same exact name.
    pub fn remove_attribute(&mut self, node: NodeId, name: impl Into<String>) {
        self.node_patches.attributes.push(EditAttribute {
            node,
            name: name.into(),
            change: AttributeChange::Remove,
        });
    }

    /// Set or clear a node's source mapping.
    pub fn set_source_map(&mut self, node: NodeId, source_map: Option<SourcePos>) {
        self.node_patches
            .source_maps
            .push(EditSourceMap { node, source_map });
    }

    /// Remove a non-root node and its complete subtree.
    pub fn remove_node(&mut self, node: NodeId) {
        self.structural_edits.removals.push(node);
    }

    /// Insert an owned draft immediately before `target` when the batch
    /// commits. Insertions at the same target preserve call order.
    pub fn insert_before(&mut self, target: NodeId, draft: NodeDraft) {
        self.structural_edits.insertions.push(InsertSibling {
            target,
            position: SiblingPosition::Before,
            draft,
        });
    }

    /// Insert an owned draft immediately after `target` when the batch
    /// commits. Insertions at the same target preserve call order.
    pub fn insert_after(&mut self, target: NodeId, draft: NodeDraft) {
        self.structural_edits.insertions.push(InsertSibling {
            target,
            position: SiblingPosition::After,
            draft,
        });
    }

    /// Replace a non-root node and its complete subtree with an owned draft.
    pub fn replace_node(&mut self, target: NodeId, draft: NodeDraft) {
        self.structural_edits
            .replacements
            .push(ReplaceNode { target, draft });
    }

    /// Wrap an inclusive range of ordered siblings in a childless draft.
    pub fn wrap_range(&mut self, first: NodeId, last: NodeId, wrapper: NodeDraft) {
        self.structural_edits.wraps.push(WrapRange {
            first,
            last,
            wrapper,
        });
    }

    fn push(&mut self, node: NodeId, range: Range<usize>, replacement: TextReplacement) {
        self.node_patches.text.push(ReplaceText {
            node,
            range,
            replacement,
            sequence: self.node_patches.text.len(),
        });
    }

    /// Validate every edit, then apply the whole batch.
    ///
    /// Any returned error leaves `document` unchanged.
    pub fn commit(mut self, document: &mut Document) -> Result<(), EditError> {
        if self.is_empty() {
            return Ok(());
        }

        if self.node_patches.attributes.is_empty()
            && self.node_patches.source_maps.is_empty()
            && self.structural_edits.is_empty()
        {
            self.sort_text_edits();
            self.validate_text(document)?;
            self.apply_text(document);
            return Ok(());
        }
        if self.node_patches.text.is_empty()
            && self.node_patches.source_maps.is_empty()
            && self.structural_edits.is_empty()
        {
            self.sort_attribute_edits();
            self.validate_attributes(document)?;
            self.apply_attributes(document);
            return Ok(());
        }
        if self.node_patches.text.is_empty()
            && self.node_patches.attributes.is_empty()
            && self.structural_edits.is_empty()
        {
            self.sort_source_map_edits();
            self.validate_source_maps(document)?;
            self.apply_source_maps(document);
            return Ok(());
        }
        if self.node_patches.is_empty()
            && self.structural_edits.insertions.is_empty()
            && self.structural_edits.replacements.is_empty()
            && self.structural_edits.wraps.is_empty()
        {
            self.sort_removed_nodes();
            self.validate_removals(document)?;
            self.apply_removals(document);
            return Ok(());
        }
        if self.node_patches.is_empty()
            && self.structural_edits.removals.is_empty()
            && self.structural_edits.replacements.is_empty()
            && self.structural_edits.wraps.is_empty()
        {
            self.validate_insertions(document)?;
            self.apply_insertions(document);
            return Ok(());
        }
        if self.node_patches.is_empty()
            && self.structural_edits.removals.is_empty()
            && self.structural_edits.insertions.is_empty()
            && self.structural_edits.wraps.is_empty()
        {
            self.sort_node_replacements();
            self.validate_replacements(document)?;
            self.apply_replacements(document);
            return Ok(());
        }
        if self.node_patches.is_empty()
            && self.structural_edits.removals.is_empty()
            && self.structural_edits.insertions.is_empty()
            && self.structural_edits.replacements.is_empty()
        {
            self.validate_wrap_ranges(document)?;
            self.apply_wrap_ranges(document);
            return Ok(());
        }

        self.sort_text_edits();
        self.sort_attribute_edits();
        self.sort_source_map_edits();
        self.sort_removed_nodes();
        self.sort_node_replacements();
        self.validate_text(document)?;
        self.validate_attributes(document)?;
        self.validate_source_maps(document)?;
        if !self.structural_edits.removals.is_empty() {
            self.validate_removals(document)?;
        }
        if !self.structural_edits.insertions.is_empty() {
            self.validate_insertions(document)?;
        }
        if !self.structural_edits.replacements.is_empty() {
            self.validate_replacements(document)?;
        }
        if !self.structural_edits.wraps.is_empty() {
            self.validate_wrap_ranges(document)?;
        }
        self.apply_text(document);
        self.apply_removals(document);
        self.apply_replacements(document);
        self.apply_wrap_ranges(document);
        self.apply_insertions(document);
        self.apply_source_maps(document);
        self.apply_attributes(document);
        Ok(())
    }

    fn sort_text_edits(&mut self) {
        self.node_patches.text.sort_unstable_by_key(|edit| {
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
        self.node_patches
            .attributes
            .sort_unstable_by(|left, right| {
                (left.node.slot(), left.node.generation(), &left.name).cmp(&(
                    right.node.slot(),
                    right.node.generation(),
                    &right.name,
                ))
            });
    }

    fn sort_source_map_edits(&mut self) {
        self.node_patches
            .source_maps
            .sort_unstable_by_key(|edit| (edit.node.slot(), edit.node.generation()));
    }

    fn sort_removed_nodes(&mut self) {
        self.structural_edits
            .removals
            .sort_unstable_by_key(|node| (node.slot(), node.generation()));
    }

    fn sort_node_replacements(&mut self) {
        self.structural_edits
            .replacements
            .sort_unstable_by_key(|replacement| {
                (replacement.target.slot(), replacement.target.generation())
            });
    }

    fn validate_text(&self, document: &Document) -> Result<(), EditError> {
        for edits in text_groups(&self.node_patches.text) {
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
        for edits in attribute_groups(&self.node_patches.attributes) {
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

    fn validate_source_maps(&self, document: &Document) -> Result<(), EditError> {
        let mut previous = None;
        for edit in &self.node_patches.source_maps {
            document.node(edit.node)?;
            if previous == Some(edit.node) {
                return Err(EditError::ConflictingSourceMapEdits { node: edit.node });
            }
            if let Some(source_map) = edit.source_map {
                let (start, end) = source_map.get_byte_offsets();
                if start > end
                    || end > document.source().len()
                    || !document.source().is_char_boundary(start)
                    || !document.source().is_char_boundary(end)
                {
                    return Err(EditError::InvalidSourceMap {
                        node: edit.node,
                        start,
                        end,
                    });
                }
            }
            previous = Some(edit.node);
        }
        Ok(())
    }

    fn validate_removals(&self, document: &Document) -> Result<(), EditError> {
        let mut removals = HashSet::with_capacity(self.structural_edits.removals.len());
        for &node in &self.structural_edits.removals {
            document.node(node)?;
            if node == document.root() {
                return Err(EditError::CannotRemoveRoot(node));
            }
            if !removals.insert(node) {
                return Err(EditError::DuplicateNodeRemoval(node));
            }
        }

        for &descendant in &self.structural_edits.removals {
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

        for edited in self.node_patches.edited_nodes() {
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
        let removals: HashSet<_> = self.structural_edits.removals.iter().copied().collect();
        for insertion in &self.structural_edits.insertions {
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
        let mut replacements = HashSet::with_capacity(self.structural_edits.replacements.len());
        for replacement in &self.structural_edits.replacements {
            document.node(replacement.target)?; // check InvalidNode
            if replacement.target == document.root() {
                return Err(EditError::CannotReplaceRoot(replacement.target));
            }
            if !replacements.insert(replacement.target) {
                return Err(EditError::DuplicateNodeReplacement(replacement.target));
            }
        }

        // not allow ancestor/descendant overlap
        for replacement in &self.structural_edits.replacements {
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
        let removals: HashSet<_> = self.structural_edits.removals.iter().copied().collect();
        for replacement in &self.structural_edits.replacements {
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
        for &removed in &self.structural_edits.removals {
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

        for edited in self.node_patches.edited_nodes() {
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

        for insertion in &self.structural_edits.insertions {
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

    fn validate_wrap_ranges(&self, document: &Document) -> Result<(), EditError> {
        let mut resolved = Vec::with_capacity(self.structural_edits.wraps.len());
        for range in &self.structural_edits.wraps {
            let first = document.node(range.first)?;
            let last = document.node(range.last)?;
            let Some(first_parent) = first.parent() else {
                return Err(EditError::CannotWrapRoot(range.first));
            };
            let Some(last_parent) = last.parent() else {
                return Err(EditError::CannotWrapRoot(range.last));
            };
            if first_parent != last_parent {
                return Err(EditError::WrapEndpointsHaveDifferentParents {
                    first: range.first,
                    last: range.last,
                });
            }
            if !range.wrapper.children().is_empty() {
                return Err(EditError::WrapperDraftHasChildren {
                    first: range.first,
                    last: range.last,
                });
            }

            let siblings = document.children(first_parent)?;
            let first_index = siblings
                .iter()
                .position(|&node| node == range.first)
                .expect("document parent links are internally consistent");
            let last_index = siblings
                .iter()
                .position(|&node| node == range.last)
                .expect("document parent links are internally consistent");
            if first_index > last_index {
                return Err(EditError::ReversedWrapRange {
                    first: range.first,
                    last: range.last,
                });
            }
            resolved.push((
                first_parent,
                first_index,
                last_index,
                range.first,
                range.last,
            ));
        }

        resolved
            .sort_unstable_by_key(|range| (range.0.slot(), range.0.generation(), range.1, range.2));
        for ranges in resolved.windows(2) {
            let first = ranges[0];
            let second = ranges[1];
            if first.0 == second.0 && second.1 <= first.2 {
                return Err(EditError::OverlappingWrapRanges {
                    first_range: (first.3, first.4),
                    second_range: (second.3, second.4),
                });
            }
        }

        let removals: HashSet<_> = self.structural_edits.removals.iter().copied().collect();
        let replacements: HashSet<_> = self
            .structural_edits
            .replacements
            .iter()
            .map(|replacement| replacement.target)
            .collect();
        for &(parent, first_index, last_index, first, last) in &resolved {
            let siblings = document.children(parent)?;
            for &selected in &siblings[first_index..=last_index] {
                let mut current = Some(selected);
                while let Some(node) = current {
                    if removals.contains(&node) {
                        return Err(EditError::WrapRangeTargetsRemovedNode {
                            removed: node,
                            first,
                            last,
                        });
                    }
                    if replacements.contains(&node) {
                        return Err(EditError::WrapRangeTargetsReplacedNode {
                            replaced: node,
                            first,
                            last,
                        });
                    }
                    current = document.parent(node)?;
                }
            }
        }

        for insertion in &self.structural_edits.insertions {
            for &(parent, first_index, last_index, first, last) in &resolved {
                let siblings = document.children(parent)?;
                if siblings[first_index..=last_index].contains(&insertion.target) {
                    return Err(EditError::InsertionTargetsWrappedNode {
                        first,
                        last,
                        target: insertion.target,
                    });
                }
            }
        }
        Ok(())
    }

    fn apply_text(&self, document: &mut Document) {
        for edits in text_groups(&self.node_patches.text) {
            let node_id = edits[0].node;
            let text = document
                .node_mut(node_id)
                .expect("validated edit node remains present")
                .cast_mut::<Text>()
                .expect("validated edit node remains a Text node");

            if let [edit] = edits {
                match &edit.replacement {
                    TextReplacement::String(replacement) => {
                        text.content
                            .replace_range(edit.range.clone(), replacement.as_str());
                    }
                    TextReplacement::Char(replacement) => {
                        let mut encoded = [0; 4];
                        text.content.replace_range(
                            edit.range.clone(),
                            replacement.encode_utf8(&mut encoded),
                        );
                    }
                }
                continue;
            }

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

    fn apply_attributes(&mut self, document: &mut Document) {
        for edit in self.node_patches.attributes.drain(..) {
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

    fn apply_source_maps(&mut self, document: &mut Document) {
        for edit in self.node_patches.source_maps.drain(..) {
            document
                .node_mut(edit.node)
                .expect("validated source map edit node remains present")
                .set_srcmap(edit.source_map);
        }
    }

    fn apply_removals(&self, document: &mut Document) {
        if !self.structural_edits.removals.is_empty() {
            document.remove_subtrees(&self.structural_edits.removals);
        }
    }

    fn apply_insertions(&mut self, document: &mut Document) {
        if !self.structural_edits.insertions.is_empty() {
            let insertions = std::mem::take(&mut self.structural_edits.insertions)
                .into_iter()
                .map(|insertion| (insertion.target, insertion.position, insertion.draft))
                .collect();
            document.insert_siblings(insertions);
        }
    }

    fn apply_replacements(&mut self, document: &mut Document) {
        if !self.structural_edits.replacements.is_empty() {
            let replacements = std::mem::take(&mut self.structural_edits.replacements)
                .into_iter()
                .map(|replacement| (replacement.target, replacement.draft))
                .collect();
            document.replace_subtrees(replacements);
        }
    }

    fn apply_wrap_ranges(&mut self, document: &mut Document) {
        if !self.structural_edits.wraps.is_empty() {
            let ranges = std::mem::take(&mut self.structural_edits.wraps)
                .into_iter()
                .map(|range| (range.first, range.last, range.wrapper))
                .collect();
            document.wrap_ranges(ranges);
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
mod tests;
