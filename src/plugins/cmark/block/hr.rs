//! Thematic breaks
//!
//! `***`, `---`, `___`
//!
//! <https://spec.commonmark.org/0.30/#thematic-breaks>
use crate::MarkdownIt;
use crate::document::{NodeId, NodeRef};
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

    // Markers, spaces, and tabs are ASCII, so scan bytes directly.
    let bytes = line.as_bytes();
    let marker = *bytes.first()?;
    if marker != b'*' && marker != b'-' && marker != b'_' {
        return None;
    }

    // Fast path for long marker runs.
    let mut cnt = 1;
    let mut index = 1;
    let marker_word = u64::from_ne_bytes([marker; 8]);
    while index + 8 <= bytes.len() {
        let word = u64::from_ne_bytes(bytes[index..index + 8].try_into().unwrap());
        if word != marker_word {
            break;
        }
        cnt += 8;
        index += 8;
    }

    // Count remaining bytes; spaces and tabs are allowed.
    for &byte in &bytes[index..] {
        if byte == marker {
            cnt += 1;
        } else if byte != b' ' && byte != b'\t' {
            return None;
        }
    }

    if cnt < 3 {
        return None;
    }

    Some((marker as char, cnt))
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

#[cfg(test)]
mod tests {
    use super::scan_thematic_break;

    #[test]
    fn long_marker_run_preserves_count() {
        let line = "*".repeat(32 * 1024);
        assert_eq!(
            scan_thematic_break(&line, 0, 4),
            Some(('*', line.len())),
            "fast path must preserve marker count"
        );
    }

    #[test]
    fn markers_can_be_mixed_with_spaces_and_tabs() {
        assert_eq!(scan_thematic_break("* * *", 0, 4), Some(('*', 3)));
        assert_eq!(scan_thematic_break("---\t ", 0, 4), Some(('-', 3)));
        assert_eq!(scan_thematic_break("___  ___", 0, 4), Some(('_', 6)));
        assert_eq!(scan_thematic_break("**", 0, 4), None);
        assert_eq!(scan_thematic_break("*x*", 0, 4), None);
        assert_eq!(scan_thematic_break("*雪*", 0, 4), None);
    }

    #[test]
    fn indentation_limit_is_applied_first() {
        assert_eq!(scan_thematic_break("***", 4, 4), None);
        assert_eq!(scan_thematic_break("***", 3, 4), Some(('*', 3)));
    }
}
