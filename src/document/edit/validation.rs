use std::collections::HashSet;
use std::ops::Range;

use super::{EditAttribute, EditBatch, ReplaceText, text_groups};
use crate::document::{Document, Node, NodeId, Text};

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
    #[cfg(feature = "sourcemap")]
    ConflictingSourceMapEdits {
        node: NodeId,
    },
    #[cfg(feature = "sourcemap")]
    InvalidSourceMap {
        node: NodeId,
        start: usize,
        end: usize,
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
            #[cfg(feature = "sourcemap")]
            Self::ConflictingSourceMapEdits { node } => {
                write!(f, "conflicting source map edits on node {node:?}")
            }
            #[cfg(feature = "sourcemap")]
            Self::InvalidSourceMap { node, start, end } => {
                write!(f, "invalid source map {start}..{end} for node {node:?}")
            }
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
        #[cfg(feature = "sourcemap")]
        self.validate_source_maps(document)?;
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

    #[cfg(feature = "sourcemap")]
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

fn checked_node(document: &Document, id: NodeId) -> Result<&Node, ValidationError> {
    document
        .get_node(id)
        .ok_or(ValidationError::InvalidNode(id))
}
