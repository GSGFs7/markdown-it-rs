//! Block quotes
//!
//! `> looks like this`
//!
//! <https://spec.commonmark.org/0.30/#block-quotes>
use crate::MarkdownIt;
use crate::common::utils::find_indent_of;
use crate::document::{NodeId, NodeRef, NodeValue};
use crate::parser::block::{BlockRule, DocumentBlockState};
use crate::plugins::cmark::block::reference::Definition;
use crate::render::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    PlainTextBlockDocumentRenderer,
    write_html_close,
    write_html_open,
};

#[derive(Debug)]
pub struct Blockquote;

struct BlockquoteDocumentRenderer;

impl DocumentNodeRenderer<Blockquote> for BlockquoteDocumentRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        _: &Blockquote,
        context: &mut DocumentRenderContext<'_>,
        output: &mut crate::DocumentWriter,
    ) {
        context.cr(output);
        write_html_open(output, "blockquote", node.attrs());

        // Definitions render nothing; empty quotes need no inner newline.
        let has_visible_children = node
            .children()
            .iter()
            .any(|&child| !context.document().node(child).is::<Definition>());
        if has_visible_children {
            context.cr(output);
        }
        context.render_children(node.id(), output);
        if has_visible_children {
            context.cr(output);
        }

        write_html_close(output, "blockquote");
        context.cr(output);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn lists_terminate_blockquotes_without_paragraph_interrupt_restrictions() {
        for preset in [crate::Preset::MarkdownItDefault, crate::Preset::CommonMark] {
            let mut md = crate::MarkdownIt::with_preset(preset);
            for limit in [3, 100] {
                md.max_nesting = limit;
                for marker in ["-", "+", "*", "- \t"] {
                    assert_eq!(
                        md.render(&format!("> foo\n{marker}")),
                        "<blockquote>\n<p>foo</p>\n</blockquote>\n<ul>\n<li></li>\n</ul>\n",
                        "marker={marker:?}, max_nesting={limit}"
                    );
                }
                for (marker, attrs) in [("1.", ""), ("2.", " start=\"2\""), ("0)", " start=\"0\"")]
                {
                    for content in ["", " bar"] {
                        assert_eq!(
                            md.render(&format!("> foo\n{marker}{content}")),
                            format!(
                                "<blockquote>\n<p>foo</p>\n</blockquote>\n<ol{attrs}>\n<li>{}</li>\n</ol>\n",
                                content.trim()
                            ),
                            "marker={marker:?}, content={content:?}, max_nesting={limit}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn empty_blockquotes_have_no_inner_newline() {
        for preset in [crate::Preset::MarkdownItDefault, crate::Preset::CommonMark] {
            let mut md = crate::MarkdownIt::with_preset(preset);
            for xhtml_out in [false, true] {
                md.render_options.xhtml_out = xhtml_out;
                for source in [">", ">\n> \n>", "> [foo]: /url"] {
                    assert_eq!(md.render(source), "<blockquote></blockquote>\n");
                }
                assert_eq!(
                    md.render("[foo]\n\n> [foo]: /url\n"),
                    "<p><a href=\"/url\">foo</a></p>\n<blockquote></blockquote>\n"
                );
                assert_eq!(
                    md.render("> >"),
                    "<blockquote>\n<blockquote></blockquote>\n</blockquote>\n"
                );
                assert_eq!(
                    md.render("> text"),
                    "<blockquote>\n<p>text</p>\n</blockquote>\n"
                );
                md.max_nesting = 1;
                assert_eq!(md.render("> text"), "<blockquote></blockquote>\n");
                md.max_nesting = 100;
            }
        }
    }
}

impl NodeValue for Blockquote {}

pub fn add(md: &mut MarkdownIt) {
    md.block.add_rule::<BlockquoteScanner>();
    md.add_document_renderer::<Blockquote, _>("html", BlockquoteDocumentRenderer);
    md.add_document_renderer::<Blockquote, _>("text", PlainTextBlockDocumentRenderer);
}

#[doc(hidden)]
pub struct BlockquoteScanner;

fn is_blockquote_line(line: &str, line_indent: i32, max_indent: i32) -> bool {
    if line_indent >= max_indent {
        return false;
    }

    // check the block quote marker
    matches!(line.chars().next(), Some('>'))
}

impl BlockRule for BlockquoteScanner {
    const MARKERS: &'static [char] = &['>'];
    const NAMES: &'static [&'static str] = &["blockquote"];

    fn check(state: &mut DocumentBlockState<'_>) -> Option<()> {
        is_blockquote_line(
            state.get_line(state.line),
            state.line_indent(state.line),
            state.md.max_indent,
        )
        .then_some(())
    }

    fn run(state: &mut DocumentBlockState<'_>) -> Option<(Option<NodeId>, usize)> {
        <Self as BlockRule>::check(state)?;

        let mut old_line_offsets = Vec::new();
        let start_line = state.line;
        let mut next_line = state.line;
        let mut last_line_empty = false;

        // Search the end of the block
        //
        // Block ends with either:
        //  1. an empty line outside:
        //     ```
        //     > test
        //
        //     ```
        //  2. an empty line inside:
        //     ```
        //     >
        //     test
        //     ```
        //  3. another tag:
        //     ```
        //     > test
        //      - - -
        //     ```
        while next_line < state.line_max {
            // check if it's outdented, i.e. it's inside list item and indented
            // less than said list item:
            //
            // ```
            // 1. anything
            //    > current blockquote
            // 2. checking this line
            // ```
            let is_outdented = state.line_indent(next_line) < 0;
            let line = state.get_line(next_line).to_owned();
            let mut chars = line.chars();

            match chars.next() {
                None => {
                    // Case 1: line is not inside the blockquote, and this line is empty.
                    break;
                }
                Some('>') if !is_outdented => {
                    // This line is inside the blockquote.

                    // set offset past spaces and ">"
                    let offsets = &state.line_offsets[next_line];
                    let pos_after_marker = offsets.first_nonspace + 1;

                    old_line_offsets.push(state.line_offsets[next_line].clone());

                    let (mut indent_after_marker, first_nonspace) = find_indent_of(
                        &state.src[offsets.line_start..offsets.line_end],
                        pos_after_marker - offsets.line_start,
                    );

                    last_line_empty = first_nonspace == offsets.line_end - offsets.line_start;

                    // skip one optional space after '>'
                    if matches!(chars.next(), Some(' ' | '\t')) {
                        indent_after_marker -= 1;
                    }

                    state.line_offsets[next_line].indent_nonspace = indent_after_marker as i32;
                    state.line_offsets[next_line].first_nonspace =
                        first_nonspace + state.line_offsets[next_line].line_start;
                    next_line += 1;
                    continue;
                }
                _ => {}
            }

            // Case 2: line is not inside the blockquote, and the last line was empty.
            if last_line_empty {
                break;
            }

            // Case 3: another tag found.
            state.line = next_line;

            if state.test_rules_at_line() {
                // Quirk to enforce "hard termination mode" for paragraphs;
                // normally if you call `nodeize(state, startLine, nextLine)`,
                // paragraphs will look below nextLine for paragraph continuation,
                // but if blockquote is terminated by another tag, they shouldn't
                //state.line_max = next_line;

                if state.blk_indent != 0 {
                    // state.blkIndent was non-zero, we now set it to zero,
                    // so we need to re-calculate all offsets to appear as
                    // if indent wasn't changed
                    old_line_offsets.push(state.line_offsets[next_line].clone());
                    state.line_offsets[next_line].indent_nonspace -= state.blk_indent as i32;
                }

                break;
            }

            old_line_offsets.push(state.line_offsets[next_line].clone());

            // A negative indentation means that this is a paragraph continuation
            //
            state.line_offsets[next_line].indent_nonspace = -1;
            next_line += 1;
        }

        let old_indent = state.blk_indent;
        state.blk_indent = 0;

        let old_node = std::mem::replace(&mut state.node, state.document.create_node(Blockquote));
        let old_line_max = state.line_max;
        state.line = start_line;
        state.line_max = next_line;
        state.tokenize_nested();
        next_line = state.line;
        state.line = start_line;
        state.line_max = old_line_max;

        // Restore original tShift; this might not be necessary since the parser
        // has already been here, but just to make sure we can do that.
        for (idx, line_offset) in old_line_offsets.iter_mut().enumerate() {
            std::mem::swap(&mut state.line_offsets[idx + start_line], line_offset);
        }
        state.blk_indent = old_indent;

        let node = std::mem::replace(&mut state.node, old_node);
        Some((Some(node), next_line - start_line))
    }
}
