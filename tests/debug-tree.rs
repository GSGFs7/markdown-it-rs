#[cfg(feature = "sourcemap")]
use markdown_it::common::sourcemap::SourcePos;
use markdown_it::{Document, MarkdownIt, NodeDraft, Preset, Root};

#[derive(Debug)]
struct CustomContainer;

#[derive(Debug)]
struct CustomLeaf;

#[test]
#[cfg(feature = "sourcemap")]
fn debug_format_records_hierarchy_identity_source_and_attributes() {
    let mut root = NodeDraft::new(Root::new("abcdef"));
    root.set_srcmap(Some(SourcePos::new(0, 6)));

    let mut container = NodeDraft::new(CustomContainer);
    container.set_srcmap(Some(SourcePos::new(1, 5)));
    container.attrs_mut().push(("class".into(), "outer".into()));

    let mut leaf = NodeDraft::new(CustomLeaf);
    leaf.attrs_mut().push(("data-value".into(), "<&>".into()));
    container.push_child(leaf);
    root.push_child(container);

    let document = Document::from_draft("abcdef", root);
    let output = MarkdownIt::empty().render_as(&document, "debug");

    let expected = format!(
        "container type={} id=NodeId(0:0) srcmap=0..6 attrs=[]\n  container type={} id=NodeId(1:0) srcmap=1..5 attrs=[(\"class\", \"outer\")]\n    leaf type={} id=NodeId(2:0) srcmap=- attrs=[(\"data-value\", \"<&>\")]\n",
        std::any::type_name::<Root>(),
        std::any::type_name::<CustomContainer>(),
        std::any::type_name::<CustomLeaf>(),
    );
    assert_eq!(output, expected);
}

#[test]
fn debug_format_covers_standard_presets_without_syntax_registrations() {
    for md in [
        MarkdownIt::new(),
        MarkdownIt::with_preset(Preset::CommonMark),
    ] {
        let document = md.parse("# heading *em*\n");
        let output = md.render_as(&document, "debug");

        assert!(output.contains(&format!("type={}", std::any::type_name::<Root>())));
        assert!(output.contains("type=markdown_it::plugins::cmark::block::heading::ATXHeading"));
        assert!(output.contains("type=markdown_it::plugins::cmark::inline::emphasis::Em"));
        assert!(output.lines().all(|line| line.contains(" id=NodeId(")));
        #[cfg(feature = "sourcemap")]
        assert!(output.lines().all(|line| line.contains(" srcmap=")));
        #[cfg(not(feature = "sourcemap"))]
        assert!(!output.contains(" srcmap="));
        assert!(output.lines().all(|line| line.contains(" attrs=")));
    }
}

#[cfg(not(feature = "sourcemap"))]
#[test]
fn debug_format_without_source_maps_preserves_generated_nodes() {
    let mut root = NodeDraft::new(Root::new(""));
    let mut container = NodeDraft::new(CustomContainer);
    let mut leaf = NodeDraft::new(CustomLeaf);
    leaf.attrs_mut().push(("id".into(), "generated".into()));
    container.push_child(leaf);
    root.push_child(container);
    let document = Document::from_draft("", root);
    let debug = MarkdownIt::empty().render_as(&document, "debug");
    assert_eq!(debug.lines().count(), 3);
    assert!(debug.contains(std::any::type_name::<CustomContainer>()));
    assert!(debug.contains(std::any::type_name::<CustomLeaf>()));
    assert!(debug.contains("generated"));
    assert!(!debug.contains("srcmap="));
}
