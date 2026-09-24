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
