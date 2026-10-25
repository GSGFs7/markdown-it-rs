//! Code spans
//!
//! `` `looks like this` ``
//!
//! <https://spec.commonmark.org/0.30/#code-span>
use crate::MarkdownIt;
use crate::document::NodeRef;
use crate::parser::inline::helpers::code_pair;
use crate::render::{
    NodeRenderer,
    RenderContext,
    TransparentRenderer,
    write_html_close,
    write_html_open,
};

#[derive(Debug)]
pub struct CodeInline {
    pub marker: char,
    pub marker_len: usize,
}

struct CodeInlineRenderer;

impl NodeRenderer<CodeInline> for CodeInlineRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        _: &CodeInline,
        context: &mut RenderContext<'_>,
        output: &mut crate::Writer,
    ) {
        write_html_open(output, "code", node.attrs());
        context.render_children(node.id(), output);
        write_html_close(output, "code");
    }
}

pub fn add(md: &mut MarkdownIt) {
    code_pair::add_with::<'`'>(md, |document, len| {
        document.create_node(CodeInline {
            marker: '`',
            marker_len: len,
        })
    });
    md.add_renderer::<CodeInline, _>("html", CodeInlineRenderer);
    md.add_renderer::<CodeInline, _>("text", TransparentRenderer);
}
