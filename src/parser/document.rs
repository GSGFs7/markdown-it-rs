//! Experimental arena-backed document storage.

use std::any::TypeId;
use std::collections::HashSet;
use std::fmt::{self, Debug};
use std::iter::FusedIterator;
use std::sync::Arc;

use crate::common::TypeKey;
use crate::common::sourcemap::SourcePos;
use crate::parser::extset::NodeExtSet;
use crate::parser::node::{HtmlAttributes, Node, NodeParts, NodeValue};

/// Stable handle to a node stored in a [`Document`].
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId {
    slot: u32,
    generation: u32,
}

impl NodeId {
    pub fn slot(self) -> u32 {
        self.slot
    }

    pub fn generation(self) -> u32 {
        self.generation
    }
}

impl Debug for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NodeId({}:{})", self.slot, self.generation)
    }
}

/// Error returned when a node handle does not belong to the current arena
/// generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidNodeId(pub NodeId);

impl fmt::Display for InvalidNodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid or stale node ID: {:?}", self.0)
    }
}

impl std::error::Error for InvalidNodeId {}

/// Data for one arena-backed document node.
#[derive(Debug)]
pub struct DocumentNode {
    id: NodeId,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    srcmap: Option<SourcePos>,
    ext: NodeExtSet,
    attrs: HtmlAttributes,
    node_type: TypeKey,
    node_value: Box<dyn NodeValue>,
}

impl DocumentNode {
    pub fn id(&self) -> NodeId {
        self.id
    }

    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    pub fn children(&self) -> &[NodeId] {
        &self.children
    }

    pub fn srcmap(&self) -> Option<SourcePos> {
        self.srcmap
    }

    pub fn ext(&self) -> &NodeExtSet {
        &self.ext
    }

    pub fn attrs(&self) -> &HtmlAttributes {
        &self.attrs
    }

    pub(crate) fn attrs_mut(&mut self) -> &mut HtmlAttributes {
        &mut self.attrs
    }

    pub fn name(&self) -> &'static str {
        self.node_type.name
    }

    pub fn is<T: NodeValue>(&self) -> bool {
        self.node_type.id == TypeId::of::<T>()
    }

    pub fn cast<T: NodeValue>(&self) -> Option<&T> {
        if self.is::<T>() {
            self.node_value.downcast_ref::<T>()
        } else {
            None
        }
    }

    pub(crate) fn cast_mut<T: NodeValue>(&mut self) -> Option<&mut T> {
        if self.is::<T>() {
            self.node_value.downcast_mut::<T>()
        } else {
            None
        }
    }
}

/// Borrowed view of a node produced by a structural traversal.
pub type NodeRef<'a> = &'a DocumentNode;

/// One step in a depth-first structural traversal.
///
/// Nodes with children produce a balanced `Enter` / `Exit` pair. Nodes
/// without children produce exactly one `Leaf` event.
#[derive(Clone, Copy, Debug)]
pub enum StructuralEvent<'a> {
    Enter(NodeRef<'a>),
    Leaf(NodeRef<'a>),
    Exit(NodeRef<'a>),
}

impl<'a> StructuralEvent<'a> {
    pub fn node(&self) -> NodeRef<'a> {
        match self {
            Self::Enter(node) | Self::Leaf(node) | Self::Exit(node) => node,
        }
    }
}

#[derive(Clone, Debug)]
struct EventFrame<'a> {
    node: NodeRef<'a>,
    children: std::slice::Iter<'a, NodeId>,
}

/// Lazy depth-first iterator over a document subtree.
///
/// The iterator allocates only a stack proportional to the current nesting
/// depth. The document's parent/children links remain the sole structural
/// source of truth.
#[derive(Debug)]
pub struct StructuralEvents<'a> {
    document: &'a Document,
    stack: Vec<EventFrame<'a>>,
    pending: Option<StructuralEvent<'a>>,
}

impl<'a> Iterator for StructuralEvents<'a> {
    type Item = StructuralEvent<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(event) = self.pending.take() {
            return Some(event);
        }

        let frame = self.stack.last_mut()?;
        let node = frame.node;

        if let Some(&child) = frame.children.next() {
            let child = self
                .document
                .arena
                .get(child)
                .expect("document tree is internally valid");
            if child.children.is_empty() {
                return Some(StructuralEvent::Leaf(child));
            }
            self.stack.push(EventFrame {
                node: child,
                children: child.children.iter(),
            });
            return Some(StructuralEvent::Enter(child));
        }

        self.stack.pop();
        Some(StructuralEvent::Exit(node))
    }
}

impl FusedIterator for StructuralEvents<'_> {}

#[derive(Debug)]
struct Slot<T> {
    /// distinguish between different nodes successively in the same slot
    generation: u32,
    /// one-way linked list
    next_free: Option<u32>,
    /// actual stored data
    value: Option<T>,
}

#[derive(Debug)]
struct Arena<T> {
    /// storage
    slots: Vec<Slot<T>>,
    /// first free slot
    free_head: Option<u32>,
    /// number of living objects
    len: usize,
}

impl<T> Arena<T> {
    fn new() -> Self {
        Self {
            slots: Vec::new(),
            free_head: None,
            len: 0,
        }
    }

    fn insert_with(&mut self, make_value: impl FnOnce(NodeId) -> T) -> NodeId {
        let id = if let Some(slot_index) = self.free_head {
            let slot = &mut self.slots[slot_index as usize];
            self.free_head = slot.next_free.take();
            NodeId {
                slot: slot_index,
                generation: slot.generation,
            }
        } else {
            let slot = u32::try_from(self.slots.len()).expect("document contains too many nodes");
            // if there a no free slots, create a new
            self.slots.push(Slot {
                generation: 0,
                next_free: None,
                value: None,
            });
            NodeId {
                slot,
                generation: 0,
            }
        };

        self.slots[id.slot as usize].value = Some(make_value(id));
        self.len += 1;
        id
    }

    fn get(&self, id: NodeId) -> Option<&T> {
        let slot = self.slots.get(id.slot as usize)?;
        if slot.generation == id.generation {
            slot.value.as_ref()
        } else {
            None
        }
    }

    fn get_mut(&mut self, id: NodeId) -> Option<&mut T> {
        let slot = self.slots.get_mut(id.slot as usize)?;
        if slot.generation == id.generation {
            slot.value.as_mut()
        } else {
            None
        }
    }

    fn remove(&mut self, id: NodeId) -> Option<T> {
        let slot = self.slots.get_mut(id.slot as usize)?;
        if slot.generation != id.generation {
            // generation must be match
            return None;
        }

        let value = slot.value.take()?;
        self.len -= 1;

        // increase generation. retire it if generation got the u32::MAX.
        if let Some(next_generation) = slot.generation.checked_add(1) {
            // insert the empty slot to the head of linked list.
            slot.generation = next_generation;
            slot.next_free = self.free_head;
            self.free_head = Some(id.slot);
        }

        Some(value)
    }
}

/// Arena-backed representation of one parsed Markdown document.
///
/// This is currently an opt-in migration API. Existing [`crate::MarkdownIt::parse`]
/// callers continue to receive a legacy [`Node`] tree.
#[derive(Debug)]
pub struct Document {
    source: Arc<str>,
    arena: Arena<DocumentNode>,
    root: NodeId,
}

impl Document {
    /// Move an existing AST into arena-backed storage.
    pub fn from_legacy(source: impl Into<Arc<str>>, root: Node) -> Self {
        let mut document = Self {
            source: source.into(),
            arena: Arena::new(),
            // Replaced before this constructor returns.
            root: NodeId {
                slot: u32::MAX,
                generation: u32::MAX,
            },
        };
        document.root = document.insert_legacy(None, root);
        document
    }

    fn insert_legacy(&mut self, parent: Option<NodeId>, mut legacy: Node) -> NodeId {
        let parts = legacy.take_parts();

        let id = self.arena.insert_with(|id| DocumentNode {
            id,
            parent,
            children: Vec::with_capacity(parts.children.len()),
            srcmap: parts.srcmap,
            ext: parts.ext,
            attrs: parts.attrs,
            node_type: parts.node_type,
            node_value: parts.node_value,
        });

        for child in parts.children {
            let child_id = stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
                self.insert_legacy(Some(id), child)
            });
            self.arena.get_mut(id).unwrap().children.push(child_id);
        }
        id
    }

    /// Original Markdown source owned by this document.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// ID of the document root, which remains present for the document's life.
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Number of live nodes.
    pub fn len(&self) -> usize {
        self.arena.len
    }

    pub fn is_empty(&self) -> bool {
        self.arena.len == 0
    }

    /// Look up a node, rejecting IDs with an unknown slot or stale generation.
    pub fn node(&self, id: NodeId) -> Result<&DocumentNode, InvalidNodeId> {
        self.arena.get(id).ok_or(InvalidNodeId(id))
    }

    pub(crate) fn node_mut(&mut self, id: NodeId) -> Result<&mut DocumentNode, InvalidNodeId> {
        self.arena.get_mut(id).ok_or(InvalidNodeId(id))
    }

    pub(crate) fn remove_subtrees(&mut self, roots: &[NodeId]) {
        if let [root] = roots {
            // single root optimization
            let parent = self
                .arena
                .get(*root)
                .expect("validated subtree root remains present")
                .parent
                .expect("the document root cannot be removed");
            let siblings = &mut self
                .arena
                .get_mut(parent)
                .expect("validated subtree parent remains present")
                .children;
            let position = siblings
                .iter()
                .position(|child| child == root)
                .expect("document parent links are internally consistent");
            siblings.remove(position);
        } else {
            let root_set: HashSet<_> = roots.iter().copied().collect();
            let parents: HashSet<_> = root_set
                .iter()
                .map(|&root| {
                    self.arena
                        .get(root)
                        .expect("validated subtree root remains present")
                        .parent
                        .expect("the document root cannot be removed")
                })
                .collect();
            for parent in parents {
                self.arena
                    .get_mut(parent)
                    .expect("validated subtree parent remains present")
                    .children
                    .retain(|child| !root_set.contains(child));
            }
        }

        // reverse delete (post-order traversal)
        // child nodes are always deleted before their parent nodes.
        // 
        // e.g.
        // root->(A->(A1,A2->(A21,A22)),B)
        // turn     action      pending      nodes
        // 0        pop B       [A]          [B]
        // 1        pop A       [A1,A2]      [B,A]
        // 2        pop A2      [A1,A21,A22] [B,A,A2]
        // 3        pop A22     [A1,A21]     [B,A,A2,A22]
        // 4        pop A21     [A1]         [B,A,A2,A22,A21]
        // 5        pop A1      []           [B,A,A2,A22,A21,A1]
        // deletion order: A1->A21->A22->A2->A->B
        let mut pending = roots.to_vec();
        let mut nodes = Vec::new();
        while let Some(node) = pending.pop() {
            let node = self
                .arena
                .get(node)
                .expect("document child links are internally valid");
            pending.extend(node.children.iter().copied()); // push children
            nodes.push(node.id); // push parent
        }
        for node in nodes.into_iter().rev() {
            self.arena
                .remove(node)
                .expect("collected subtree node remains present");
        }
    }

    /// Look up a node's parent.
    pub fn parent(&self, id: NodeId) -> Result<Option<NodeId>, InvalidNodeId> {
        Ok(self.node(id)?.parent())
    }

    /// Look up a node's ordered children.
    pub fn children(&self, id: NodeId) -> Result<&[NodeId], InvalidNodeId> {
        Ok(self.node(id)?.children())
    }

    /// Lazily traverse a node and its descendants in depth-first order.
    pub fn events(&self, root: NodeId) -> Result<StructuralEvents<'_>, InvalidNodeId> {
        let root = self.node(root)?;
        let mut stack = Vec::with_capacity(16);
        let pending = if root.children.is_empty() {
            Some(StructuralEvent::Leaf(root))
        } else {
            stack.push(EventFrame {
                node: root,
                children: root.children.iter(),
            });
            Some(StructuralEvent::Enter(root))
        };
        Ok(StructuralEvents {
            document: self,
            stack,
            pending,
        })
    }

    /// Rebuild the legacy tree, consuming this document.
    pub fn into_legacy(mut self) -> Node {
        self.take_legacy(self.root)
    }

    fn take_legacy(&mut self, id: NodeId) -> Node {
        let mut data = self
            .arena
            .remove(id)
            .expect("document tree is internally valid");
        let child_ids = std::mem::take(&mut data.children);
        let mut children = Vec::with_capacity(child_ids.len());
        for child_id in child_ids {
            children.push(stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
                self.take_legacy(child_id)
            }));
        }

        Node::from_parts(NodeParts {
            children,
            srcmap: data.srcmap,
            ext: data.ext,
            attrs: data.attrs,
            node_type: data.node_type,
            node_value: data.node_value,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use super::{Arena, Document, DocumentNode, StructuralEvent};
    use crate::parser::core::Root;
    use crate::parser::inline::Text;
    use crate::plugins::cmark::block::paragraph::Paragraph;
    use crate::{
        EditBatch,
        EditError,
        MarkdownIt,
        Node,
        TextProjection,
        TextProjectionKind,
        plugins,
    };

    fn transparent_text_projection(_: super::NodeRef<'_>) -> TextProjectionKind<'_> {
        TextProjectionKind::Transparent
    }

    #[test]
    fn stale_id_cannot_access_reused_slot() {
        let mut arena = Arena::new();
        let old = arena.insert_with(|_| "old");
        assert_eq!(arena.remove(old), Some("old"));

        let new = arena.insert_with(|_| "new");
        assert_eq!(old.slot(), new.slot());
        assert_ne!(old.generation(), new.generation());
        assert_eq!(arena.get(old), None);
        assert_eq!(arena.get(new), Some(&"new"));
    }

    #[test]
    fn maximum_generation_slot_is_retired_instead_of_wrapping() {
        let old = super::NodeId {
            slot: 0,
            generation: u32::MAX,
        };
        let mut arena = Arena {
            slots: vec![super::Slot {
                generation: u32::MAX,
                next_free: None,
                value: Some("old"),
            }],
            free_head: None,
            len: 1,
        };

        assert_eq!(arena.remove(old), Some("old"));
        let new = arena.insert_with(|_| "new");

        assert_eq!(new.slot(), 1);
        assert_eq!(arena.get(old), None);
    }

    #[test]
    fn legacy_roundtrip_preserves_tree_and_payloads() {
        let mut root = Node::new(Root::new("hello".to_owned()));
        let mut paragraph = Node::new(Paragraph);
        paragraph.children.push(Node::new(Text {
            content: "hello".to_owned(),
        }));
        root.children.push(paragraph);

        let document = Document::from_legacy("hello", root);
        let root_id = document.root();
        let paragraph_id = document.children(root_id).unwrap()[0];
        let text_id = document.children(paragraph_id).unwrap()[0];

        assert_eq!(document.source(), "hello");
        assert_eq!(document.len(), 3);
        assert_eq!(document.parent(root_id).unwrap(), None);
        assert_eq!(document.parent(text_id).unwrap(), Some(paragraph_id));
        assert_eq!(
            document
                .node(text_id)
                .unwrap()
                .cast::<Text>()
                .unwrap()
                .content,
            "hello"
        );

        let legacy = document.into_legacy();
        assert!(legacy.is::<Root>());
        assert!(legacy.children[0].is::<Paragraph>());
        assert_eq!(
            legacy.children[0].children[0]
                .cast::<Text>()
                .unwrap()
                .content,
            "hello"
        );
    }

    #[test]
    fn structural_events_are_ordered_and_balanced() {
        let mut root = Node::new(Root::new("hello".to_owned()));
        let mut paragraph = Node::new(Paragraph);
        paragraph.children.push(Node::new(Text {
            content: "first".to_owned(),
        }));
        paragraph.children.push(Node::new(Text {
            content: "second".to_owned(),
        }));
        root.children.push(paragraph);

        let document = Document::from_legacy("hello", root);
        let root = document.root();
        let paragraph = document.children(root).unwrap()[0];
        let children = document.children(paragraph).unwrap();
        let events = document.events(document.root()).unwrap();
        let actual: Vec<_> = events
            .map(|event| match event {
                StructuralEvent::Enter(node) => ("enter", node.id()),
                StructuralEvent::Leaf(node) => ("leaf", node.id()),
                StructuralEvent::Exit(node) => ("exit", node.id()),
            })
            .collect();

        assert_eq!(
            actual,
            [
                ("enter", root),
                ("enter", paragraph),
                ("leaf", children[0]),
                ("leaf", children[1]),
                ("exit", paragraph),
                ("exit", root),
            ]
        );
    }

    #[test]
    fn structural_events_can_start_at_a_subtree_or_leaf() {
        let mut root = Node::new(Root::new("hello".to_owned()));
        let mut paragraph = Node::new(Paragraph);
        paragraph.children.push(Node::new(Text {
            content: "hello".to_owned(),
        }));
        root.children.push(paragraph);

        let document = Document::from_legacy("hello", root);
        let paragraph = document.children(document.root()).unwrap()[0];
        let text = document.children(paragraph).unwrap()[0];

        assert!(matches!(
            document.events(paragraph).unwrap().next(),
            Some(StructuralEvent::Enter(node)) if node.id() == paragraph
        ));
        assert!(matches!(
            document.events(text).unwrap().collect::<Vec<_>>().as_slice(),
            [StructuralEvent::Leaf(node)] if node.id() == text
        ));
    }

    #[test]
    fn structural_events_reject_a_stale_root() {
        let mut root = Node::new(Root::new("text".to_owned()));
        root.children.push(Node::new(Text {
            content: "text".to_owned(),
        }));
        let mut document = Document::from_legacy("", root);
        let root = document.root();
        let text = document.children(root).unwrap()[0];
        assert!(document.arena.remove(root).is_some());

        assert_eq!(
            document.events(root).unwrap_err(),
            super::InvalidNodeId(root)
        );
        assert!(matches!(
            document.text_events_from(root, TextProjection::new(transparent_text_projection)),
            Err(error) if error == super::InvalidNodeId(root)
        ));
        let mut batch = EditBatch::new();
        batch.replace_text(root, 0..0, "stale");
        assert_eq!(
            batch.commit(&mut document),
            Err(EditError::InvalidNode(super::InvalidNodeId(root)))
        );

        let mut batch = EditBatch::new();
        batch.replace_text(text, 0..1, "T");
        batch.set_attribute(root, "class", "stale");
        assert_eq!(
            batch.commit(&mut document),
            Err(EditError::InvalidNode(super::InvalidNodeId(root)))
        );
        assert_eq!(
            document.node(text).unwrap().cast::<Text>().unwrap().content,
            "text"
        );
    }

    #[test]
    fn layout_sizes_are_visible_to_the_arena_design() {
        // Keep this measurement close to the storage definition so future
        // layout changes cannot happen without an explicit review point.
        eprintln!(
            "Node={} DocumentNode={} Slot<DocumentNode>={}",
            size_of::<Node>(),
            size_of::<DocumentNode>(),
            size_of::<super::Slot<DocumentNode>>()
        );
        assert!(size_of::<super::Slot<DocumentNode>>() <= 256);
    }

    #[test]
    fn parser_facade_preserves_rendering_and_source() {
        let mut md = MarkdownIt::empty();
        plugins::cmark::add(&mut md);
        plugins::html::add(&mut md);
        let source = "# 雪\n\nA *small* document.\n";

        let expected = md.parse(source).render();
        let document = md.parse_document(source);

        assert_eq!(document.source(), source);
        assert_eq!(document.into_legacy().render(), expected);
    }

    #[test]
    fn parsed_document_events_visit_every_node_once() {
        use std::collections::HashSet;

        let mut md = MarkdownIt::empty();
        plugins::cmark::add(&mut md);
        plugins::html::add(&mut md);
        let document = md.parse_document("# Heading\n\nA *small* [link](url).\n\n---\n");
        let mut stack = Vec::new();
        let mut visited = HashSet::new();

        for event in document.events(document.root()).unwrap() {
            let node = event.node();
            match event {
                StructuralEvent::Enter(_) => {
                    assert_eq!(node.parent(), stack.last().copied());
                    assert!(visited.insert(node.id()));
                    stack.push(node.id());
                }
                StructuralEvent::Leaf(_) => {
                    assert_eq!(node.parent(), stack.last().copied());
                    assert!(visited.insert(node.id()));
                }
                StructuralEvent::Exit(_) => {
                    assert_eq!(stack.pop(), Some(node.id()));
                }
            }
        }

        assert!(stack.is_empty());
        assert_eq!(visited.len(), document.len());
    }
}
