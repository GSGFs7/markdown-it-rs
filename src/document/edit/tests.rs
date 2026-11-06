use super::*;
#[cfg(feature = "sourcemap")]
use crate::common::sourcemap::SourcePos;
use crate::document::{Root, Text};
use crate::plugins::cmark::block::paragraph::Paragraph;

fn document(texts: &[&str]) -> Document {
    let mut document = Document::new(texts.join(""), Root::new(texts.join("")));
    for content in texts {
        let text = document.create_node(Text {
            content: (*content).to_owned(),
        });
        document.push_child(document.root(), text);
    }
    document
}

fn content(document: &Document, node: NodeId) -> &str {
    &document.node(node).cast::<Text>().unwrap().content
}

#[cfg(debug_assertions)]
fn branched_document() -> Document {
    let mut document = Document::new("ab", Root::new("ab"));
    for content in ["a", "b"] {
        let paragraph = document.create_node(Paragraph);
        document.push_child(document.root(), paragraph);
        let text = document.create_node(Text {
            content: content.to_owned(),
        });
        document.push_child(paragraph, text);
    }
    document
}

#[test]
fn payload_replacement_preserves_identity_children_and_metadata() {
    #[derive(Debug, PartialEq)]
    struct Marker(u8);
    let mut document = Document::new("abc", Root::new("abc"));
    let paragraph = document.create_node(Paragraph);
    #[cfg(feature = "sourcemap")]
    {
        document.node_mut(paragraph).data.srcmap = Some(SourcePos::new(0, 3));
    }
    document
        .node_mut(paragraph)
        .data
        .attrs
        .push(("id".into(), "keep".into()));
    document.node_mut(paragraph).data.ext.insert(Marker(42));
    let text = document.create_node(Text {
        content: "abc".into(),
    });
    document.push_child(paragraph, text);
    document.push_child(document.root(), paragraph);
    let child = document.children(paragraph)[0];
    let mut batch = EditBatch::new();
    batch.replace_value(
        paragraph,
        Text {
            content: "replacement".into(),
        },
    );
    batch.set_attribute(paragraph, "title", "new");
    assert_eq!(batch.len(), 2);
    batch.commit(&mut document);
    assert_eq!(content(&document, paragraph), "replacement");
    assert_eq!(document.node(paragraph).parent(), Some(document.root()));
    assert_eq!(document.children(paragraph), &[child]);
    assert_eq!(document.node(child).parent(), Some(paragraph));
    #[cfg(feature = "sourcemap")]
    assert_eq!(
        document.node(paragraph).srcmap(),
        Some(SourcePos::new(0, 3))
    );
    assert_eq!(
        document.node(paragraph).ext().get::<Marker>(),
        Some(&Marker(42))
    );
    assert_eq!(
        document.node(paragraph).attrs(),
        &vec![("id".into(), "keep".into()), ("title".into(), "new".into())]
    );
}

#[test]
fn text_edits_run_before_payload_replacement() {
    let mut document = document(&["abc"]);
    let text = document.children(document.root())[0];
    let mut batch = EditBatch::new();
    batch.replace_value(text, Paragraph);
    batch.replace_text(text, 0..1, "A");
    batch.commit(&mut document);
    assert!(document.node(text).is::<Paragraph>());
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
    let mut document = Document::new("text", Root::new("text"));
    let text = document.create_node(Text {
        content: "text".to_owned(),
    });
    document.node_mut(text).data.attrs = vec![
        ("class".to_owned(), "old".to_owned()),
        ("id".to_owned(), "remove-me".to_owned()),
        ("class".to_owned(), "duplicate".to_owned()),
        ("data-key".to_owned(), "kept".to_owned()),
    ];
    document.push_child(document.root(), text);
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
#[cfg(feature = "sourcemap")]
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

#[cfg(debug_assertions)]
mod validation_tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use super::*;
    use crate::document::edit::validation::ValidationError;

    #[test]
    fn payload_replacements_validate_duplicates_and_stale_ids() {
        let mut document = branched_document();
        let paragraph = document.children(document.root())[0];
        let text = document.children(paragraph)[0];
        let mut duplicate = EditBatch::new();
        duplicate.replace_value(text, Paragraph);
        duplicate.replace_value(text, Paragraph);
        assert_invalid(
            duplicate,
            &document,
            ValidationError::DuplicateValueReplacement(text),
        );
        document.remove_node(text);
        let mut stale = EditBatch::new();
        stale.replace_value(text, Paragraph);
        assert_invalid(stale, &document, ValidationError::InvalidNode(text));
    }

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

        let paragraph = Document::new("", Paragraph);
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
    #[cfg(feature = "sourcemap")]
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
    #[cfg(feature = "sourcemap")]
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
    fn rejects_stale_edit_targets_without_mutating_document() {
        let mut document = document(&["abc", "def"]);
        let children = document.children(document.root()).to_vec();
        let (stale, live) = (children[0], children[1]);
        document.remove_node(stale);

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
