//! Thematic breaks
//!
//! `***`, `---`, `___`
//!
//! <https://spec.commonmark.org/0.30/#thematic-breaks>
use crate::document::{NodeDraft, NodeRef};
use crate::parser::block::{BlockRule, BlockState, DocumentBlockRule};
use crate::parser::document_parser::DocumentBlockState;
use crate::parser::main::MarkdownIt;
use crate::parser::node::{Node, NodeValue};
use crate::parser::renderer::Renderer;
use crate::render::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    PlainTextBreakDocumentRenderer,
    write_html_self_close,
};

#[derive(Debug)]
pub struct ThematicBreak {
    pub marker: char,
    pub marker_len: usize,
}

struct ThematicBreakDocumentRenderer;

impl DocumentNodeRenderer<ThematicBreak> for ThematicBreakDocumentRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        _: &ThematicBreak,
        context: &mut DocumentRenderContext<'_>,
        output: &mut crate::DocumentWriter,
    ) {
        context.cr(output);
        write_html_self_close(output, "hr", node.attrs(), context.options().xhtml_out);
        context.cr(output);
    }
}

impl NodeValue for ThematicBreak {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        fmt.cr();
        fmt.self_close("hr", &node.attrs);
        fmt.cr();
    }
}

pub fn add(md: &mut MarkdownIt) {
    md.block.add_rule::<HrScanner>();
    md.block.add_document_rule::<HrScanner>();
    md.add_document_renderer::<ThematicBreak, _>("html", ThematicBreakDocumentRenderer);
    md.add_document_renderer::<ThematicBreak, _>("text", PlainTextBreakDocumentRenderer);
}

#[doc(hidden)]
pub struct HrScanner;

fn scan_thematic_break(line: &str, line_indent: i32, max_indent: i32) -> Option<(char, usize)> {
    if line_indent >= max_indent {
        return None;
    }

    let mut chars = line.chars();

    // check hr marker
    let marker = chars.next()?;
    if marker != '*' && marker != '-' && marker != '_' {
        return None;
    }

    // markers can be mixed with spaces, but there should be at least 3 of them
    let mut cnt = 1;
    for ch in chars {
        if ch == marker {
            cnt += 1;
        } else if ch != ' ' && ch != '\t' {
            return None;
        }
    }

    if cnt < 3 {
        return None;
    }

    Some((marker, cnt))
}

impl BlockRule for HrScanner {
    const MARKERS: &'static [char] = &['*', '-', '_'];
    const NAMES: &'static [&'static str] = &["hr"];

    fn run(state: &mut BlockState) -> Option<(Node, usize)> {
        let (marker, marker_len) = scan_thematic_break(
            state.get_line(state.line),
            state.line_indent(state.line),
            state.md.max_indent,
        )?;

        let node = Node::new(ThematicBreak { marker, marker_len });
        Some((node, 1))
    }
}

impl DocumentBlockRule for HrScanner {
    fn run(state: &mut DocumentBlockState<'_>) -> Option<(NodeDraft, usize)> {
        let (marker, marker_len) = scan_thematic_break(
            state.get_line(state.line),
            state.line_indent(state.line),
            state.md.max_indent,
        )?;

        let node = NodeDraft::new(ThematicBreak { marker, marker_len });
        Some((node, 1))
    }
}
