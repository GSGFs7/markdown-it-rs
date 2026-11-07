//! Document storage.

mod arena;
mod data;
pub mod edit;
mod events;
mod node;
mod root;
mod structure;
pub mod text;
mod value;

use std::sync::Arc;

use self::arena::Arena;
pub use self::arena::NodeId;
pub use self::events::StructuralEvent;
pub use self::node::Node;
pub use self::root::Root;
pub use self::text::{Text, TextSpecial};
pub use self::value::{HtmlAttribute, HtmlAttributes, NodeValue};

/// A parsed Markdown document and its source text.
///
#[derive(Debug)]
pub struct Document {
    source: Arc<str>,
    arena: Arena<Node>,
    root: NodeId,
}

impl Document {
    /// Create a document with its root allocated directly in the arena.
    ///
    /// The source is stored verbatim. The root may contain any [`NodeValue`];
    /// it starts with no children, attributes, extensions, or source mapping.
    /// Add nodes with [`Self::create_node`] and [`Self::push_child`].
    ///
    /// ```
    /// use markdown_it::document::{Document, Root, Text};
    ///
    /// let mut document = Document::new("hello", Root::new("hello"));
    /// let text = document.create_node(Text { content: "hello".into() });
    /// document.push_child(document.root(), text);
    /// assert_eq!(document.parent(text), Some(document.root()));
    /// ```
    pub fn new<T: NodeValue>(source: impl Into<Arc<str>>, value: T) -> Self {
        let mut arena = Arena::new();
        let root = arena.insert_with(|id| Node {
            id,
            parent: None,
            children: Vec::new(),
            data: self::data::NodeData::new(value),
        });
        Self {
            source: source.into(),
            arena,
            root,
        }
    }

    /// Allocate a detached node. Attach it with `push_child` before exposing the document.
    pub fn create_node<T: NodeValue>(&mut self, value: T) -> NodeId {
        self.arena.insert_with(|id| Node {
            id,
            parent: None,
            children: Vec::new(),
            data: data::NodeData::new(value),
        })
    }

    /// ID of the document root, which remains present for the document's life.
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Number of live nodes.
    pub fn len(&self) -> usize {
        self.arena.len()
    }

    /// Whether the `Document` holds no live nodes.
    pub fn is_empty(&self) -> bool {
        self.arena.len() == 0
    }

    /// Return a node if its slot and generation are still valid.
    /// The ID must originate from this document.
    pub fn get_node(&self, id: NodeId) -> Option<&Node> {
        self.arena.get(id)
    }

    /// Access a node whose ID must be valid in this document.
    ///
    /// # Panics
    /// Panics if the slot is unknown or the generation is stale.
    #[track_caller]
    pub fn node(&self, id: NodeId) -> &Node {
        match self.arena.get(id) {
            Some(node) => node,
            None => panic!("invalid or stale node ID: {id:?}"),
        }
    }

    /// Mutably access a node whose ID must be valid in this document.
    ///
    /// # Panics
    /// Panics if the slot is unknown or the generation is stale.
    #[track_caller]
    pub fn node_mut(&mut self, id: NodeId) -> &mut Node {
        match self.arena.get_mut(id) {
            Some(node) => node,
            None => {
                panic!("invalid or stale node ID: {id:?}")
            }
        }
    }

    /// Borrow a node's payload through its owning document.
    ///
    /// # Panics
    /// Panics if the ID is invalid or stale.
    #[inline]
    #[track_caller]
    pub fn value<T: NodeValue>(&self, id: NodeId) -> Option<&T> {
        self.node(id).cast::<T>()
    }

    /// Mutably borrow a node's payload without changing its type or identity.
    ///
    /// # Panics
    /// Panics if the ID is invalid or stale. It must originate from this document.
    #[inline]
    #[track_caller]
    pub fn value_mut<T: NodeValue>(&mut self, id: NodeId) -> Option<&mut T> {
        self.node_mut(id).cast_mut::<T>()
    }

    /// Return the parent of a valid node, or None for the document root.
    ///
    /// # Panics
    /// Panics if the slot is unknown or the generation is stale.
    #[track_caller]
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.node(id).parent()
    }

    /// Return the ordered children of a valid node.
    ///
    /// # Panics
    /// Panics if the slot is unknown or the generation is stale.
    #[track_caller]
    pub fn children(&self, id: NodeId) -> &[NodeId] {
        self.node(id).children()
    }

    /// Visit `root` and its descendants in preorder.
    ///
    /// This reuses [`Document::events`], so traversal memory grows with nesting
    /// depth rather than with the number of siblings. The callback receives
    /// node IDs and cannot mutate the document during the walk; collect IDs
    /// first, then apply structural edits after it returns.
    ///
    /// ```
    /// use markdown_it::MarkdownIt;
    ///
    /// let document = MarkdownIt::new().parse("hello");
    /// let mut visited = Vec::new();
    /// document.walk(document.root(), |id| visited.push(id));
    ///
    /// assert_eq!(visited[0], document.root());
    /// assert_eq!(visited.len(), document.len());
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if `root` is invalid or stale.
    #[track_caller]
    pub fn walk(&self, root: NodeId, mut visit: impl FnMut(NodeId)) {
        for event in self.events(root) {
            match event {
                StructuralEvent::Enter(node) | StructuralEvent::Leaf(node) => visit(node.id()),
                StructuralEvent::Exit(_) => {}
            }
        }
    }

    /// Attach a detached node to a parent.
    ///
    /// The caller must ensure attaching would not create a cycle. Ancestor
    /// cycles are checked only with debug assertions enabled.
    ///
    /// # Panics
    ///
    /// Panics if either ID is invalid, `child` is not detached or is the root,
    /// or `parent` equals `child`. Ancestor cycles panic before mutation only
    /// with debug assertions enabled.
    #[track_caller]
    pub fn push_child(&mut self, parent: NodeId, child: NodeId) {
        self.check_attachment(parent, child);

        self.node_mut(child).parent = Some(parent);
        self.node_mut(parent).children.push(child);
    }

    /// Markdown source owned by this document, used for byte source maps.
    ///
    /// [`crate::MarkdownIt::parse`] stores it with NUL replaced by
    /// U+FFFD; [`Document::new`] stores it verbatim.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Detach all children, preserving their order, IDs, metadata and subtrees.
    ///
    /// Reattach them with [`Self::push_child`] or [`Self::replace_children`],
    /// or delete them with [`Self::discard_node`].
    ///
    /// # Panics
    ///
    /// Panics if `parent` is invalid or stale.
    pub fn take_children(&mut self, parent: NodeId) -> Vec<NodeId> {
        let children = std::mem::take(&mut self.node_mut(parent).children);
        for &child in &children {
            self.node_mut(child).parent = None;
        }
        children
    }

    /// Attach an ordered sequence of detached children to an empty `parent`.
    pub(crate) fn attach_children(&mut self, parent: NodeId, children: Vec<NodeId>) {
        debug_assert!(
            self.children(parent).is_empty(),
            "parent must have no children"
        );

        for &child in &children {
            debug_assert_ne!(child, self.root, "cannot attach the document root");
            debug_assert_ne!(child, parent, "cannot attach a node to itself");
            debug_assert!(self.node(child).parent.is_none(), "child must be detached");
            self.node_mut(child).parent = Some(parent);
        }
        self.node_mut(parent).children = children;
    }

    /// Detach `parent`'s children, run `rewrite`, then reattach the result.
    ///
    /// Children removed by `rewrite` must be discarded by the caller before
    /// `rewrite` returns; the resulting sequence is reattached in order.
    pub(crate) fn rewrite_children<R>(
        &mut self,
        parent: NodeId,
        rewrite: impl FnOnce(&mut Document, &mut Vec<NodeId>) -> R,
    ) -> R {
        let mut children = self.take_children(parent);
        let result = rewrite(self, &mut children);
        self.attach_children(parent, children);
        result
    }

    /// Release unused arena slots at the end of the backing storage.
    pub(crate) fn trim_unused_tail(&mut self) {
        self.arena.trim_unused_tail();
    }
}

#[cfg(test)]
mod tests;
