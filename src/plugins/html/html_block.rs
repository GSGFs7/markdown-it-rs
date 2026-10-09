//! HTML block syntax from CommonMark
//!
//! <https://spec.commonmark.org/0.30/#html-blocks>
use std::sync::LazyLock;

use regex::Regex;

use super::utils::blocks::*;
use super::utils::regexps::*;
use crate::MarkdownIt;
use crate::document::{NodeId, NodeRef, NodeValue};
use crate::parser::block::{BlockRule, DocumentBlockState};
use crate::render::{DocumentNodeRenderer, DocumentRenderContext};

#[derive(Debug)]
pub struct HtmlBlock {
    pub content: String,
}

struct HtmlBlockDocumentRenderer;

impl DocumentNodeRenderer<HtmlBlock> for HtmlBlockDocumentRenderer {
    fn render(
        &self,
        _: NodeRef<'_>,
        value: &HtmlBlock,
        context: &mut DocumentRenderContext<'_>,
        output: &mut crate::DocumentWriter,
    ) {
        context.cr(output);
        output.write_str(&value.content);
        context.cr(output);
    }
}

struct HtmlBlockTextRenderer;

impl DocumentNodeRenderer<HtmlBlock> for HtmlBlockTextRenderer {
    fn render(
        &self,
        _: NodeRef<'_>,
        value: &HtmlBlock,
        context: &mut DocumentRenderContext<'_>,
        output: &mut crate::DocumentWriter,
    ) {
        context.cr(output);
        output.write_str(&value.content);
        context.cr(output);
    }
}

impl NodeValue for HtmlBlock {}

pub fn add(md: &mut MarkdownIt) {
    md.block.add_rule::<HtmlBlockScanner>();
    md.add_document_renderer::<HtmlBlock, _>("html", HtmlBlockDocumentRenderer);
    md.add_document_renderer::<HtmlBlock, _>("text", HtmlBlockTextRenderer);
}

struct HTMLSequence {
    open: Regex,
    close: Regex,
    can_terminate_paragraph: bool,
}

impl HTMLSequence {
    pub fn new(open: Regex, close: Regex, can_terminate_paragraph: bool) -> Self {
        Self {
            open,
            close,
            can_terminate_paragraph,
        }
    }
}

// An array of opening and corresponding closing sequences for html tags,
// last argument defines whether it can terminate a paragraph or not
//
static HTML_SEQUENCES: LazyLock<[HTMLSequence; 7]> = LazyLock::new(|| {
    let block_names = HTML_BLOCKS.join("|");
    let open_close_tag_re = HTML_OPEN_CLOSE_TAG_RE.as_str();

    [
        HTMLSequence::new(
            Regex::new(r#"(?i)^<(script|pre|style|textarea)(\s|>|$)"#).unwrap(),
            Regex::new(r#"(?i)</(script|pre|style|textarea)>"#).unwrap(),
            true,
        ),
        HTMLSequence::new(
            Regex::new(r#"^<!--"#).unwrap(),
            Regex::new(r#"-->"#).unwrap(),
            true,
        ),
        HTMLSequence::new(
            Regex::new(r#"^<\?"#).unwrap(),
            Regex::new(r#"\?>"#).unwrap(),
            true,
        ),
        HTMLSequence::new(
            Regex::new(r#"^<![A-Za-z]"#).unwrap(),
            Regex::new(r#">"#).unwrap(),
            true,
        ),
        HTMLSequence::new(
            Regex::new(r#"^<!\[CDATA\["#).unwrap(),
            Regex::new(r#"\]\]>"#).unwrap(),
            true,
        ),
        HTMLSequence::new(
            Regex::new(&format!("(?i)^</?({block_names})(\\s|/?>|$)")).unwrap(),
            Regex::new(r#"^$"#).unwrap(),
            true,
        ),
        HTMLSequence::new(
            Regex::new(&format!("{open_close_tag_re}\\s*$")).unwrap(),
            Regex::new(r#"^$"#).unwrap(),
            false,
        ),
    ]
});

#[doc(hidden)]
pub struct HtmlBlockScanner;

impl HtmlBlockScanner {
    fn get_sequence(
        line_text: &str,
        indent: i32,
        max_indent: i32,
    ) -> Option<&'static HTMLSequence> {
        if indent >= max_indent || !line_text.starts_with('<') {
            return None;
        }

        HTML_SEQUENCES
            .iter()
            .find(|seq| seq.open.is_match(line_text))
    }

    fn end_line<'a>(
        sequence: &HTMLSequence,
        start_line: usize,
        line_max: usize,
        get_line: impl Fn(usize) -> (&'a str, i32),
    ) -> usize {
        let mut next_line = start_line + 1;

        // If we are here - we detected HTML block.
        // Let's roll down till block end.
        if !sequence.close.is_match(get_line(start_line).0) {
            while next_line < line_max {
                let (line_text, indent) = get_line(next_line);

                // Blank lines may occur inside explicitly terminated HTML
                // blocks. A non-empty negative-indent line, however, has left
                // the current list or blockquote container.
                if indent < 0 && !line_text.is_empty() {
                    break;
                }

                if sequence.close.is_match(line_text) {
                    if !line_text.is_empty() {
                        next_line += 1;
                    }
                    break;
                }

                next_line += 1;
            }
        }
        next_line
    }
}

impl BlockRule for HtmlBlockScanner {
    const MARKERS: &'static [char] = &['<'];
    const NAMES: &'static [&'static str] = &["html_block"];
    fn check(state: &mut DocumentBlockState<'_>) -> Option<()> {
        let sequence = Self::get_sequence(
            state.get_line(state.line),
            state.line_indent(state.line),
            state.md.max_indent,
        )?;
        sequence.can_terminate_paragraph.then_some(())
    }

    fn run(state: &mut DocumentBlockState<'_>) -> Option<(Option<NodeId>, usize)> {
        let sequence = Self::get_sequence(
            state.get_line(state.line),
            state.line_indent(state.line),
            state.md.max_indent,
        )?;
        let start_line = state.line;
        let next_line = Self::end_line(sequence, start_line, state.line_max, |line| {
            (state.get_line(line), state.line_indent(line))
        });

        let (content, _) = state.get_lines(start_line, next_line, state.blk_indent, true);
        // The block tokenizer assigns the source map for the consumed lines.
        Some((
            Some(state.document.create_node(HtmlBlock { content })),
            next_line - start_line,
        ))
    }
}
