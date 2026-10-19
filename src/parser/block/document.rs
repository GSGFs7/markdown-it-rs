//! Document-level block parsing entry point.

use super::{BlockState, build_line_offsets};
use crate::MarkdownIt;
use crate::common::extset::RootExtSet;
use crate::document::{Document, Text};

pub(crate) fn parse_document(
    source: &str,
    md: &MarkdownIt,
    document: &mut Document,
    root_ext: &mut RootExtSet,
) {
    if md.block.rules().is_empty() && md.inline.has_only_text_rule() && md.max_nesting > 0 {
        parse_text_fallback(source, document);
        return;
    }

    let mut state = BlockState::new(source, md, md.block.rules(), document);
    state.root_ext = std::mem::take(root_ext);
    state.tokenize();
    *root_ext = state.root_ext;
}

fn parse_text_fallback(source: &str, document: &mut Document) {
    for line in build_line_offsets(source) {
        if line.first_nonspace >= line.line_end {
            continue;
        }

        let mut content = source[line.first_nonspace..line.line_end].to_owned();
        content.push('\n');
        let text = document.create_node(Text { content });
        #[cfg(feature = "sourcemap")]
        {
            let source_end = (line.line_end + 1).min(source.len());
            document
                .node_mut(text)
                .set_srcmap(Some(crate::common::sourcemap::SourcePos::new(
                    line.first_nonspace,
                    source_end,
                )));
        }
        document.push_child(document.root(), text);
    }
}
