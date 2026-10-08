use std::collections::{HashMap, HashSet};

use super::{Document, NodeDraft, NodeId};

#[derive(Clone, Copy, Debug)]
pub(crate) enum SiblingPosition {
    Before,
    After,
}

impl Document {
    pub(crate) fn remove_subtrees(&mut self, roots: &[NodeId]) {
        if let [root] = roots {
            // single root optimization
            let parent = self
                .arena
                .get(*root)
                .expect("subtree root must exist")
                .parent
                .expect("subtree root must not be the document root");
            let siblings = &mut self
                .arena
                .get_mut(parent)
                .expect("subtree parent must exist")
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
                        .expect("subtree root must exist")
                        .parent
                        .expect("subtree root must not be the document root")
                })
                .collect();
            for parent in parents {
                self.arena
                    .get_mut(parent)
                    .expect("subtree parent must exist")
                    .children
                    .retain(|child| !root_set.contains(child));
            }
        }

        self.delete_subtrees(roots);
    }

    fn delete_subtrees(&mut self, roots: &[NodeId]) {
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
                .expect("collected subtree node must still exist");
        }
    }

    pub(crate) fn replace_subtrees(&mut self, replacements: Vec<(NodeId, NodeDraft)>) {
        let mut by_target = HashMap::with_capacity(replacements.len());
        let mut affected_parents = HashSet::new();
        let mut replaced_roots = Vec::with_capacity(replacements.len());
        // insert new subtree
        for (target, draft) in replacements {
            let parent = self
                .arena
                .get(target)
                .expect("replacement target must exist")
                .parent
                .expect("replacement target must not be the document root");
            let replacement = self.insert_draft(parent, draft);
            let previous = by_target.insert(target, replacement);
            debug_assert!(previous.is_none(), "replacement targets must be unique");
            affected_parents.insert(parent);
            replaced_roots.push(target);
        }

        // replace old
        for parent in affected_parents {
            let children = &mut self
                .arena
                .get_mut(parent)
                .expect("replacement parent must exist")
                .children;
            for child in children {
                if let Some(&replacement) = by_target.get(child) {
                    *child = replacement;
                }
            }
        }

        self.delete_subtrees(&replaced_roots);
    }

    pub(crate) fn wrap_ranges(&mut self, ranges: Vec<(NodeId, NodeId, NodeDraft)>) {
        let mut by_first = HashMap::with_capacity(ranges.len());
        let mut affected_parents = HashSet::new();
        for (first, last, wrapper) in ranges {
            let parent = self
                .arena
                .get(first)
                .expect("wrap endpoint must exist")
                .parent
                .expect("wrap endpoint must not be the document root");
            let wrapper = self.insert_draft(parent, wrapper);
            let previous = by_first.insert(first, (last, wrapper));
            debug_assert!(previous.is_none(), "wrap ranges must be disjoint");
            affected_parents.insert(parent);
        }

        for parent in affected_parents {
            let old_children = std::mem::take(
                &mut self
                    .arena
                    .get_mut(parent)
                    .expect("wrap parent must exist")
                    .children,
            );
            let mut children = Vec::with_capacity(old_children.len());
            let mut index = 0;
            while index < old_children.len() {
                let first = old_children[index];
                let Some(&(last, wrapper)) = by_first.get(&first) else {
                    children.push(first);
                    index += 1;
                    continue;
                };

                let end = old_children[index..]
                    .iter()
                    .position(|&node| node == last)
                    .map(|offset| index + offset)
                    .expect("wrap range must remain ordered");
                let wrapped = old_children[index..=end].to_vec();
                for &node in &wrapped {
                    self.arena
                        .get_mut(node)
                        .expect("wrapped node must exist")
                        .parent = Some(wrapper);
                }
                self.arena
                    .get_mut(wrapper)
                    .expect("new wrapper must remain present")
                    .children = wrapped;
                children.push(wrapper);
                index = end + 1;
            }
            self.arena
                .get_mut(parent)
                .expect("wrap parent must exist")
                .children = children;
        }
    }

    pub(crate) fn insert_siblings(
        &mut self,
        insertions: Vec<(NodeId, SiblingPosition, NodeDraft)>,
    ) {
        #[derive(Default)]
        struct InsertedSiblings {
            before: Vec<NodeId>,
            after: Vec<NodeId>,
        }

        // grouping
        let mut by_target: HashMap<NodeId, InsertedSiblings> = HashMap::new();
        let mut affected_parents = HashSet::new();
        for (target, position, draft) in insertions {
            let parent = self
                .arena
                .get(target)
                .expect("insertion target must exist")
                .parent
                .expect("insertion target must not be the document root");
            let inserted = self.insert_draft(parent, draft);
            let siblings = by_target.entry(target).or_default();
            match position {
                SiblingPosition::Before => siblings.before.push(inserted),
                SiblingPosition::After => siblings.after.push(inserted),
            }
            affected_parents.insert(parent);
        }

        // rebuild
        for parent in affected_parents {
            let old_children = std::mem::take(
                &mut self
                    .arena
                    .get_mut(parent)
                    .expect("insertion parent must exist")
                    .children,
            );
            let inserted_count = old_children
                .iter()
                .filter_map(|child| by_target.get(child))
                .map(|siblings| siblings.before.len() + siblings.after.len())
                .sum::<usize>();
            let mut children = Vec::with_capacity(old_children.len() + inserted_count);
            for child in old_children {
                if let Some(siblings) = by_target.get(&child) {
                    children.extend_from_slice(&siblings.before);
                }
                children.push(child);
                if let Some(siblings) = by_target.get(&child) {
                    children.extend_from_slice(&siblings.after);
                }
            }
            self.arena
                .get_mut(parent)
                .expect("insertion parent must exist")
                .children = children;
        }
    }

    // dfs
    fn insert_draft(&mut self, parent: NodeId, draft: NodeDraft) -> NodeId {
        self.insert_draft_with_parent(Some(parent), draft)
    }

    pub(super) fn insert_draft_with_parent(
        &mut self,
        root_parent: Option<NodeId>,
        draft: NodeDraft,
    ) -> NodeId {
        let mut pending = vec![(root_parent, false, draft)];
        let mut root = None;
        while let Some((parent, link_to_parent, draft)) = pending.pop() {
            let (children, data) = draft.into_parts();
            let id = self.arena.insert_with(|id| super::node::DocumentNode {
                id,
                parent,
                children: Vec::with_capacity(children.len()),
                data,
            });
            if link_to_parent {
                self.arena
                    .get_mut(parent.expect("a linked draft always has a parent"))
                    .expect("new draft parent remains present")
                    .children
                    .push(id);
            } else {
                root = Some(id);
            }
            pending.extend(
                children
                    .into_iter()
                    .rev()
                    .map(|child| (Some(id), true, child)),
            );
        }
        root.expect("a draft always contains a root node")
    }
}
