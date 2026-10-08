//! Add source mapping to resulting HTML, looks like this: `<stuff data-sourcepos="1:1-2:3">`.
//! ```rust
//! let md = &mut markdown_it::MarkdownIt::empty();
//! markdown_it::plugins::cmark::add(md);
//! markdown_it::plugins::sourcepos::add(md);
//!
//! let html = md.render("# hello");
//! assert_eq!(html.trim(), r#"<h1 data-sourcepos="1:1-1:7">hello</h1>"#);
//! ```
use crate::common::sourcemap::{SourcePos, SourceWithLineStarts};
use crate::document::edit::EditBatch;
use crate::document::transform::DocumentTransform;
use crate::document::{Document, StructuralEvent};
use crate::parser::main::MarkdownIt;

pub fn add(md: &mut MarkdownIt) {
    md.add_document_transform::<SourcePosDocumentTransform>();
}

const SOURCEPOS_ATTRIBUTE: &str = "data-sourcepos";

fn sourcepos_value(map: SourcePos, mapping: &SourceWithLineStarts) -> String {
    let ((startline, startcol), (endline, endcol)) = map.get_positions(mapping);
    format!("{startline}:{startcol}-{endline}:{endcol}")
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

        for event in document.events(document.root()) {
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
