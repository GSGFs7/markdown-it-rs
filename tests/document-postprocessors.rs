use markdown_it::parser::core::Root;
use markdown_it::parser::inline::Text;
use markdown_it::plugins::{cmark, extra, sourcepos};
use markdown_it::{Document, MarkdownIt, NodeDraft, StructuralEvent};

#[test]
fn typographer_precedes_custom_smartquotes_in_reverse_registration_order() {
    let mut md = MarkdownIt::new();
    extra::smartquotes::add_with::<'‹', '›', '«', '»'>(&mut md);
    extra::typographer::add(&mut md);
    assert_eq!(md.render("\"...\" '雪' (TM)"), "<p>«…» ‹雪› ™</p>\n");
}

#[test]
fn sourcepos_runs_after_tasklist_in_reverse_registration_order() {
    let mut md = MarkdownIt::new();
    sourcepos::add(&mut md);
    extra::tasklist::add(&mut md);
    md.render_options.xhtml_out = true;
    let doc = md.parse_document("- [x] 雪");
    let text = doc
        .events(doc.root())
        .find_map(|event| {
            if let StructuralEvent::Leaf(node) = event {
                node.cast::<Text>()
                    .filter(|text| text.content == "雪")
                    .map(|_| node)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(text.srcmap().unwrap().get_byte_offsets(), (6, 9));
    let html = md.render_document(&doc);
    assert!(
        html.contains("type=\"checkbox\" checked=\"\" /> 雪"),
        "{html}"
    );
    assert!(html.contains("data-sourcepos=\"1:1-1:7\""), "{html}");
    assert_eq!(md.render_document_as(&doc, "text"), "雪\n");
}

#[test]
fn sourcepos_deduplicates_existing_attributes_and_skips_generated_nodes() {
    let mut md = MarkdownIt::new();
    sourcepos::add(&mut md);
    let source = "# 雪";
    let mut root = NodeDraft::new(Root::new(source.into()));
    let mut heading = NodeDraft::new(cmark::block::heading::ATXHeading { level: 1 });
    heading.set_srcmap(Some(markdown_it::common::sourcemap::SourcePos::new(
        0,
        source.len(),
    )));
    heading.attrs_mut().extend([
        ("data-sourcepos".into(), "stale".into()),
        ("data-sourcepos".into(), "duplicate".into()),
    ]);
    heading.push_child(NodeDraft::new(Text {
        content: "雪".into(),
    }));
    root.push_child(heading);
    let mut doc = Document::from_draft(source, root);
    md.run_document_transforms(&mut doc);
    let heading = doc.node(doc.children(doc.root())[0]);
    assert_eq!(
        heading.attrs(),
        &[("data-sourcepos".into(), "1:1-1:3".into())]
    );
    let generated = doc.node(heading.children()[0]);
    assert!(generated.srcmap().is_none());
    assert!(generated.attrs().is_empty());
}

#[test]
fn text_fallback_runs_typographer_and_custom_smartquotes() {
    let mut md = MarkdownIt::empty();
    extra::smartquotes::add_with::<'‹', '›', '«', '»'>(&mut md);
    extra::typographer::add(&mut md);
    assert_eq!(md.render("\"雪...\""), "«雪…»\n");
    assert_eq!(md.render(""), "");
}

#[test]
fn draft_drop_and_arena_insertion_handle_deep_generated_trees() {
    fn tree() -> NodeDraft {
        let mut node = NodeDraft::new(Text {
            content: "leaf".into(),
        });
        for _ in 0..50_000 {
            let mut parent = NodeDraft::new(cmark::block::paragraph::Paragraph);
            parent.push_child(node);
            node = parent;
        }
        node
    }
    drop(tree());
    let doc = Document::from_draft("", tree());
    assert_eq!(doc.len(), 50_001);
    assert_eq!(
        doc.events(doc.root())
            .filter(|event| matches!(event, StructuralEvent::Leaf(_)))
            .count(),
        1
    );
}

#[test]
fn appended_subtrees_keep_parent_links_and_existing_node_ids() {
    let md = MarkdownIt::new();
    let mut doc = md.parse_document("first");
    let root = doc.root();
    let first = doc.children(root)[0];
    let appended = doc.append_child(
        root,
        NodeDraft::new(Text {
            content: "second".into(),
        }),
    );
    assert_eq!(doc.children(root), &[first, appended]);
    assert_eq!(doc.parent(appended), Some(root));
    assert_eq!(md.render_document(&doc), "<p>first</p>\nsecond");
}

#[test]
fn parser_and_document_are_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<MarkdownIt>();
    assert_send_sync::<Document>();
}
