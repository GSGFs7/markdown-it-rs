//! Plugin-supplied edits for arena-backed documents.

use std::ops::Range;

use crate::common::sourcemap::SourcePos;
use crate::document::data::NodeData;
use crate::document::{Document, NodeDraft, NodeId, NodeValue, SiblingPosition, Text};

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

#[derive(Debug)]
struct ReplaceValue {
    node: NodeId,
    value: NodeData,
}

#[derive(Debug, Default)]
struct NodePatchSet {
    values: Vec<ReplaceValue>,
    text: Vec<ReplaceText>,
    attributes: Vec<EditAttribute>,
    source_maps: Vec<EditSourceMap>,
}

impl NodePatchSet {
    fn len(&self) -> usize {
        self.values.len() + self.text.len() + self.attributes.len() + self.source_maps.len()
    }

    fn is_empty(&self) -> bool {
        self.values.is_empty()
            && self.text.is_empty()
            && self.attributes.is_empty()
            && self.source_maps.is_empty()
    }

    #[cfg(debug_assertions)]
    fn edited_nodes(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.text
            .iter()
            .map(|edit| edit.node)
            .chain(self.attributes.iter().map(|edit| edit.node))
            .chain(self.source_maps.iter().map(|edit| edit.node))
            .chain(self.values.iter().map(|edit| edit.node))
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

/// A batch of document edits supplied by a plugin.
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

    /// Replace a node's payload, preserving its ID, children and metadata.
    pub fn replace_value<T: NodeValue>(&mut self, node: NodeId, value: T) {
        self.node_patches.values.push(ReplaceValue {
            node,
            value: NodeData::new(value),
        });
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

    fn normalize(&mut self) {
        self.sort_text_edits();
        self.sort_attribute_edits();
        self.sort_source_map_edits();
        self.sort_removed_nodes();
        self.sort_node_replacements();
    }

    /// Apply a batch of edits supplied by a plugin.
    ///
    /// The caller must provide a valid batch. With debug assertions enabled,
    /// invalid batches panic before application. Otherwise, invalid batches
    /// have unspecified logical results and may panic after partial changes.
    /// No rollback or recovery guarantee is provided for invalid batches.
    #[track_caller]
    pub fn commit(mut self, document: &mut Document) {
        if self.is_empty() {
            return;
        }

        self.normalize();

        #[cfg(debug_assertions)]
        if let Err(error) = self.validate(document) {
            panic!("invalid edit batch: {error}");
        }

        self.apply_text(document);
        for edit in self.node_patches.values.drain(..) {
            document.node_mut(edit.node).data.replace_value(edit.value);
        }
        self.apply_removals(document);
        self.apply_replacements(document);
        self.apply_wrap_ranges(document);
        self.apply_insertions(document);
        self.apply_source_maps(document);
        self.apply_attributes(document);
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

    fn apply_text(&self, document: &mut Document) {
        for edits in text_groups(&self.node_patches.text) {
            let node_id = edits[0].node;
            let text = document
                .node_mut(node_id)
                .cast_mut::<Text>()
                .expect("text edit target must be a Text node");

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
            let attrs = document.node_mut(edit.node).attrs_mut();
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
            document.node_mut(edit.node).set_srcmap(edit.source_map);
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

#[cfg(debug_assertions)]
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

#[cfg(debug_assertions)]
mod validation;
