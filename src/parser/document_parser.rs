//! Transitional direct-to-arena parser support.

use std::fmt;
use std::sync::Arc;

use crate::common::sourcemap::SourcePos;
use crate::parser::block::build_line_offsets;
use crate::parser::core::Root;
use crate::parser::document::{Document, NodeDraft};
use crate::parser::inline::Text;
use crate::parser::render_options::RenderOptions;

/// Error returned while a parser configuration still contains rules that
/// have not been migrated to the direct arena pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentParseError {
    UnsupportedConfiguration,
}

impl fmt::Display for DocumentParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedConfiguration => f.write_str(
                "parser configuration contains rules without direct arena implementations",
            ),
        }
    }
}

impl std::error::Error for DocumentParseError {}

pub(crate) struct DocumentParseContext<'a> {
    source: &'a str,
    root: NodeDraft,
}

impl<'a> DocumentParseContext<'a> {
    pub(crate) fn new(source: &'a str, options: &RenderOptions) -> Self {
        let mut root = NodeDraft::new(Root::new(source.to_owned()));
        root.set_srcmap(Some(SourcePos::new(0, source.len())));
        root.ext_mut().insert(options.clone());
        Self { source, root }
    }

    pub(crate) fn parse_text_fallback(mut self) -> Document {
        for line in build_line_offsets(self.source) {
            if line.first_nonspace >= line.line_end {
                continue;
            }

            let mut content = self.source[line.first_nonspace..line.line_end].to_owned();
            content.push('\n');
            let mut text = NodeDraft::new(Text { content });
            text.set_srcmap(Some(SourcePos::new(line.first_nonspace, line.line_end + 1)));
            self.root.push_child(text);
        }

        Document::from_draft(Arc::<str>::from(self.source), self.root)
    }
}
