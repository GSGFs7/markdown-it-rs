use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use markdown_it::document::edit::EditBatch;
use markdown_it::document::{Document, NodeDraft, NodeId, Root, Text};
use markdown_it::plugins::cmark::block::paragraph::Paragraph;

fn document() -> Document {
    Document::from_draft("", NodeDraft::new(Root::new("")))
}

fn text(document: &mut Document, content: &str) -> NodeId {
    document.create_node(Text {
        content: content.into(),
    })
}

#[test]
fn immediate_insertions_preserve_ids_metadata_and_current_positions() {
    let mut doc = document();
    let root = doc.root();
    let target = text(&mut doc, "target");
    doc.push_child(root, target);
    let before1 = doc.create_node(Paragraph);
    let child = text(&mut doc, "child");
    doc.push_child(before1, child);
    doc.node_mut(before1)
        .attrs_mut()
        .push(("class".into(), "generated".into()));
    doc.node_mut(before1).ext_mut().insert(42_u32);
    #[cfg(feature = "sourcemap")]
    doc.node_mut(before1)
        .set_srcmap(Some(markdown_it::common::sourcemap::SourcePos::new(0, 0)));
    let before2 = text(&mut doc, "before2");
    let after1 = text(&mut doc, "after1");
    let after2 = text(&mut doc, "after2");
    doc.insert_before(target, before1);
    doc.insert_before(target, before2);
    doc.insert_after(target, after1);
    doc.insert_after(target, after2);
    assert_eq!(
        doc.children(root),
        &[before1, before2, target, after2, after1]
    );
    for &id in doc.children(root) {
        assert_eq!(doc.parent(id), Some(root));
    }
    assert_eq!(doc.children(before1), &[child]);
    assert_eq!(doc.parent(child), Some(before1));
    assert_eq!(doc.node(before1).attrs()[0].1, "generated");
    assert_eq!(doc.node(before1).ext().get::<u32>(), Some(&42));
    #[cfg(feature = "sourcemap")]
    assert!(doc.node(before1).srcmap().is_some());
}

#[derive(Debug)]
struct Payload(Arc<AtomicUsize>);
impl Drop for Payload {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn replacement_and_removal_drop_whole_subtrees_and_invalidate_old_ids() {
    let drops = Arc::new(AtomicUsize::new(0));
    let mut doc = document();
    let root = doc.root();
    let old = doc.create_node(Payload(drops.clone()));
    let old_child = doc.create_node(Payload(drops.clone()));
    doc.push_child(old, old_child);
    doc.push_child(root, old);
    let kept = text(&mut doc, "kept");
    doc.push_child(root, kept);
    let replacement = doc.create_node(Payload(drops.clone()));
    let child = text(&mut doc, "new child");
    doc.push_child(replacement, child);
    doc.replace_node(old, replacement);
    assert_eq!(drops.load(Ordering::SeqCst), 2);
    assert!(doc.get_node(old).is_none());
    assert!(doc.get_node(old_child).is_none());
    assert_eq!(doc.children(root), &[replacement, kept]);
    assert_eq!(doc.children(replacement), &[child]);
    assert_eq!(doc.parent(child), Some(replacement));
    assert_eq!(doc.parent(replacement), Some(root));
    doc.remove_node(replacement);
    assert_eq!(drops.load(Ordering::SeqCst), 3);
    assert!(doc.get_node(child).is_none());
    assert!(doc.get_node(replacement).is_none());
    assert_eq!(doc.children(root), &[kept]);
    let reused = text(&mut doc, "reused");
    assert_ne!(reused, replacement);
    assert!(doc.get_node(replacement).is_none());
}

fn assert_rejected(doc: &mut Document, operation: impl FnOnce(&mut Document)) {
    let before = format!("{doc:?}");
    assert!(catch_unwind(AssertUnwindSafe(|| operation(doc))).is_err());
    assert_eq!(format!("{doc:?}"), before);
}

#[test]
fn invalid_structural_operations_panic_before_mutation_in_all_builds() {
    let mut doc = document();
    let root = doc.root();
    let target = text(&mut doc, "target");
    doc.push_child(root, target);
    let detached = text(&mut doc, "detached");
    let stale = text(&mut doc, "stale");
    doc.discard_node(stale);
    for invalid_target in [root, detached, stale] {
        assert_rejected(&mut doc, |d| d.remove_node(invalid_target));
        assert_rejected(&mut doc, |d| d.insert_before(invalid_target, detached));
        assert_rejected(&mut doc, |d| d.insert_after(invalid_target, detached));
        assert_rejected(&mut doc, |d| d.replace_node(invalid_target, detached));
    }
    for invalid_child in [root, target, stale] {
        assert_rejected(&mut doc, |d| d.insert_before(target, invalid_child));
        assert_rejected(&mut doc, |d| d.insert_after(target, invalid_child));
        assert_rejected(&mut doc, |d| d.replace_node(target, invalid_child));
    }
    doc.insert_before(target, detached);
    assert_eq!(doc.children(root), &[detached, target]);
}

#[test]
#[cfg(debug_assertions)]
fn sibling_edits_reject_ancestor_cycles_before_mutation() {
    let mut doc = document();
    // Detached targets can also create cycles: attaching an ancestor next to a
    // descendant, or replacing the descendant with it, must panic.
    let ancestor = doc.create_node(Paragraph);
    let parent = doc.create_node(Paragraph);
    let descendant = text(&mut doc, "descendant");
    doc.push_child(ancestor, parent);
    doc.push_child(parent, descendant);
    for child in [ancestor, parent] {
        assert_rejected(&mut doc, |d| d.insert_before(descendant, child));
        assert_rejected(&mut doc, |d| d.insert_after(descendant, child));
        assert_rejected(&mut doc, |d| d.replace_node(descendant, child));
    }
}

#[test]
fn deep_subtrees_can_be_inserted_replaced_and_removed_without_recursion() {
    let mut doc = document();
    let root = doc.root();
    let target = text(&mut doc, "target");
    doc.push_child(root, target);
    let leaf = text(&mut doc, "leaf");
    let mut subtree = leaf;
    for _ in 0..20_000 {
        let parent = doc.create_node(Paragraph);
        doc.push_child(parent, subtree);
        subtree = parent;
    }
    doc.insert_before(target, subtree);
    assert_eq!(doc.parent(subtree), Some(root));
    let replacement = text(&mut doc, "replacement");
    doc.replace_node(subtree, replacement);
    assert!(doc.get_node(leaf).is_none());
    assert!(doc.get_node(subtree).is_none());
    assert_eq!(doc.len(), 3);
    doc.remove_node(replacement);
    assert_eq!(doc.children(root), &[target]);
}

#[test]
fn content_batches_can_commit_before_structural_changes() {
    let mut doc = document();
    let root = doc.root();
    let target = text(&mut doc, "target");
    doc.push_child(root, target);
    let inserted = text(&mut doc, "inserted");
    let mut edits = EditBatch::new();
    edits.replace_text(target, 0..1, "T");
    edits.set_attribute(target, "class", "edited");
    edits.commit(&mut doc);
    doc.insert_before(target, inserted);
    assert_eq!(doc.node(target).cast::<Text>().unwrap().content, "Target");
    assert_eq!(doc.node(target).attrs()[0].1, "edited");
    assert_eq!(doc.children(root), &[inserted, target]);
}

#[test]
fn bulk_replacement_reorders_retained_nodes_and_attaches_generated_subtrees() {
    let mut doc = document();
    let root = doc.root();
    let first = doc.create_node(Paragraph);
    let nested = text(&mut doc, "nested");
    doc.push_child(first, nested);
    doc.push_child(root, first);
    doc.node_mut(first)
        .attrs_mut()
        .push(("class".into(), "kept".into()));
    doc.node_mut(first).ext_mut().insert(42_u32);
    #[cfg(feature = "sourcemap")]
    doc.node_mut(first)
        .set_srcmap(Some(markdown_it::common::sourcemap::SourcePos::new(0, 0)));
    let last = text(&mut doc, "last");
    doc.push_child(root, last);
    let generated = doc.create_node(Paragraph);
    let generated_child = text(&mut doc, "generated");
    doc.push_child(generated, generated_child);

    doc.replace_children(root, vec![last, generated, first]);

    assert_eq!(doc.children(root), &[last, generated, first]);
    for &child in doc.children(root) {
        assert_eq!(doc.parent(child), Some(root));
    }
    assert_eq!(doc.children(first), &[nested]);
    assert_eq!(doc.parent(nested), Some(first));
    assert_eq!(doc.children(generated), &[generated_child]);
    assert_eq!(doc.parent(generated_child), Some(generated));
    assert_eq!(doc.node(first).attrs()[0].1, "kept");
    assert_eq!(doc.node(first).ext().get::<u32>(), Some(&42));
    #[cfg(feature = "sourcemap")]
    assert!(doc.node(first).srcmap().is_some());
    let before = format!("{doc:?}");
    doc.replace_children(root, vec![last, generated, first]);
    assert_eq!(format!("{doc:?}"), before);
}

#[test]
fn bulk_replacement_deletes_only_omitted_subtrees_and_clear_releases_the_rest() {
    let drops = Arc::new(AtomicUsize::new(0));
    let mut doc = document();
    let root = doc.root();
    let kept = doc.create_node(Payload(drops.clone()));
    let omitted = doc.create_node(Payload(drops.clone()));
    let descendant = doc.create_node(Payload(drops.clone()));
    doc.push_child(omitted, descendant);
    doc.push_child(root, omitted);
    doc.push_child(root, kept);
    let generated = doc.create_node(Payload(drops.clone()));

    doc.replace_children(root, vec![kept, generated]);

    assert_eq!(doc.children(root), &[kept, generated]);
    assert_eq!(drops.load(Ordering::SeqCst), 2);
    assert!(doc.get_node(omitted).is_none());
    assert!(doc.get_node(descendant).is_none());
    let reused = text(&mut doc, "reused slot");
    assert!(doc.get_node(omitted).is_none());
    assert!(doc.get_node(descendant).is_none());
    doc.discard_node(reused);
    doc.replace_children(root, Vec::new());
    assert_eq!(drops.load(Ordering::SeqCst), 4);
    assert!(doc.get_node(kept).is_none());
    assert!(doc.get_node(generated).is_none());
    assert_eq!(doc.len(), 1);
    assert_eq!(doc.parent(root), None);
    doc.replace_children(root, Vec::new());
    assert_eq!(doc.len(), 1);
}

#[test]
fn bulk_replacement_rejects_invalid_lists_before_changing_links_or_dropping_payloads() {
    let drops = Arc::new(AtomicUsize::new(0));
    let mut doc = document();
    let root = doc.root();
    let parent = doc.create_node(Paragraph);
    doc.push_child(root, parent);
    let kept = doc.create_node(Payload(drops.clone()));
    doc.push_child(parent, kept);
    let foreign = text(&mut doc, "belongs to root");
    doc.push_child(root, foreign);
    let new = text(&mut doc, "detached");
    let stale = text(&mut doc, "stale");
    doc.discard_node(stale);
    for children in [
        vec![kept, kept],
        vec![new, new],
        vec![new, foreign],
        vec![new, stale],
        vec![new, root],
        vec![new, parent],
        vec![new, parent, kept],
    ] {
        assert_rejected(&mut doc, |d| d.replace_children(parent, children));
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        assert_eq!(doc.parent(new), None);
        assert_eq!(doc.children(parent), &[kept]);
    }
    assert_rejected(&mut doc, |d| d.replace_children(stale, vec![new]));
    assert_rejected(&mut doc, |d| d.replace_children(stale, Vec::new()));
    // A descendant cannot escape an omitted subtree implicitly.
    assert_rejected(&mut doc, |d| d.replace_children(root, vec![kept]));
    doc.replace_children(parent, vec![new, kept]);
    assert_eq!(doc.children(parent), &[new, kept]);
}

#[test]
#[cfg(debug_assertions)]
fn bulk_replacement_rejects_cycles_in_detached_trees() {
    let mut doc = document();
    let ancestor = doc.create_node(Paragraph);
    let parent = doc.create_node(Paragraph);
    let leaf = text(&mut doc, "leaf");
    doc.push_child(ancestor, parent);
    doc.push_child(parent, leaf);
    let new = text(&mut doc, "new");
    assert_rejected(&mut doc, |d| {
        d.replace_children(parent, vec![new, ancestor])
    });
    assert_rejected(&mut doc, |d| d.replace_children(leaf, vec![new, ancestor]));
    assert_rejected(&mut doc, |d| d.replace_children(parent, vec![parent]));
    assert_eq!(doc.parent(ancestor), None);
    assert_eq!(doc.parent(new), None);
    doc.replace_children(parent, vec![new, leaf]);
    assert_eq!(doc.children(parent), &[new, leaf]);
    doc.discard_node(ancestor);
    assert_eq!(doc.len(), 1);
}

#[test]
fn bulk_replacement_handles_wide_and_deep_removed_subtrees_iteratively() {
    let mut doc = document();
    let root = doc.root();
    let leaf = text(&mut doc, "deep leaf");
    let mut deep = leaf;
    for _ in 0..20_000 {
        let parent = doc.create_node(Paragraph);
        doc.push_child(parent, deep);
        deep = parent;
    }
    let mut children = vec![deep];
    for _ in 0..10_000 {
        children.push(text(&mut doc, "wide leaf"));
    }
    let kept = children[5000];
    doc.replace_children(root, children);
    assert_eq!(doc.children(root).len(), 10_001);
    doc.replace_children(root, vec![kept]);
    assert_eq!(doc.len(), 2);
    assert_eq!(doc.children(root), &[kept]);
    assert!(doc.get_node(deep).is_none());
    assert!(doc.get_node(leaf).is_none());
    doc.replace_children(root, Vec::new());
    assert_eq!(doc.len(), 1);
}
