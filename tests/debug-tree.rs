use markdown_it::MarkdownIt;
#[cfg(feature = "sourcemap")]
use markdown_it::common::sourcemap::SourcePos;
use markdown_it::document::{Document, Root};
use markdown_it::plugins::presets::Preset;

#[derive(Debug)]
struct CustomContainer;

#[derive(Debug)]
struct CustomLeaf;

#[test]
#[cfg(feature = "sourcemap")]
fn debug_format_records_hierarchy_identity_source_and_attributes() {
    let mut document = Document::new("abcdef", Root::new("abcdef"));
    let root = document.root();
    document
        .node_mut(root)
        .set_srcmap(Some(SourcePos::new(0, 6)));

    let container = document.create_node(CustomContainer);
    document
        .node_mut(container)
        .set_srcmap(Some(SourcePos::new(1, 5)));
    document
        .node_mut(container)
        .attrs_mut()
        .push(("class".into(), "outer".into()));

    let leaf = document.create_node(CustomLeaf);
    document
        .node_mut(leaf)
        .attrs_mut()
        .push(("data-value".into(), "<&>".into()));
    document.push_child(container, leaf);
    document.push_child(root, container);

    let output = MarkdownIt::empty().render_document(&document, "debug");

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
        let output = md.render_document(&document, "debug");

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
    let mut document = Document::new("", Root::new(""));
    let root = document.root();
    let container = document.create_node(CustomContainer);
    let leaf = document.create_node(CustomLeaf);
    document
        .node_mut(leaf)
        .attrs_mut()
        .push(("id".into(), "generated".into()));
    document.push_child(container, leaf);
    document.push_child(root, container);
    let debug = MarkdownIt::empty().render_document(&document, "debug");
    assert_eq!(debug.lines().count(), 3);
    assert!(debug.contains(std::any::type_name::<CustomContainer>()));
    assert!(debug.contains(std::any::type_name::<CustomLeaf>()));
    assert!(debug.contains("generated"));
    assert!(!debug.contains("srcmap="));
}
