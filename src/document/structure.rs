use std::collections::HashSet;
use std::ops::Range;

use super::{Document, NodeId};

impl Document {
    /// Delete a detached subtree, invalidating its IDs.
    ///
    /// # Panics
    ///
    /// Panics if `node` is invalid, attached, or the root.
    #[track_caller]
    pub fn discard_node(&mut self, node: NodeId) {
        assert_ne!(node, self.root, "cannot discard the document root");
        assert!(self.node(node).parent.is_none(), "node must be detached");

        self.delete_subtree(node);
    }

    /// Remove an attached non-root node and its subtree.
    ///
    /// All IDs in the subtree become stale.
    ///
    /// # Panics
    ///
    /// Panics before mutation if `node` is invalid, detached, or the root.
    #[track_caller]
    pub fn remove_node(&mut self, node: NodeId) {
        let (parent, index) = self.sibling_location(node);
        self.node_mut(parent).children.remove(index);
        self.delete_subtree(node);
    }

    /// Insert a detached node before `target`.
    ///
    /// The caller must ensure attaching would not create an ancestor cycle;
    /// this is checked only with debug assertions enabled.
    ///
    /// Repeated calls at the same target preserve order.
    ///
    /// ```
    /// use markdown_it::{MarkdownIt, document::Text};
    ///
    /// let mut document = MarkdownIt::new().parse("hello");
    /// let target = document.children(document.root())[0];
    /// let node = document.create_node(Text { content: "prefix".into() });
    /// document.insert_before(target, node);
    /// assert_eq!(document.children(document.root()), &[node, target]);
    /// ```
    ///
    /// # Panics
    ///
    /// Panics before mutation if either ID is invalid, the target is detached or
    /// the root, the node is attached or the root, or it would be attached to
    /// itself. Ancestor cycles panic before mutation only with debug assertions
    /// enabled.
    #[track_caller]
    pub fn insert_before(&mut self, target: NodeId, node: NodeId) {
        let (parent, index) = self.sibling_location(target);
        self.check_attachment(parent, node);
        self.node_mut(node).parent = Some(parent);
        self.node_mut(parent).children.insert(index, node);
    }

    /// Insert a detached node after `target`.
    ///
    /// The caller must ensure attaching would not create an ancestor cycle;
    /// this is checked only with debug assertions enabled.
    ///
    /// Repeated calls at the same target place the latest node nearest the
    /// target; pass the inserted node as the next target to preserve order.
    ///
    /// # Panics
    ///
    /// Panics under the same conditions as [`Document::insert_before`].
    #[track_caller]
    pub fn insert_after(&mut self, target: NodeId, node: NodeId) {
        let (parent, index) = self.sibling_location(target);
        self.check_attachment(parent, node);
        self.node_mut(node).parent = Some(parent);
        self.node_mut(parent).children.insert(index + 1, node);
    }

    /// Replace an attached non-root node with a detached subtree.
    ///
    /// The caller must ensure attaching would not create an ancestor cycle;
    /// this is checked only with debug assertions enabled.
    ///
    /// The replacement keeps its IDs; all IDs in the old subtree become stale.
    ///
    /// # Panics
    ///
    /// Panics under the same conditions as [`Document::insert_before`].
    #[track_caller]
    pub fn replace_node(&mut self, target: NodeId, replacement: NodeId) {
        let (parent, index) = self.sibling_location(target);
        self.check_attachment(parent, replacement);
        self.node_mut(replacement).parent = Some(parent);
        self.node_mut(parent).children[index] = replacement;
        self.delete_subtree(target);
    }

    /// Wrap a contiguous sibling range in a detached `wrapper` node.
    ///
    /// Children in `range` keep their relative order and become the wrapper's
    /// children. The wrapper takes their position in `parent`; any children the
    /// wrapper already had remain before the wrapped range.
    ///
    /// ```
    /// use markdown_it::{MarkdownIt, document::Text};
    ///
    /// let mut document = MarkdownIt::new().parse("first *second* third");
    /// let paragraph = document.children(document.root())[0];
    /// let first = document.children(paragraph)[0];
    /// let middle = document.children(paragraph)[1];
    /// let last = document.children(paragraph)[2];
    ///
    /// let wrapper = document.create_node(Text { content: String::new() });
    /// document.wrap_children(paragraph, 1..3, wrapper);
    ///
    /// assert_eq!(document.children(paragraph), &[first, wrapper]);
    /// assert_eq!(document.children(wrapper), &[middle, last]);
    /// ```
    ///
    /// # Panics
    ///
    /// Panics before mutation on invalid IDs, a reversed or out-of-bounds
    /// `range`, or an attached, root or cyclic `wrapper`.
    #[track_caller]
    pub fn wrap_children(&mut self, parent: NodeId, range: Range<usize>, wrapper: NodeId) {
        let len = self.children(parent).len();
        assert!(
            range.start <= range.end,
            "child range start must not exceed its end"
        );
        assert!(
            range.end <= len,
            "child range end {} exceeds {len} children",
            range.end
        );
        self.check_attachment(parent, wrapper);

        let mut children = self.take_children(parent);
        for child in children.drain(range.clone()) {
            self.push_child(wrapper, child);
        }
        children.insert(range.start, wrapper);
        self.replace_children(parent, children);
    }

    /// Replace all children of `parent` in one operation.
    ///
    /// Children may be existing direct children (in any order) or detached
    /// nodes. Retained nodes keep their IDs and metadata; omitted old children
    /// are deleted with their subtrees. An empty list clears the parent.
    /// The caller must ensure attaching would not create an ancestor cycle;
    /// this is checked only with debug assertions enabled.
    ///
    /// Prefer this over repeated sibling edits for large lists: it scans the old
    /// and new lists once, plus any removed subtrees.
    ///
    /// ```
    /// use markdown_it::{MarkdownIt, document::Text};
    ///
    /// let mut document = MarkdownIt::new().parse("first\n\nsecond");
    /// let root = document.root();
    /// let kept = document.children(root)[1];
    /// let added = document.create_node(Text { content: "prefix".into() });
    /// document.replace_children(root, vec![added, kept]);
    /// assert_eq!(document.children(root), &[added, kept]);
    /// document.replace_children(root, Vec::new());
    /// assert_eq!(document.len(), 1);
    /// ```
    ///
    /// # Panics
    ///
    /// Panics before mutation if any ID is invalid, a child is duplicated or
    /// attached to another parent, or the root or parent is used as a child.
    /// Ancestor cycles panic before mutation only with debug assertions enabled.
    /// On failure, detached nodes remain in the document for reuse or
    /// discard.
    #[track_caller]
    pub fn replace_children(&mut self, parent: NodeId, children: Vec<NodeId>) {
        if self.children(parent) == children.as_slice() {
            return;
        }

        #[cfg(debug_assertions)]
        let mut check_ancestors = false;

        let mut retained = HashSet::with_capacity(children.len());
        for &child in &children {
            assert_ne!(child, self.root, "cannot attach the document root");
            assert_ne!(child, parent, "cannot attach a node to itself");

            let node = self.node(child);
            assert!(
                node.parent.is_none() || node.parent == Some(parent),
                "child must be detached or already belong to this parent"
            );
            assert!(retained.insert(child), "duplicate child ID");

            #[cfg(debug_assertions)]
            {
                check_ancestors |= node.parent.is_none() && !node.children.is_empty();
            }
        }

        #[cfg(debug_assertions)]
        if check_ancestors {
            let mut ancestor = self.parent(parent);
            while let Some(id) = ancestor {
                assert!(!retained.contains(&id), "cannot create a cycle");
                ancestor = self.parent(id);
            }
        }

        // Validation passed; links and payloads are still unchanged.
        for &child in &children {
            self.node_mut(child).parent = Some(parent);
        }
        let mut removed = std::mem::replace(&mut self.node_mut(parent).children, children);
        removed.retain(|child| !retained.contains(child));
        self.delete_subtrees(removed);
    }

    #[track_caller]
    fn sibling_location(&self, node: NodeId) -> (NodeId, usize) {
        let parent = self
            .node(node)
            .parent
            .expect("target must be attached and not the document root");
        let index = self
            .children(parent)
            .iter()
            .position(|&child| child == node)
            .expect("document parent links are internally consistent");
        (parent, index)
    }

    #[track_caller]
    pub(super) fn check_attachment(&self, parent: NodeId, child: NodeId) {
        self.node(parent);
        assert_ne!(child, self.root, "cannot attach the document root");
        assert!(self.node(child).parent.is_none(), "child must be detached");
        assert_ne!(parent, child, "cannot attach a node to itself");

        #[cfg(debug_assertions)]
        if !self.children(child).is_empty() {
            let mut ancestor = Some(parent);
            while let Some(id) = ancestor {
                assert_ne!(id, child, "cannot create a cycle");
                ancestor = self.parent(id);
            }
        }
    }

    fn delete_subtree(&mut self, root: NodeId) {
        if self.children(root).is_empty() {
            self.arena.remove(root).expect("subtree root must exist");
            return;
        }
        self.delete_subtrees(vec![root]);
    }

    fn delete_subtrees(&mut self, mut pending: Vec<NodeId>) {
        // Collect all removed nodes, then delete children before parents.
        let mut nodes = Vec::new();
        while let Some(id) = pending.pop() {
            pending.extend_from_slice(self.children(id));
            nodes.push(id);
        }
        for node in nodes.into_iter().rev() {
            self.arena
                .remove(node)
                .expect("collected subtree node must still exist");
        }
    }
}
