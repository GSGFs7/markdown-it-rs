//! Document-level inline parsing entry point.

use crate::MarkdownIt;
use crate::common::extset::RootExtSet;
use crate::document::Document;
use crate::parser::pipeline::resolve_pending_inline;

pub(crate) fn parse_document(md: &MarkdownIt, document: &mut Document, root_ext: &RootExtSet) {
    resolve_pending_inline(document, md, md.inline.rules(), root_ext);
}
