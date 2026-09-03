//! Transactional edits for arena-backed documents.

use std::collections::HashSet;
use std::fmt;
use std::ops::Range;

use crate::common::sourcemap::SourcePos;
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
    fn text_attribute_and_source_map_edits_commit_together() {
        let mut document = document(&["abc"]);
        let text = document.children(document.root()).unwrap()[0];
        let mut batch = EditBatch::new();
        batch.replace_text(text, 0..1, "A");
        batch.set_attribute(text, "data-state", "edited");
        batch.set_source_map(text, Some(SourcePos::new(1, 3)));

        assert_eq!(batch.len(), 3);
        batch.commit(&mut document).unwrap();

        assert_eq!(content(&document, text), "Abc");
        assert_eq!(
            document
                .node(text)
                .unwrap()
                .srcmap()
                .unwrap()
                .get_byte_offsets(),
            (1, 3)
        );
    }

    #[test]
    fn conflicting_source_map_edits_leave_other_edits_unchanged() {
        let mut document = document(&["abc"]);
        let text = document.children(document.root()).unwrap()[0];
        let mut batch = EditBatch::new();
        batch.replace_text(text, 0..1, "A");
        batch.set_source_map(text, Some(SourcePos::new(1, 2)));
        batch.set_source_map(text, None);

        assert_eq!(
            batch.commit(&mut document),
            Err(EditError::ConflictingSourceMapEdits { node: text })
        );
        assert_eq!(content(&document, text), "abc");
        assert!(document.node(text).unwrap().srcmap().is_none());
    }

    #[test]
    fn invalid_source_map_leaves_attributes_unchanged() {
        let mut document = document(&["雪"]);
        let text = document.children(document.root()).unwrap()[0];
        let mut batch = EditBatch::new();
        batch.set_attribute(text, "class", "new");
        batch.set_source_map(text, Some(SourcePos::new(1, 2)));

        assert_eq!(
            batch.commit(&mut document),
            Err(EditError::InvalidSourceMap {
                node: text,
                start: 1,
                end: 2,
            })
        );
        assert!(document.node(text).unwrap().attrs().is_empty());
        assert!(document.node(text).unwrap().srcmap().is_none());
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

        let mut source_map_conflict = EditBatch::new();
        source_map_conflict.remove_node(branch);
        source_map_conflict.set_source_map(text, None);
        assert_eq!(
            source_map_conflict.commit(&mut document),
            Err(EditError::EditTargetsRemovedNode {
                removed: branch,
                edited: text,
            })
        );
        assert!(document.node(text).unwrap().srcmap().is_none());
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

        let mut source_map_conflict = EditBatch::new();
        source_map_conflict.replace_node(branch, text_draft("replacement"));
        source_map_conflict.set_source_map(text, None);
        assert_eq!(
            source_map_conflict.commit(&mut document),
            Err(EditError::EditTargetsReplacedNode {
                replaced: branch,
                edited: text,
            })
        );

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

    #[test]
    fn wraps_an_inclusive_sibling_range_without_changing_node_ids() {
        let mut document = document(&["a", "b", "c", "d"]);
        let root = document.root();
        let original = document.children(root).unwrap().to_vec();
        let mut wrapper = NodeDraft::new(Paragraph);
        wrapper
            .attrs_mut()
            .push(("class".to_owned(), "wrapper".to_owned()));
        let mut batch = EditBatch::new();
        batch.wrap_range(original[1], original[2], wrapper);

        assert_eq!(batch.len(), 1);
        batch.commit(&mut document).unwrap();

        let children = document.children(root).unwrap();
        assert_eq!(children.len(), 3);
        assert_eq!(children[0], original[0]);
        assert_eq!(children[2], original[3]);
        let wrapper = children[1];
        let wrapper_node = document.node(wrapper).unwrap();
        assert!(wrapper_node.is::<Paragraph>());
        assert!(wrapper_node.srcmap().is_none());
        assert_eq!(
            wrapper_node.attrs(),
            &[("class".to_owned(), "wrapper".to_owned())]
        );
        assert_eq!(wrapper_node.children(), &original[1..=2]);
        assert_eq!(document.parent(original[1]).unwrap(), Some(wrapper));
        assert_eq!(document.parent(original[2]).unwrap(), Some(wrapper));
        assert_eq!(content(&document, original[1]), "b");
        assert_eq!(content(&document, original[2]), "c");

        let legacy = document.into_legacy();
        assert_eq!(legacy.children.len(), 3);
        assert_eq!(legacy.children[1].children.len(), 2);
    }

    #[test]
    fn wraps_multiple_disjoint_ranges_with_one_parent_rebuild() {
        let mut document = document(&["a", "b", "c", "d", "e"]);
        let root = document.root();
        let original = document.children(root).unwrap().to_vec();
        let mut batch = EditBatch::new();
        batch.wrap_range(original[3], original[4], NodeDraft::new(Paragraph));
        batch.wrap_range(original[0], original[1], NodeDraft::new(Paragraph));

        batch.commit(&mut document).unwrap();

        let children = document.children(root).unwrap();
        assert_eq!(children.len(), 3);
        assert_eq!(children[1], original[2]);
        assert_eq!(document.children(children[0]).unwrap(), &original[0..=1]);
        assert_eq!(document.children(children[2]).unwrap(), &original[3..=4]);
        for &node in &original {
            assert!(document.node(node).is_ok());
        }
    }

    #[test]
    fn wraps_ranges_under_different_parents_in_one_batch() {
        let mut document = branched_document();
        let branches = document.children(document.root()).unwrap().to_vec();
        let first_text = document.children(branches[0]).unwrap()[0];
        let second_text = document.children(branches[1]).unwrap()[0];
        let mut batch = EditBatch::new();
        batch.wrap_range(first_text, first_text, NodeDraft::new(Paragraph));
        batch.wrap_range(second_text, second_text, NodeDraft::new(Paragraph));

        batch.commit(&mut document).unwrap();

        for (branch, text) in branches.into_iter().zip([first_text, second_text]) {
            let wrapper = document.children(branch).unwrap()[0];
            assert_eq!(document.children(wrapper).unwrap(), &[text]);
            assert_eq!(document.parent(text).unwrap(), Some(wrapper));
        }
    }

    #[test]
    fn rejects_invalid_wrap_ranges_atomically() {
        let mut document = document(&["a", "b", "c", "d"]);
        let root = document.root();
        let children = document.children(root).unwrap().to_vec();

        let mut root_endpoint = EditBatch::new();
        root_endpoint.replace_text(children[0], 0..1, "A");
        root_endpoint.wrap_range(root, root, NodeDraft::new(Paragraph));
        assert_eq!(
            root_endpoint.commit(&mut document),
            Err(EditError::CannotWrapRoot(root))
        );
        assert_eq!(content(&document, children[0]), "a");

        let mut reversed = EditBatch::new();
        reversed.wrap_range(children[2], children[0], NodeDraft::new(Paragraph));
        assert_eq!(
            reversed.commit(&mut document),
            Err(EditError::ReversedWrapRange {
                first: children[2],
                last: children[0],
            })
        );

        let mut childful_wrapper = NodeDraft::new(Paragraph);
        childful_wrapper.push_child(text_draft("existing"));
        let mut childful = EditBatch::new();
        childful.wrap_range(children[0], children[1], childful_wrapper);
        assert_eq!(
            childful.commit(&mut document),
            Err(EditError::WrapperDraftHasChildren {
                first: children[0],
                last: children[1],
            })
        );

        let mut overlap = EditBatch::new();
        overlap.wrap_range(children[0], children[2], NodeDraft::new(Paragraph));
        overlap.wrap_range(children[2], children[3], NodeDraft::new(Paragraph));
        assert_eq!(
            overlap.commit(&mut document),
            Err(EditError::OverlappingWrapRanges {
                first_range: (children[0], children[2]),
                second_range: (children[2], children[3]),
            })
        );
        assert_eq!(document.children(root).unwrap(), children);
    }

    #[test]
    fn rejects_different_parent_and_stale_wrap_endpoints() {
        let mut document = branched_document();
        let branches = document.children(document.root()).unwrap().to_vec();
        let first = document.children(branches[0]).unwrap()[0];
        let second = document.children(branches[1]).unwrap()[0];
        let mut different_parents = EditBatch::new();
        different_parents.wrap_range(first, second, NodeDraft::new(Paragraph));
        assert_eq!(
            different_parents.commit(&mut document),
            Err(EditError::WrapEndpointsHaveDifferentParents {
                first,
                last: second,
            })
        );

        let mut removal = EditBatch::new();
        removal.remove_node(first);
        removal.commit(&mut document).unwrap();
        let mut stale = EditBatch::new();
        stale.wrap_range(first, first, NodeDraft::new(Paragraph));
        assert_eq!(
            stale.commit(&mut document),
            Err(EditError::InvalidNode(InvalidNodeId(first)))
        );
    }

    #[test]
    fn rejects_destructive_and_insertion_conflicts_with_wrap_ranges() {
        let mut document = branched_document();
        let branch = document.children(document.root()).unwrap()[0];
        let text = document.children(branch).unwrap()[0];
        let mut removal = EditBatch::new();
        removal.remove_node(branch);
        removal.wrap_range(text, text, NodeDraft::new(Paragraph));
        assert_eq!(
            removal.commit(&mut document),
            Err(EditError::WrapRangeTargetsRemovedNode {
                removed: branch,
                first: text,
                last: text,
            })
        );

        let mut replacement = EditBatch::new();
        replacement.replace_node(text, text_draft("replacement"));
        replacement.wrap_range(text, text, NodeDraft::new(Paragraph));
        assert_eq!(
            replacement.commit(&mut document),
            Err(EditError::WrapRangeTargetsReplacedNode {
                replaced: text,
                first: text,
                last: text,
            })
        );

        let mut insertion = EditBatch::new();
        insertion.insert_before(text, text_draft("inserted"));
        insertion.wrap_range(text, text, NodeDraft::new(Paragraph));
        assert_eq!(
            insertion.commit(&mut document),
            Err(EditError::InsertionTargetsWrappedNode {
                first: text,
                last: text,
                target: text,
            })
        );
        assert_eq!(document.len(), 5);
    }

    #[test]
    fn value_and_descendant_structure_edits_can_commit_with_wrap() {
        let mut root = Node::new(Root::new("ab".to_owned()));
        let mut paragraph = Node::new(Paragraph);
        paragraph.children.push(Node::new(Text {
            content: "a".to_owned(),
        }));
        paragraph.children.push(Node::new(Text {
            content: "b".to_owned(),
        }));
        root.children.push(paragraph);
        let mut document = Document::from_legacy("ab", root);
        let root = document.root();
        let paragraph = document.children(root).unwrap()[0];
        let texts = document.children(paragraph).unwrap().to_vec();
        let mut batch = EditBatch::new();
        batch.wrap_range(paragraph, paragraph, NodeDraft::new(Paragraph));
        batch.remove_node(texts[0]);
        batch.insert_before(texts[1], text_draft("inserted"));
        batch.replace_text(texts[1], 0..1, "B");
        batch.set_attribute(paragraph, "class", "wrapped-child");

        batch.commit(&mut document).unwrap();

        let wrapper = document.children(root).unwrap()[0];
        assert_eq!(document.children(wrapper).unwrap(), &[paragraph]);
        let paragraph_children = document.children(paragraph).unwrap();
        assert_eq!(paragraph_children.len(), 2);
        assert_eq!(content(&document, paragraph_children[0]), "inserted");
        assert_eq!(paragraph_children[1], texts[1]);
        assert_eq!(content(&document, texts[1]), "B");
        assert_eq!(
            document.node(paragraph).unwrap().attrs(),
            &[("class".to_owned(), "wrapped-child".to_owned())]
        );
    }

    #[test]
    fn wide_sibling_ranges_are_wrapped_iteratively() {
        let mut root = Node::new(Root::new(String::new()));
        for index in 0..10_000 {
            root.children.push(Node::new(Text {
                content: index.to_string(),
            }));
        }
        let mut document = Document::from_legacy("", root);
        let root = document.root();
        let original = document.children(root).unwrap().to_vec();
        let mut batch = EditBatch::new();
        batch.wrap_range(original[0], original[9_999], NodeDraft::new(Paragraph));

        batch.commit(&mut document).unwrap();

        assert_eq!(document.len(), 10_002);
        let wrapper = document.children(root).unwrap()[0];
        assert_eq!(document.children(wrapper).unwrap(), original);
        assert!(
            document
                .children(wrapper)
                .unwrap()
                .iter()
                .all(|&node| document.parent(node).unwrap() == Some(wrapper))
        );
    }
}
