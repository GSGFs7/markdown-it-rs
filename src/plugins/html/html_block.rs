//! HTML block syntax from CommonMark
//!
//! <https://spec.commonmark.org/0.30/#html-blocks>
use std::sync::LazyLock;

use regex::Regex;

use super::utils::blocks::*;
use super::utils::regexps::*;
use crate::MarkdownIt;
use crate::document::{NodeId, NodeRef};
use crate::parser::block::{BlockRule, BlockState};
use crate::render::{NodeRenderer, RenderContext};

#[derive(Debug)]
pub struct HtmlBlock {
    pub content: String,
}

struct HtmlBlockRenderer;

impl NodeRenderer<HtmlBlock> for HtmlBlockRenderer {
    fn render(
        &self,
        _: NodeRef<'_>,
        value: &HtmlBlock,
        _: &mut RenderContext<'_>,
        output: &mut crate::Writer,
    ) {
        output.write_str(&value.content);
    }
}

struct HtmlBlockTextRenderer;

impl NodeRenderer<HtmlBlock> for HtmlBlockTextRenderer {
    fn render(
        &self,
        _: NodeRef<'_>,
        value: &HtmlBlock,
        context: &mut RenderContext<'_>,
        output: &mut crate::Writer,
    ) {
        context.cr(output);
        output.write_str(&value.content);
        context.cr(output);
    }
}

pub fn add(md: &mut MarkdownIt) {
    md.block.add_rule::<HtmlBlockScanner>();
    md.add_renderer::<HtmlBlock, _>("html", HtmlBlockRenderer);
    md.add_renderer::<HtmlBlock, _>("text", HtmlBlockTextRenderer);
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
            Regex::new(&format!(
                "(?i)^<(script|pre|style|textarea)({HTML_WHITESPACE}|>|$)"
            ))
            .unwrap(),
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
            Regex::new(&format!("(?i)^</?({block_names})({HTML_WHITESPACE}|/?>|$)")).unwrap(),
            Regex::new(r#"^$"#).unwrap(),
            true,
        ),
        HTMLSequence::new(
            Regex::new(&format!("{open_close_tag_re}{HTML_WHITESPACE}*$")).unwrap(),
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
    fn check(state: &mut BlockState<'_>) -> Option<()> {
        let sequence = Self::get_sequence(
            state.get_line(state.line),
            state.line_indent(state.line),
            state.md.max_indent,
        )?;
        sequence.can_terminate_paragraph.then_some(())
    }

    fn run(state: &mut BlockState<'_>) -> Option<(Option<NodeId>, usize)> {
        let sequence = Self::get_sequence(
            state.get_line(state.line),
            state.line_indent(state.line),
            state.md.max_indent,
        )?;
        let start_line = state.line;
        let next_line = Self::end_line(sequence, start_line, state.line_max, |line| {
            (state.get_line(line), state.line_indent(line))
        });

        // Preserve a final line break only when it exists in the source.
        let keep_last_lf = state.line_offsets[next_line - 1].line_end < state.src.len();
        let (content, _) = state.get_lines(start_line, next_line, state.blk_indent, keep_last_lf);
        // The block tokenizer assigns the source map for the consumed lines.
        Some((
            Some(state.document.create_node(HtmlBlock { content })),
            next_line - start_line,
        ))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn html_blocks_after_tight_list_text_do_not_insert_a_line_break() {
        let mut md = crate::MarkdownIt::empty();
        crate::plugins::cmark::add(&mut md);
        crate::plugins::html::add(&mut md);

        for max_nesting in [3, 100] {
            md.max_nesting = max_nesting;
            for (source, expected) in [
                ("1. a\n    <!--", "<ol>\n<li>a <!--</li>\n</ol>\n"),
                ("1. a\n    <!--\n", "<ol>\n<li>a <!--\n</li>\n</ol>\n"),
                (
                    "- a\n  <!-- comment -->",
                    "<ul>\n<li>a<!-- comment --></li>\n</ul>\n",
                ),
                ("1. a\n    <div>", "<ol>\n<li>a <div></li>\n</ol>\n"),
                ("1. a\n    <?x?>", "<ol>\n<li>a <?x?></li>\n</ol>\n"),
                (
                    "- <div>\n- foo\n",
                    "<ul>\n<li>\n<div>\n</li>\n<li>foo</li>\n</ul>\n",
                ),
                (
                    "1. a\n\n    <!--",
                    "<ol>\n<li>\n<p>a</p>\n <!--</li>\n</ol>\n",
                ),
                (
                    "1. i&9文\t\n    <!--\n!c😀;    a\n\n    b\n    -->\n    c\n",
                    "<ol>\n<li>i&amp;9文 <!--\n</li>\n</ol>\n<p>!c😀;    a</p>\n<pre><code>b\n--&gt;\nc\n</code></pre>\n",
                ),
            ] {
                assert_eq!(
                    md.render(source),
                    expected,
                    "source={source:?}, max_nesting={max_nesting}"
                );
            }
        }
    }

    #[test]
    fn preserves_html_block_final_line_break() {
        let mut md = crate::MarkdownIt::empty();
        crate::plugins::cmark::add(&mut md);
        crate::plugins::html::add(&mut md);

        for source in [
            "<div>",
            "<div>\n",
            "<script>\ntext\n</script>",
            "<script>\ntext\n</script>\n",
            "<!-- comment -->",
            "<!-- comment -->\n",
            "<?instruction?>",
            "<!DOCTYPE html>",
            "<![CDATA[text]]>",
            "<custom>",
        ] {
            assert_eq!(md.render(source), source, "source={source:?}");
        }
        assert_eq!(
            md.render("<div>\n\n*text*\n</div>"),
            "<div>\n<p><em>text</em></p>\n</div>"
        );
        assert_eq!(md.render("<div>\r\n"), "<div>\n");
    }

    #[test]
    fn quoted_html_preserves_final_line_break() {
        let mut md = crate::MarkdownIt::empty();
        crate::plugins::cmark::add(&mut md);
        crate::plugins::html::add(&mut md);

        for html in [
            "<div>",
            "<script>text</script>",
            "<!-- comment -->",
            "<?instruction?>",
            "<!DOCTYPE html>",
            "<![CDATA[text]]>",
            "<custom>",
        ] {
            for newline in ["", "\n", "\r\n"] {
                let source = format!("> {html}{newline}");
                let trailing = if newline.is_empty() { "" } else { "\n" };
                assert_eq!(
                    md.render(&source),
                    format!("<blockquote>\n{html}{trailing}</blockquote>\n"),
                    "source={source:?}"
                );
            }
        }
        for (source, expected) in [
            (
                "> > <div>",
                "<blockquote>\n<blockquote>\n<div></blockquote>\n</blockquote>\n",
            ),
            (
                "> text\n>\n> <div>",
                "<blockquote>\n<p>text</p>\n<div></blockquote>\n",
            ),
            ("> text", "<blockquote>\n<p>text</p>\n</blockquote>\n"),
            (">", "<blockquote></blockquote>\n"),
        ] {
            assert_eq!(md.render(source), expected, "source={source:?}");
        }
    }
}
