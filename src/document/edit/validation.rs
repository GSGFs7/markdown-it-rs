use std::collections::HashSet;
use std::ops::Range;

use super::{EditBatch, ReplaceText, attribute_groups, text_groups};
use crate::DocumentNode;
use crate::document::{Document, NodeId};
use crate::parser::inline::Text;

/// A validation failure that leaves the document unchanged.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ValidationError {
    InvalidNode(NodeId),
    DuplicateValueReplacement(NodeId),
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

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateValueReplacement(node) => {
                write!(f, "node {node:?} has multiple payload replacements")
            }
            Self::InvalidNode(node) => write!(f, "invalid or stale node ID: {node:?}"),
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

impl EditBatch {
    pub(super) fn validate(&self, document: &Document) -> Result<(), ValidationError> {
        let mut values = HashSet::new();
        for edit in &self.node_patches.values {
            checked_node(document, edit.node)?;
            if !values.insert(edit.node) {
                return Err(ValidationError::DuplicateValueReplacement(edit.node));
            }
        }
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
        Ok(())
    }

    fn validate_text(&self, document: &Document) -> Result<(), ValidationError> {
        for edits in text_groups(&self.node_patches.text) {
            let node_id = edits[0].node;
            let node = checked_node(document, node_id)?;
            let Some(text) = node.cast::<Text>() else {
                return Err(ValidationError::NotEditableText(node_id));
            };

            let mut previous: Option<&ReplaceText> = None;
            let mut final_len = text.content.len();
            for edit in edits {
                if edit.range.start > edit.range.end
                    || edit.range.end > text.content.len()
                    || !text.content.is_char_boundary(edit.range.start)
                    || !text.content.is_char_boundary(edit.range.end)
                {
                    return Err(ValidationError::InvalidTextRange {
                        node: node_id,
                        range: edit.range.clone(),
                    });
                }
                if let Some(previous) = previous
                    && edit.range.start < previous.range.end
                {
                    return Err(ValidationError::OverlappingTextEdits {
                        node: node_id,
                        first: previous.range.clone(),
                        second: edit.range.clone(),
                    });
                }
                final_len = final_len
                    .checked_sub(edit.range.end - edit.range.start)
                    .and_then(|len| len.checked_add(edit.replacement.len()))
                    .ok_or(ValidationError::TextLengthOverflow(node_id))?;
                previous = Some(edit);
            }
        }
        Ok(())
    }

    fn validate_attributes(&self, document: &Document) -> Result<(), ValidationError> {
        for edits in attribute_groups(&self.node_patches.attributes) {
            let edit = &edits[0];
            checked_node(document, edit.node)?;
            if edits.len() > 1 {
                return Err(ValidationError::ConflictingAttributeEdits {
                    node: edit.node,
                    name: edit.name.clone(),
                });
            }
        }
        Ok(())
    }

    fn validate_source_maps(&self, document: &Document) -> Result<(), ValidationError> {
        let mut previous = None;
        for edit in &self.node_patches.source_maps {
            checked_node(document, edit.node)?;
            if previous == Some(edit.node) {
                return Err(ValidationError::ConflictingSourceMapEdits { node: edit.node });
            }
            if let Some(source_map) = edit.source_map {
                let (start, end) = source_map.get_byte_offsets();
                if start > end
                    || end > document.source().len()
                    || !document.source().is_char_boundary(start)
                    || !document.source().is_char_boundary(end)
                {
                    return Err(ValidationError::InvalidSourceMap {
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

    fn validate_removals(&self, document: &Document) -> Result<(), ValidationError> {
        let mut removals = HashSet::with_capacity(self.structural_edits.removals.len());
        for &node in &self.structural_edits.removals {
            checked_node(document, node)?;
            if node == document.root() {
                return Err(ValidationError::CannotRemoveRoot(node));
            }
            if !removals.insert(node) {
                return Err(ValidationError::DuplicateNodeRemoval(node));
            }
        }

        for &descendant in &self.structural_edits.removals {
            let mut ancestor = checked_node(document, descendant)?.parent();
            while let Some(node) = ancestor {
                if removals.contains(&node) {
                    return Err(ValidationError::OverlappingNodeRemovals {
                        ancestor: node,
                        descendant,
                    });
                }
                ancestor = checked_node(document, node)?.parent();
            }
        }

        for edited in self.node_patches.edited_nodes() {
            let mut current = Some(edited);
            while let Some(node) = current {
                if removals.contains(&node) {
                    return Err(ValidationError::EditTargetsRemovedNode {
                        removed: node,
                        edited,
                    });
                }
                current = checked_node(document, node)?.parent();
            }
        }
        Ok(())
    }

    fn validate_insertions(&self, document: &Document) -> Result<(), ValidationError> {
        let removals: HashSet<_> = self.structural_edits.removals.iter().copied().collect();
        for insertion in &self.structural_edits.insertions {
            let target = checked_node(document, insertion.target)?;
            if target.parent().is_none() {
                return Err(ValidationError::CannotInsertSiblingOfRoot(insertion.target));
            }

            let mut current = Some(insertion.target);
            while let Some(node) = current {
                if removals.contains(&node) {
                    return Err(ValidationError::InsertionTargetsRemovedNode {
                        removed: node,
                        target: insertion.target,
                    });
                }
                current = checked_node(document, node)?.parent();
            }
        }
        Ok(())
    }

    fn validate_replacements(&self, document: &Document) -> Result<(), ValidationError> {
        let mut replacements = HashSet::with_capacity(self.structural_edits.replacements.len());
        for replacement in &self.structural_edits.replacements {
            checked_node(document, replacement.target)?; // check InvalidNode
            if replacement.target == document.root() {
                return Err(ValidationError::CannotReplaceRoot(replacement.target));
            }
            if !replacements.insert(replacement.target) {
                return Err(ValidationError::DuplicateNodeReplacement(
                    replacement.target,
                ));
            }
        }

        // not allow ancestor/descendant overlap
        for replacement in &self.structural_edits.replacements {
            let mut ancestor = checked_node(document, replacement.target)?.parent();
            while let Some(node) = ancestor {
                if replacements.contains(&node) {
                    return Err(ValidationError::OverlappingNodeReplacements {
                        ancestor: node,
                        descendant: replacement.target,
                    });
                }
                ancestor = checked_node(document, node)?.parent();
            }
        }

        // bidirectional detection with delete
        let removals: HashSet<_> = self.structural_edits.removals.iter().copied().collect();
        for replacement in &self.structural_edits.replacements {
            let mut current = Some(replacement.target);
            while let Some(node) = current {
                if removals.contains(&node) {
                    return Err(ValidationError::ConflictingNodeRemovalAndReplacement {
                        removed: node,
                        replaced: replacement.target,
                    });
                }
                current = checked_node(document, node)?.parent();
            }
        }
        for &removed in &self.structural_edits.removals {
            let mut current = Some(removed);
            while let Some(node) = current {
                if replacements.contains(&node) {
                    return Err(ValidationError::ConflictingNodeRemovalAndReplacement {
                        removed,
                        replaced: node,
                    });
                }
                current = checked_node(document, node)?.parent();
            }
        }

        for edited in self.node_patches.edited_nodes() {
            let mut current = Some(edited);
            while let Some(node) = current {
                if replacements.contains(&node) {
                    return Err(ValidationError::EditTargetsReplacedNode {
                        replaced: node,
                        edited,
                    });
                }
                current = checked_node(document, node)?.parent();
            }
        }

        for insertion in &self.structural_edits.insertions {
            let mut current = Some(insertion.target);
            while let Some(node) = current {
                if replacements.contains(&node) {
                    return Err(ValidationError::InsertionTargetsReplacedNode {
                        replaced: node,
                        target: insertion.target,
                    });
                }
                current = checked_node(document, node)?.parent();
            }
        }
        Ok(())
    }

    fn validate_wrap_ranges(&self, document: &Document) -> Result<(), ValidationError> {
        let mut resolved = Vec::with_capacity(self.structural_edits.wraps.len());
        for range in &self.structural_edits.wraps {
            let first = checked_node(document, range.first)?;
            let last = checked_node(document, range.last)?;
            let Some(first_parent) = first.parent() else {
                return Err(ValidationError::CannotWrapRoot(range.first));
            };
            let Some(last_parent) = last.parent() else {
                return Err(ValidationError::CannotWrapRoot(range.last));
            };
            if first_parent != last_parent {
                return Err(ValidationError::WrapEndpointsHaveDifferentParents {
                    first: range.first,
                    last: range.last,
                });
            }
            if !range.wrapper.children().is_empty() {
                return Err(ValidationError::WrapperDraftHasChildren {
                    first: range.first,
                    last: range.last,
                });
            }

            let siblings = checked_node(document, first_parent)?.children();
            let first_index = siblings
                .iter()
                .position(|&node| node == range.first)
                .expect("document parent links are internally consistent");
            let last_index = siblings
                .iter()
                .position(|&node| node == range.last)
                .expect("document parent links are internally consistent");
            if first_index > last_index {
                return Err(ValidationError::ReversedWrapRange {
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
                return Err(ValidationError::OverlappingWrapRanges {
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
            let siblings = checked_node(document, parent)?.children();
            for &selected in &siblings[first_index..=last_index] {
                let mut current = Some(selected);
                while let Some(node) = current {
                    if removals.contains(&node) {
                        return Err(ValidationError::WrapRangeTargetsRemovedNode {
                            removed: node,
                            first,
                            last,
                        });
                    }
                    if replacements.contains(&node) {
                        return Err(ValidationError::WrapRangeTargetsReplacedNode {
                            replaced: node,
                            first,
                            last,
                        });
                    }
                    current = checked_node(document, node)?.parent();
                }
            }
        }

        for insertion in &self.structural_edits.insertions {
            for &(parent, first_index, last_index, first, last) in &resolved {
                let siblings = checked_node(document, parent)?.children();
                if siblings[first_index..=last_index].contains(&insertion.target) {
                    return Err(ValidationError::InsertionTargetsWrappedNode {
                        first,
                        last,
                        target: insertion.target,
                    });
                }
            }
        }
        Ok(())
    }
}

// helper
fn checked_node(document: &Document, id: NodeId) -> Result<&DocumentNode, ValidationError> {
    document
        .get_node(id)
        .ok_or(ValidationError::InvalidNode(id))
}
