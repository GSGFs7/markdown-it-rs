//! Add source mapping to resulting HTML, looks like this: `<stuff data-sourcepos="1:1-2:3">`.
//! ```rust
//! let md = &mut markdown_it::MarkdownIt::empty();
//! markdown_it::plugins::cmark::add(md);
//! markdown_it::plugins::sourcepos::add(md);
//!
//! let html = md.parse("# hello").render();
//! assert_eq!(html.trim(), r#"<h1 data-sourcepos="1:1-1:7">hello</h1>"#);
//! ```
use crate::common::sourcemap::{SourcePos, SourceWithLineStarts};
use crate::parser::block::builtin::BlockParserRule;
use crate::parser::core::{CoreRule, Root};
use crate::parser::document::{Document, StructuralEvent};
use crate::parser::document_edit::EditBatch;
use crate::parser::document_transform::DocumentTransform;
use crate::parser::inline::builtin::InlineParserRule;
use crate::parser::main::MarkdownIt;
use crate::parser::node::{HtmlAttributes, Node};

pub fn add(md: &mut MarkdownIt) {
    md.add_rule::<SyntaxPosRule>()
        .after::<BlockParserRule>()
        .after::<InlineParserRule>();
}

/// Register source-position attributes for an explicit arena-backed document
/// pipeline.
pub fn add_document(md: &mut MarkdownIt) {
    md.add_document_transform::<SourcePosDocumentTransform>();
}

const SOURCEPOS_ATTRIBUTE: &str = "data-sourcepos";

fn sourcepos_value(map: SourcePos, mapping: &SourceWithLineStarts) -> String {
    let ((startline, startcol), (endline, endcol)) = map.get_positions(mapping);
    format!("{startline}:{startcol}-{endline}:{endcol}")
}

fn set_sourcepos_attribute(attrs: &mut HtmlAttributes, value: String) {
    if let Some(index) = attrs
        .iter()
        .position(|attribute| attribute.0 == SOURCEPOS_ATTRIBUTE)
    {
        attrs[index].1 = value;
        let mut kept = false;
        attrs.retain(|attribute| {
            if attribute.0 == SOURCEPOS_ATTRIBUTE {
                let keep = !kept;
                kept = true;
                keep
            } else {
                true
            }
        });
    } else {
        attrs.push((SOURCEPOS_ATTRIBUTE.into(), value));
    }
}

#[doc(hidden)]
pub struct SyntaxPosRule;
impl CoreRule for SyntaxPosRule {
    const NAMES: &'static [&'static str] = &["sourcepos", "source_pos"];

    fn run(root: &mut Node, _: &MarkdownIt) {
        let source = root.cast::<Root>().unwrap().content.as_str();
        let mapping = SourceWithLineStarts::new(source);

        root.walk_mut(|node, _| {
            if let Some(map) = node.srcmap {
                set_sourcepos_attribute(&mut node.attrs, sourcepos_value(map, &mapping));
            }
        });
    }
}

/// Arena-backed source-position transform.
#[derive(Debug, Default)]
pub struct SourcePosDocumentTransform;

impl DocumentTransform for SourcePosDocumentTransform {
    const KEY: &'static str = "sourcepos";
    const ALIASES: &'static [&'static str] = &["source_pos"];

    fn run(&self, document: &Document) -> EditBatch {
        let mapping = SourceWithLineStarts::new(document.source());
        let mut edits = EditBatch::new();

        for event in document.events(document.root()).unwrap() {
            let node = match event {
                StructuralEvent::Enter(node) | StructuralEvent::Leaf(node) => node,
                StructuralEvent::Exit(_) => continue,
            };
            if let Some(map) = node.srcmap() {
                edits.set_attribute(
                    node.id(),
                    SOURCEPOS_ATTRIBUTE,
                    sourcepos_value(map, &mapping),
                );
            }
        }

        edits
    }
}

#[cfg(test)]
mod tests {
    use super::{SOURCEPOS_ATTRIBUTE, SourcePosDocumentTransform, SyntaxPosRule};
    use crate::parser::core::CoreRule;
    use crate::plugins::cmark::block::heading::ATXHeading;
    use crate::{Document, MarkdownIt, Node};

    fn parser() -> MarkdownIt {
        let mut md = MarkdownIt::empty();
        crate::plugins::cmark::add(&mut md);
        md
    }

    fn render_both(source: &str) -> (String, String) {
        let parser = parser();

        let mut legacy = parser.parse(source);
        SyntaxPosRule::run(&mut legacy, &parser);

        let mut transforms = MarkdownIt::empty();
        super::add_document(&mut transforms);
        let mut document = parser.parse_document(source);
        transforms.run_document_transforms(&mut document).unwrap();

        (legacy.render(), document.into_legacy().render())
    }

    #[test]
    fn header_test() {
        // same as doctest, keep in sync!
        // used for code coverage and quicker rust-analyzer hints
        let md = &mut crate::MarkdownIt::empty();
        crate::plugins::cmark::add(md);
        crate::plugins::sourcepos::add(md);

        let html = md.parse("# hello").render();
        assert_eq!(html.trim(), r#"<h1 data-sourcepos="1:1-1:7">hello</h1>"#);
    }

    #[test]
    fn document_matches_legacy_for_source_variants() {
        for source in [
            "# hello",
            "# héllo 世界\n\nparagraph with *emphasis*\n",
            "first\r\n=====\r\n\r\n- one\r\n- two\r\n",
            "> quote\n> continued\n\n```rust\nlet x = 1;\n```\n",
        ] {
            let (legacy, document) = render_both(source);
            assert_eq!(document, legacy, "{source:?}");
        }
    }

    #[test]
    fn existing_attribute_is_replaced_and_deduplicated() {
        let parser = parser();
        let source = "# hello";

        let mut legacy = parser.parse(source);
        add_stale_sourcepos(&mut legacy);
        SyntaxPosRule::run(&mut legacy, &parser);

        let mut document_root = parser.parse(source);
        add_stale_sourcepos(&mut document_root);
        let mut document = Document::from_legacy(source, document_root);
        let mut transforms = MarkdownIt::empty();
        super::add_document(&mut transforms);
        transforms.run_document_transforms(&mut document).unwrap();

        let legacy_html = legacy.render();
        assert_eq!(document.into_legacy().render(), legacy_html);
        assert_eq!(legacy_html.matches(SOURCEPOS_ATTRIBUTE).count(), 1);
        assert!(legacy_html.contains(r#"class="before" data-sourcepos="1:1-1:7" title="after""#));
    }

    #[test]
    fn nodes_without_source_maps_are_unchanged() {
        let parser = parser();
        let source = "# hello";

        let mut legacy = parser.parse(source);
        remove_heading_source_map(&mut legacy);
        SyntaxPosRule::run(&mut legacy, &parser);

        let mut document_root = parser.parse(source);
        remove_heading_source_map(&mut document_root);
        let mut document = Document::from_legacy(source, document_root);
        let mut transforms = MarkdownIt::empty();
        super::add_document(&mut transforms);
        transforms.run_document_transforms(&mut document).unwrap();

        let legacy_html = legacy.render();
        assert_eq!(document.into_legacy().render(), legacy_html);
        assert_eq!(legacy_html, "<h1>hello</h1>\n");
    }

    #[test]
    fn document_runner_is_explicit() {
        let parser = parser();
        let mut transforms = MarkdownIt::empty();
        super::add_document(&mut transforms);

        let document = parser.parse_document("# hello");
        assert_eq!(document.into_legacy().render(), "<h1>hello</h1>\n");

        let mut document = parser.parse_document("# hello");
        transforms.run_document_transforms(&mut document).unwrap();
        assert!(
            document
                .into_legacy()
                .render()
                .contains(r#"data-sourcepos="1:1-1:7""#)
        );
    }

    #[test]
    fn legacy_registration_does_not_register_document_transform() {
        let mut md = parser();
        super::add(&mut md);
        assert!(
            !md.document_transforms
                .contains::<SourcePosDocumentTransform>()
        );
        let mut document = md.parse_document("# hello");

        md.run_document_transforms(&mut document).unwrap();

        assert_eq!(
            document
                .into_legacy()
                .render()
                .matches(SOURCEPOS_ATTRIBUTE)
                .count(),
            1
        );
    }

    fn add_stale_sourcepos(root: &mut Node) {
        root.walk_mut(|node, _| {
            if node.is::<ATXHeading>() {
                node.attrs.push(("class".into(), "before".into()));
                node.attrs
                    .push((SOURCEPOS_ATTRIBUTE.into(), "stale-one".into()));
                node.attrs.push(("title".into(), "after".into()));
                node.attrs
                    .push((SOURCEPOS_ATTRIBUTE.into(), "stale-two".into()));
            }
        });
    }

    fn remove_heading_source_map(root: &mut Node) {
        root.walk_mut(|node, _| {
            if node.is::<ATXHeading>() {
                node.srcmap = None;
            }
        });
    }
}
