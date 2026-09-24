use super::*;
use crate::document::Node;
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
    &document.node(node).cast::<Text>().unwrap().content
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
    let children = document.children(document.root());
    let first = children[0];
    let second = children[1];
    let mut batch = EditBatch::new();
    batch.replace_text(second, 1..2, "E");
    batch.replace_char(first, 1..4, '雨');
    batch.replace_text(first, 0..1, "A");

    batch.commit(&mut document);

    assert_eq!(content(&document, first), "A雨c");
    assert_eq!(content(&document, second), "dEf");
}

#[test]
fn adjacent_ranges_and_same_position_insertions_are_deterministic() {
    let mut document = document(&["abcd"]);
    let text = document.children(document.root())[0];
    let mut batch = EditBatch::new();
    batch.replace_text(text, 0..1, "A");
    batch.replace_text(text, 1..2, "B");
    batch.replace_text(text, 2..2, "first");
    batch.replace_text(text, 2..2, "second");

    batch.commit(&mut document);
    assert_eq!(content(&document, text), "ABfirstsecondcd");
}

#[test]
fn supports_deletion() {
    let mut document = document(&["abcdef"]);
    let text = document.children(document.root())[0];
    let mut delete = EditBatch::new();
    delete.replace_text(text, 1..3, "");
    delete.commit(&mut document);
    assert_eq!(content(&document, text), "adef");
}

#[test]
fn empty_batch_is_a_no_op() {
    let mut document = document(&["unchanged"]);
    EditBatch::new().commit(&mut document);
    let text = document.children(document.root())[0];
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
    let text = document.children(document.root())[0];
    let mut batch = EditBatch::new();
    batch.set_attribute(text, "class", "new");
    batch.remove_attribute(text, "id");
    batch.set_attribute(text, "title", "added");

    batch.commit(&mut document);

    assert_eq!(
        document.node(text).attrs(),
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
    let text = document.children(document.root())[0];
    let mut batch = EditBatch::new();
    batch.replace_text(text, 0..1, "A");
    batch.set_attribute(text, "data-state", "edited");

    assert_eq!(batch.len(), 2);
    batch.commit(&mut document);

    assert_eq!(content(&document, text), "Abc");
    assert_eq!(
        document.node(text).attrs(),
        &[("data-state".to_owned(), "edited".to_owned())]
    );
}

#[test]
fn text_attribute_and_source_map_edits_commit_together() {
    let mut document = document(&["abc"]);
    let text = document.children(document.root())[0];
    let mut batch = EditBatch::new();
    batch.replace_text(text, 0..1, "A");
    batch.set_attribute(text, "data-state", "edited");
    batch.set_source_map(text, Some(SourcePos::new(1, 3)));

    assert_eq!(batch.len(), 3);
    batch.commit(&mut document);

    assert_eq!(content(&document, text), "Abc");
    assert_eq!(
        document.node(text).srcmap().unwrap().get_byte_offsets(),
        (1, 3)
    );
}

#[test]
fn different_attribute_names_and_case_are_independent() {
    let mut document = document(&["text"]);
    let text = document.children(document.root())[0];
    let mut batch = EditBatch::new();
    batch.set_attribute(text, "class", "lower");
    batch.set_attribute(text, "CLASS", "upper");

    batch.commit(&mut document);

    assert_eq!(document.node(text).attrs().len(), 2);
}

#[test]
fn removes_a_complete_subtree_and_invalidates_all_ids() {
    let mut document = branched_document();
    let root = document.root();
    let branches = document.children(root);
    let removed = branches[0];
    let kept = branches[1];
    let removed_text = document.children(removed)[0];
    let mut batch = EditBatch::new();
    batch.remove_node(removed);

    batch.commit(&mut document);

    assert_eq!(document.len(), 3);
    assert_eq!(document.children(root), &[kept]);
    assert_eq!(document.parent(kept), Some(root));
    assert!(document.get_node(removed).is_none());
    assert!(document.get_node(removed_text).is_none());
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
    let branches = document.children(root).to_vec();
    let mut batch = EditBatch::new();
    batch.remove_node(branches[1]);
    batch.remove_node(branches[0]);

    assert_eq!(batch.len(), 2);
    batch.commit(&mut document);

    assert_eq!(document.len(), 1);
    assert!(document.children(root).is_empty());
}

#[test]
fn edits_outside_a_removed_subtree_commit_normally() {
    let mut document = branched_document();
    let root = document.root();
    let branches = document.children(root);
    let removed = branches[0];
    let kept = branches[1];
    let kept_text = document.children(kept)[0];
    let mut batch = EditBatch::new();
    batch.remove_node(removed);
    batch.replace_text(kept_text, 0..1, "B");
    batch.set_attribute(root, "class", "edited");

    batch.commit(&mut document);

    assert_eq!(content(&document, kept_text), "B");
    assert_eq!(document.children(root), &[kept]);
    assert_eq!(
        document.node(root).attrs(),
        &[("class".to_owned(), "edited".to_owned())]
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
    let subtree = document.children(document.root())[0];
    let mut batch = EditBatch::new();
    batch.remove_node(subtree);

    batch.commit(&mut document);

    assert_eq!(document.len(), 1);
    assert!(document.children(document.root()).is_empty());
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
    let original = document.children(root).to_vec();
    let target = original[1];
    let mut batch = EditBatch::new();
    batch.insert_before(target, text_draft("before-1"));
    batch.insert_after(target, text_draft("after-1"));
    batch.insert_before(target, text_draft("before-2"));
    batch.insert_after(target, text_draft("after-2"));

    assert_eq!(batch.len(), 4);
    batch.commit(&mut document);

    let children = document.children(root);
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
        assert_eq!(document.parent(inserted), Some(root));
        assert!(document.node(inserted).srcmap().is_none());
    }
}

#[test]
fn inserts_an_owned_subtree_without_cloning_payloads() {
    #[derive(Debug)]
    struct NonClonePayload(&'static str);
    impl crate::NodeValue for NonClonePayload {}

    let mut document = document(&["anchor"]);
    let root = document.root();
    let anchor = document.children(root)[0];
    let mut draft = NodeDraft::new(NonClonePayload("parent"));
    draft
        .attrs_mut()
        .push(("data-generated".to_owned(), "yes".to_owned()));
    draft.push_child(text_draft("child-1"));
    draft.push_child(text_draft("child-2"));
    let mut batch = EditBatch::new();
    batch.insert_before(anchor, draft);

    batch.commit(&mut document);

    let inserted = document.children(root)[0];
    let node = document.node(inserted);
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
            .all(|&child| document.parent(child) == Some(inserted))
    );
}

#[test]
fn insertion_next_to_a_kept_subtree_can_commit_with_removal() {
    let mut document = branched_document();
    let root = document.root();
    let branches = document.children(root).to_vec();
    let mut batch = EditBatch::new();
    batch.remove_node(branches[0]);
    batch.insert_before(branches[1], text_draft("inserted"));

    batch.commit(&mut document);

    let children = document.children(root);
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
    let anchor = document.children(document.root())[0];
    let mut batch = EditBatch::new();
    batch.insert_before(anchor, draft);

    batch.commit(&mut document);

    assert_eq!(document.len(), 10_003);
    assert_eq!(document.children(document.root()).len(), 2);
}

#[test]
fn replaces_a_complete_subtree_in_place_and_invalidates_old_ids() {
    let mut document = branched_document();
    let root = document.root();
    let original = document.children(root).to_vec();
    let replaced = original[0];
    let replaced_child = document.children(replaced)[0];
    let mut draft = NodeDraft::new(Paragraph);
    draft.push_child(text_draft("replacement"));
    let mut batch = EditBatch::new();
    batch.replace_node(replaced, draft);

    assert_eq!(batch.len(), 1);
    batch.commit(&mut document);

    let children = document.children(root);
    assert_eq!(children.len(), 2);
    assert_eq!(children[1], original[1]);
    let replacement = children[0];
    assert_ne!(replacement, replaced);
    assert!(document.node(replacement).is::<Paragraph>());
    assert_eq!(document.parent(replacement), Some(root));
    let replacement_child = document.children(replacement)[0];
    assert_eq!(content(&document, replacement_child), "replacement");
    assert_eq!(document.parent(replacement_child), Some(replacement));
    assert!(document.get_node(replaced).is_none());
    assert!(document.get_node(replaced_child).is_none());

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
    let original = document.children(root).to_vec();
    let mut batch = EditBatch::new();
    batch.replace_node(original[2], text_draft("C"));
    batch.replace_node(original[0], text_draft("A"));

    batch.commit(&mut document);

    let children = document.children(root);
    let contents: Vec<_> = children
        .iter()
        .map(|&node| content(&document, node))
        .collect();
    assert_eq!(contents, ["A", "b", "C"]);
    assert_eq!(children[1], original[1]);
    assert!(document.get_node(original[0]).is_none());
    assert!(document.get_node(original[2]).is_none());
}

#[test]
fn disjoint_structural_and_value_edits_commit_together() {
    let mut document = document(&["a", "b", "c"]);
    let root = document.root();
    let original = document.children(root).to_vec();
    let mut batch = EditBatch::new();
    batch.remove_node(original[0]);
    batch.replace_node(original[1], text_draft("replacement"));
    batch.insert_before(original[2], text_draft("inserted"));
    batch.replace_text(original[2], 0..1, "C");
    batch.set_attribute(root, "class", "edited");

    batch.commit(&mut document);

    let children = document.children(root);
    let contents: Vec<_> = children
        .iter()
        .map(|&node| content(&document, node))
        .collect();
    assert_eq!(contents, ["replacement", "inserted", "C"]);
    assert_eq!(children[2], original[2]);
    assert_eq!(
        document.node(root).attrs(),
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
    let replaced = document.children(document.root())[0];
    let mut batch = EditBatch::new();
    batch.replace_node(replaced, draft);

    batch.commit(&mut document);

    assert_eq!(document.len(), 10_002);
    assert_eq!(document.children(document.root()).len(), 1);
    assert!(document.get_node(replaced).is_none());
}

#[test]
fn wraps_an_inclusive_sibling_range_without_changing_node_ids() {
    let mut document = document(&["a", "b", "c", "d"]);
    let root = document.root();
    let original = document.children(root).to_vec();
    let mut wrapper = NodeDraft::new(Paragraph);
    wrapper
        .attrs_mut()
        .push(("class".to_owned(), "wrapper".to_owned()));
    let mut batch = EditBatch::new();
    batch.wrap_range(original[1], original[2], wrapper);

    assert_eq!(batch.len(), 1);
    batch.commit(&mut document);

    let children = document.children(root);
    assert_eq!(children.len(), 3);
    assert_eq!(children[0], original[0]);
    assert_eq!(children[2], original[3]);
    let wrapper = children[1];
    let wrapper_node = document.node(wrapper);
    assert!(wrapper_node.is::<Paragraph>());
    assert!(wrapper_node.srcmap().is_none());
    assert_eq!(
        wrapper_node.attrs(),
        &[("class".to_owned(), "wrapper".to_owned())]
    );
    assert_eq!(wrapper_node.children(), &original[1..=2]);
    assert_eq!(document.parent(original[1]), Some(wrapper));
    assert_eq!(document.parent(original[2]), Some(wrapper));
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
    let original = document.children(root).to_vec();
    let mut batch = EditBatch::new();
    batch.wrap_range(original[3], original[4], NodeDraft::new(Paragraph));
    batch.wrap_range(original[0], original[1], NodeDraft::new(Paragraph));

    batch.commit(&mut document);

    let children = document.children(root);
    assert_eq!(children.len(), 3);
    assert_eq!(children[1], original[2]);
    assert_eq!(document.children(children[0]), &original[0..=1]);
    assert_eq!(document.children(children[2]), &original[3..=4]);
    for &node in &original {
        assert!(document.get_node(node).is_some());
    }
}

#[test]
fn wraps_ranges_under_different_parents_in_one_batch() {
    let mut document = branched_document();
    let branches = document.children(document.root()).to_vec();
    let first_text = document.children(branches[0])[0];
    let second_text = document.children(branches[1])[0];
    let mut batch = EditBatch::new();
    batch.wrap_range(first_text, first_text, NodeDraft::new(Paragraph));
    batch.wrap_range(second_text, second_text, NodeDraft::new(Paragraph));

    batch.commit(&mut document);

    for (branch, text) in branches.into_iter().zip([first_text, second_text]) {
        let wrapper = document.children(branch)[0];
        assert_eq!(document.children(wrapper), &[text]);
        assert_eq!(document.parent(text), Some(wrapper));
    }
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
    let paragraph = document.children(root)[0];
    let texts = document.children(paragraph).to_vec();
    let mut batch = EditBatch::new();
    batch.wrap_range(paragraph, paragraph, NodeDraft::new(Paragraph));
    batch.remove_node(texts[0]);
    batch.insert_before(texts[1], text_draft("inserted"));
    batch.replace_text(texts[1], 0..1, "B");
    batch.set_attribute(paragraph, "class", "wrapped-child");

    batch.commit(&mut document);

    let wrapper = document.children(root)[0];
    assert_eq!(document.children(wrapper), &[paragraph]);
    let paragraph_children = document.children(paragraph);
    assert_eq!(paragraph_children.len(), 2);
    assert_eq!(content(&document, paragraph_children[0]), "inserted");
    assert_eq!(paragraph_children[1], texts[1]);
    assert_eq!(content(&document, texts[1]), "B");
    assert_eq!(
        document.node(paragraph).attrs(),
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
    let original = document.children(root).to_vec();
    let mut batch = EditBatch::new();
    batch.wrap_range(original[0], original[9_999], NodeDraft::new(Paragraph));

    batch.commit(&mut document);

    assert_eq!(document.len(), 10_002);
    let wrapper = document.children(root)[0];
    assert_eq!(document.children(wrapper), original);
    assert!(
        document
            .children(wrapper)
            .iter()
            .all(|&node| document.parent(node) == Some(wrapper))
    );
}

#[cfg(debug_assertions)]
mod validation_tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use super::*;
    use crate::document::edit::validation::ValidationError;

    fn error_for(mut batch: EditBatch, document: &Document) -> ValidationError {
        batch.normalize();
        batch.validate(document).unwrap_err()
    }

    fn assert_invalid(batch: EditBatch, document: &Document, expected: ValidationError) {
        assert_eq!(error_for(batch, document), expected);
    }

    #[test]
    fn rejects_invalid_text_edits_across_nodes() {
        let document = document(&["abc", "雪"]);
        let children = document.children(document.root());
        let first = children[0];
        let second = children[1];
        let mut batch = EditBatch::new();
        batch.replace_text(first, 0..1, "A");
        batch.replace_text(second, 1..2, "invalid UTF-8 boundary");

        assert_invalid(
            batch,
            &document,
            ValidationError::InvalidTextRange {
                node: second,
                range: 1..2,
            },
        );
    }

    #[test]
    fn rejects_non_text_and_overlapping_ranges() {
        let document = document(&["abcd"]);
        let text = document.children(document.root())[0];

        let mut overlap = EditBatch::new();
        overlap.replace_text(text, 0..2, "x");
        overlap.replace_text(text, 1..3, "y");
        assert_invalid(
            overlap,
            &document,
            ValidationError::OverlappingTextEdits {
                node: text,
                first: 0..2,
                second: 1..3,
            },
        );

        let root = document.root();
        let mut wrong_type = EditBatch::new();
        wrong_type.replace_text(root, 0..0, "x");
        assert_invalid(
            wrong_type,
            &document,
            ValidationError::NotEditableText(root),
        );

        let paragraph = Document::from_legacy("", Node::new(Paragraph));
        let paragraph_id = paragraph.root();
        let mut wrong_type = EditBatch::new();
        wrong_type.replace_text(paragraph_id, 0..0, "x");
        assert_invalid(
            wrong_type,
            &paragraph,
            ValidationError::NotEditableText(paragraph_id),
        );
    }

    #[test]
    fn rejects_reversed_text_ranges() {
        let document = document(&["abcdef"]);
        let text = document.children(document.root())[0];
        let mut reversed = EditBatch::new();
        let start = 3;
        let end = 1;
        reversed.replace_text(text, start..end, "x");

        assert_invalid(
            reversed,
            &document,
            ValidationError::InvalidTextRange {
                node: text,
                range: start..end,
            },
        );
    }

    #[test]
    fn rejects_conflicting_source_map_edits() {
        let document = document(&["abc"]);
        let text = document.children(document.root())[0];
        let mut batch = EditBatch::new();
        batch.replace_text(text, 0..1, "A");
        batch.set_source_map(text, Some(SourcePos::new(1, 2)));
        batch.set_source_map(text, None);

        assert_invalid(
            batch,
            &document,
            ValidationError::ConflictingSourceMapEdits { node: text },
        );
    }

    #[test]
    fn rejects_invalid_source_maps() {
        let document = document(&["雪"]);
        let text = document.children(document.root())[0];
        let mut batch = EditBatch::new();
        batch.set_attribute(text, "class", "new");
        batch.set_source_map(text, Some(SourcePos::new(1, 2)));

        assert_invalid(
            batch,
            &document,
            ValidationError::InvalidSourceMap {
                node: text,
                start: 1,
                end: 2,
            },
        );
    }

    #[test]
    fn rejects_conflicting_attribute_edits() {
        let document = document(&["abc"]);
        let text = document.children(document.root())[0];
        let mut batch = EditBatch::new();
        batch.replace_text(text, 0..1, "A");
        batch.set_attribute(text, "class", "first");
        batch.remove_attribute(text, "class");

        assert_invalid(
            batch,
            &document,
            ValidationError::ConflictingAttributeEdits {
                node: text,
                name: "class".to_owned(),
            },
        );
    }

    #[test]
    fn rejects_invalid_text_edits_beside_other_patches() {
        let document = document(&["雪"]);
        let text = document.children(document.root())[0];
        let mut batch = EditBatch::new();
        batch.set_attribute(text, "class", "new");
        batch.replace_text(text, 1..2, "invalid UTF-8 boundary");

        assert_invalid(
            batch,
            &document,
            ValidationError::InvalidTextRange {
                node: text,
                range: 1..2,
            },
        );
    }

    #[test]
    fn rejects_root_duplicate_and_overlapping_removals() {
        let document = branched_document();
        let root = document.root();
        let branch = document.children(root)[0];
        let text = document.children(branch)[0];

        let mut root_removal = EditBatch::new();
        root_removal.replace_text(text, 0..1, "A");
        root_removal.remove_node(root);
        assert_invalid(
            root_removal,
            &document,
            ValidationError::CannotRemoveRoot(root),
        );

        let mut duplicate = EditBatch::new();
        duplicate.set_attribute(root, "class", "changed");
        duplicate.remove_node(branch);
        duplicate.remove_node(branch);
        assert_invalid(
            duplicate,
            &document,
            ValidationError::DuplicateNodeRemoval(branch),
        );

        let mut overlap = EditBatch::new();
        overlap.remove_node(branch);
        overlap.remove_node(text);
        assert_invalid(
            overlap,
            &document,
            ValidationError::OverlappingNodeRemovals {
                ancestor: branch,
                descendant: text,
            },
        );
    }

    #[test]
    fn rejects_edits_inside_a_removed_subtree() {
        let document = branched_document();
        let branch = document.children(document.root())[0];
        let text = document.children(branch)[0];

        let mut text_conflict = EditBatch::new();
        text_conflict.remove_node(branch);
        text_conflict.replace_text(text, 0..1, "A");
        assert_invalid(
            text_conflict,
            &document,
            ValidationError::EditTargetsRemovedNode {
                removed: branch,
                edited: text,
            },
        );

        let mut attribute_conflict = EditBatch::new();
        attribute_conflict.remove_node(branch);
        attribute_conflict.set_attribute(branch, "class", "changed");
        assert_invalid(
            attribute_conflict,
            &document,
            ValidationError::EditTargetsRemovedNode {
                removed: branch,
                edited: branch,
            },
        );

        let mut source_map_conflict = EditBatch::new();
        source_map_conflict.remove_node(branch);
        source_map_conflict.set_source_map(text, None);
        assert_invalid(
            source_map_conflict,
            &document,
            ValidationError::EditTargetsRemovedNode {
                removed: branch,
                edited: text,
            },
        );
    }

    #[test]
    fn rejects_stale_removal_targets() {
        let mut document = branched_document();
        let removed = document.children(document.root())[0];
        let mut first = EditBatch::new();
        first.remove_node(removed);
        first.commit(&mut document);

        let mut stale = EditBatch::new();
        stale.remove_node(removed);
        assert_invalid(stale, &document, ValidationError::InvalidNode(removed));
    }

    #[test]
    fn rejects_stale_edit_targets_without_mutating_document() {
        let mut document = document(&["abc", "def"]);
        let children = document.children(document.root()).to_vec();
        let (stale, live) = (children[0], children[1]);
        let mut removal = EditBatch::new();
        removal.remove_node(stale);
        removal.commit(&mut document);

        let mut stale_edit = EditBatch::new();
        stale_edit.replace_text(stale, 0..0, "stale");
        assert_invalid(stale_edit, &document, ValidationError::InvalidNode(stale));

        let mut mixed = EditBatch::new();
        mixed.replace_text(live, 0..1, "D");
        mixed.set_attribute(stale, "class", "stale");
        assert_invalid(mixed, &document, ValidationError::InvalidNode(stale));
        assert_eq!(content(&document, live), "def");
    }

    #[test]
    fn rejects_root_and_stale_insertion_targets() {
        let mut document = document(&["abc"]);
        let root = document.root();
        let text = document.children(root)[0];

        let mut root_target = EditBatch::new();
        root_target.replace_text(text, 0..1, "A");
        root_target.insert_before(root, text_draft("invalid"));
        assert_invalid(
            root_target,
            &document,
            ValidationError::CannotInsertSiblingOfRoot(root),
        );

        let mut removal = EditBatch::new();
        removal.remove_node(text);
        removal.commit(&mut document);
        let mut stale = EditBatch::new();
        stale.insert_after(text, text_draft("invalid"));
        assert_invalid(stale, &document, ValidationError::InvalidNode(text));
    }

    #[test]
    fn rejects_insertions_inside_removed_subtrees() {
        let document = branched_document();
        let removed = document.children(document.root())[0];
        let target = document.children(removed)[0];
        let mut batch = EditBatch::new();
        batch.remove_node(removed);
        batch.insert_before(target, text_draft("invalid"));

        assert_invalid(
            batch,
            &document,
            ValidationError::InsertionTargetsRemovedNode { removed, target },
        );
    }

    #[test]
    fn rejects_root_duplicate_and_overlapping_replacements() {
        let document = branched_document();
        let root = document.root();
        let branch = document.children(root)[0];
        let text = document.children(branch)[0];

        let mut root_replacement = EditBatch::new();
        root_replacement.replace_text(text, 0..1, "A");
        root_replacement.replace_node(root, text_draft("invalid"));
        assert_invalid(
            root_replacement,
            &document,
            ValidationError::CannotReplaceRoot(root),
        );

        let mut duplicate = EditBatch::new();
        duplicate.set_attribute(root, "class", "changed");
        duplicate.replace_node(branch, text_draft("first"));
        duplicate.replace_node(branch, text_draft("second"));
        assert_invalid(
            duplicate,
            &document,
            ValidationError::DuplicateNodeReplacement(branch),
        );

        let mut overlap = EditBatch::new();
        overlap.replace_node(branch, text_draft("ancestor"));
        overlap.replace_node(text, text_draft("descendant"));
        assert_invalid(
            overlap,
            &document,
            ValidationError::OverlappingNodeReplacements {
                ancestor: branch,
                descendant: text,
            },
        );
    }

    #[test]
    fn rejects_stale_replacement_targets() {
        let mut document = branched_document();
        let branch = document.children(document.root())[0];
        let mut replace = EditBatch::new();
        replace.replace_node(branch, text_draft("replacement"));
        replace.commit(&mut document);

        let mut stale = EditBatch::new();
        stale.replace_node(branch, text_draft("invalid"));
        assert_invalid(stale, &document, ValidationError::InvalidNode(branch));
    }

    #[test]
    fn rejects_edits_and_insertions_inside_replaced_subtrees() {
        let document = branched_document();
        let branch = document.children(document.root())[0];
        let text = document.children(branch)[0];

        let mut text_conflict = EditBatch::new();
        text_conflict.replace_node(branch, text_draft("replacement"));
        text_conflict.replace_text(text, 0..1, "A");
        assert_invalid(
            text_conflict,
            &document,
            ValidationError::EditTargetsReplacedNode {
                replaced: branch,
                edited: text,
            },
        );

        let mut attribute_conflict = EditBatch::new();
        attribute_conflict.replace_node(branch, text_draft("replacement"));
        attribute_conflict.set_attribute(branch, "class", "changed");
        assert_invalid(
            attribute_conflict,
            &document,
            ValidationError::EditTargetsReplacedNode {
                replaced: branch,
                edited: branch,
            },
        );

        let mut source_map_conflict = EditBatch::new();
        source_map_conflict.replace_node(branch, text_draft("replacement"));
        source_map_conflict.set_source_map(text, None);
        assert_invalid(
            source_map_conflict,
            &document,
            ValidationError::EditTargetsReplacedNode {
                replaced: branch,
                edited: text,
            },
        );

        let mut insertion_conflict = EditBatch::new();
        insertion_conflict.replace_node(branch, text_draft("replacement"));
        insertion_conflict.insert_before(text, text_draft("inserted"));
        assert_invalid(
            insertion_conflict,
            &document,
            ValidationError::InsertionTargetsReplacedNode {
                replaced: branch,
                target: text,
            },
        );
    }

    #[test]
    fn rejects_overlapping_removal_and_replacement_in_both_directions() {
        let document = branched_document();
        let branch = document.children(document.root())[0];
        let text = document.children(branch)[0];

        let mut same_target = EditBatch::new();
        same_target.remove_node(branch);
        same_target.replace_node(branch, text_draft("replacement"));
        assert_invalid(
            same_target,
            &document,
            ValidationError::ConflictingNodeRemovalAndReplacement {
                removed: branch,
                replaced: branch,
            },
        );

        let mut remove_ancestor = EditBatch::new();
        remove_ancestor.remove_node(branch);
        remove_ancestor.replace_node(text, text_draft("replacement"));
        assert_invalid(
            remove_ancestor,
            &document,
            ValidationError::ConflictingNodeRemovalAndReplacement {
                removed: branch,
                replaced: text,
            },
        );

        let mut replace_ancestor = EditBatch::new();
        replace_ancestor.replace_node(branch, text_draft("replacement"));
        replace_ancestor.remove_node(text);
        assert_invalid(
            replace_ancestor,
            &document,
            ValidationError::ConflictingNodeRemovalAndReplacement {
                removed: text,
                replaced: branch,
            },
        );
    }

    #[test]
    fn rejects_invalid_wrap_ranges() {
        let document = document(&["a", "b", "c", "d"]);
        let root = document.root();
        let children = document.children(root).to_vec();

        let mut root_endpoint = EditBatch::new();
        root_endpoint.replace_text(children[0], 0..1, "A");
        root_endpoint.wrap_range(root, root, NodeDraft::new(Paragraph));
        assert_invalid(
            root_endpoint,
            &document,
            ValidationError::CannotWrapRoot(root),
        );

        let mut reversed = EditBatch::new();
        reversed.wrap_range(children[2], children[0], NodeDraft::new(Paragraph));
        assert_invalid(
            reversed,
            &document,
            ValidationError::ReversedWrapRange {
                first: children[2],
                last: children[0],
            },
        );

        let mut childful_wrapper = NodeDraft::new(Paragraph);
        childful_wrapper.push_child(text_draft("existing"));
        let mut childful = EditBatch::new();
        childful.wrap_range(children[0], children[1], childful_wrapper);
        assert_invalid(
            childful,
            &document,
            ValidationError::WrapperDraftHasChildren {
                first: children[0],
                last: children[1],
            },
        );

        let mut overlap = EditBatch::new();
        overlap.wrap_range(children[0], children[2], NodeDraft::new(Paragraph));
        overlap.wrap_range(children[2], children[3], NodeDraft::new(Paragraph));
        assert_invalid(
            overlap,
            &document,
            ValidationError::OverlappingWrapRanges {
                first_range: (children[0], children[2]),
                second_range: (children[2], children[3]),
            },
        );
    }

    #[test]
    fn rejects_different_parent_and_stale_wrap_endpoints() {
        let mut document = branched_document();
        let branches = document.children(document.root()).to_vec();
        let first = document.children(branches[0])[0];
        let second = document.children(branches[1])[0];

        let mut different_parents = EditBatch::new();
        different_parents.wrap_range(first, second, NodeDraft::new(Paragraph));
        assert_invalid(
            different_parents,
            &document,
            ValidationError::WrapEndpointsHaveDifferentParents {
                first,
                last: second,
            },
        );

        let mut removal = EditBatch::new();
        removal.remove_node(first);
        removal.commit(&mut document);
        let mut stale = EditBatch::new();
        stale.wrap_range(first, first, NodeDraft::new(Paragraph));
        assert_invalid(stale, &document, ValidationError::InvalidNode(first));
    }

    #[test]
    fn rejects_destructive_and_insertion_conflicts_with_wrap_ranges() {
        let document = branched_document();
        let branch = document.children(document.root())[0];
        let text = document.children(branch)[0];

        let mut removal = EditBatch::new();
        removal.remove_node(branch);
        removal.wrap_range(text, text, NodeDraft::new(Paragraph));
        assert_invalid(
            removal,
            &document,
            ValidationError::WrapRangeTargetsRemovedNode {
                removed: branch,
                first: text,
                last: text,
            },
        );

        let mut replacement = EditBatch::new();
        replacement.replace_node(text, text_draft("replacement"));
        replacement.wrap_range(text, text, NodeDraft::new(Paragraph));
        assert_invalid(
            replacement,
            &document,
            ValidationError::WrapRangeTargetsReplacedNode {
                replaced: text,
                first: text,
                last: text,
            },
        );

        let mut insertion = EditBatch::new();
        insertion.insert_before(text, text_draft("inserted"));
        insertion.wrap_range(text, text, NodeDraft::new(Paragraph));
        assert_invalid(
            insertion,
            &document,
            ValidationError::InsertionTargetsWrappedNode {
                first: text,
                last: text,
                target: text,
            },
        );
    }

    #[test]
    #[should_panic(expected = "invalid edit batch: cannot remove document root")]
    fn commit_panics_on_invalid_batch() {
        let mut document = branched_document();
        let root = document.root();
        let mut batch = EditBatch::new();
        batch.remove_node(root);

        batch.commit(&mut document);
    }

    #[test]
    fn commit_leaves_document_unchanged_when_validation_fails() {
        let mut document = document(&["abc", "雪"]);
        let children = document.children(document.root()).to_vec();
        let (first, second) = (children[0], children[1]);

        let result = catch_unwind(AssertUnwindSafe(|| {
            let mut batch = EditBatch::new();
            batch.replace_text(first, 0..1, "A");
            batch.replace_text(second, 1..2, "invalid UTF-8 boundary");
            batch.commit(&mut document);
        }));

        assert!(result.is_err());
        assert_eq!(content(&document, first), "abc");
        assert_eq!(content(&document, second), "雪");
    }
}
