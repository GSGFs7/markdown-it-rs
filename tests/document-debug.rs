use markdown_it::common::sourcemap::SourcePos;
use markdown_it::parser::core::Root;
use markdown_it::{Document, MarkdownIt, Node, NodeValue, Preset};

#[derive(Debug)]
struct CustomContainer;
impl NodeValue for CustomContainer {}

#[derive(Debug)]
struct CustomLeaf;
impl NodeValue for CustomLeaf {}

#[test]
fn debug_format_records_hierarchy_identity_source_and_attributes() {
    let mut root = Node::new(Root::new("abcdef".into()));
    root.srcmap = Some(SourcePos::new(0, 6));

    let mut container = Node::new(CustomContainer);
    container.srcmap = Some(SourcePos::new(1, 5));
    container.attrs.push(("class".into(), "outer".into()));

    let mut leaf = Node::new(CustomLeaf);
    leaf.attrs.push(("data-value".into(), "<&>".into()));
    container.children.push(leaf);
    root.children.push(container);

    let document = Document::from_legacy("abcdef", root);
    let output = MarkdownIt::empty()
        .render_document_as(&document, "debug")
        .unwrap();

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
        let document = md.parse_document("# heading *em*\n");
        let output = md.render_document_as(&document, "debug").unwrap();

        assert!(output.contains("type=markdown_it::parser::core::root::Root"));
        assert!(output.contains("type=markdown_it::plugins::cmark::block::heading::ATXHeading"));
        assert!(output.contains("type=markdown_it::plugins::cmark::inline::emphasis::Em"));
        assert!(output.lines().all(|line| line.contains(" id=NodeId(")));
        assert!(output.lines().all(|line| line.contains(" srcmap=")));
        assert!(output.lines().all(|line| line.contains(" attrs=")));
    }
}
