//! Thematic breaks
//!
//! `***`, `---`, `___`
//!
//! <https://spec.commonmark.org/0.30/#thematic-breaks>
use crate::MarkdownIt;
use crate::document::{NodeId, NodeRef, NodeValue};
use crate::parser::block::{BlockRule, BlockState};
use crate::render::{NodeRenderer, PlainTextBreakRenderer, RenderContext, write_html_self_close};

#[derive(Debug)]
pub struct ThematicBreak {
    pub marker: char,
    pub marker_len: usize,
}

struct ThematicBreakRenderer;

impl NodeRenderer<ThematicBreak> for ThematicBreakRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        _: &ThematicBreak,
        context: &mut RenderContext<'_>,
        output: &mut crate::Writer,
    ) {
        context.cr(output);
        write_html_self_close(output, "hr", node.attrs(), context.options().xhtml_out);
        context.cr(output);
    }
}

impl NodeValue for ThematicBreak {}

pub fn add(md: &mut MarkdownIt) {
    md.block.add_rule::<HrScanner>();
    md.add_renderer::<ThematicBreak, _>("html", ThematicBreakRenderer);
    md.add_renderer::<ThematicBreak, _>("text", PlainTextBreakRenderer);
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

    fn run(state: &mut BlockState<'_>) -> Option<(Option<NodeId>, usize)> {
        let (marker, marker_len) = scan_thematic_break(
            state.get_line(state.line),
            state.line_indent(state.line),
            state.md.max_indent,
        )?;

        let node = state
            .document
            .create_node(ThematicBreak { marker, marker_len });
        Some((Some(node), 1))
    }
}
