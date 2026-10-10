//! Ordered and bullet lists
//!
//! This plugin parses both kinds of lists (bullet and ordered) as well as list items.
//!
//! looks like `1. this` or `- this`
//!
//!  - <https://spec.commonmark.org/0.30/#lists>
//!  - <https://spec.commonmark.org/0.30/#list-items>
use crate::MarkdownIt;
use crate::common::utils::find_indent_of;
use crate::document::{Document, NodeId, NodeRef, NodeValue};
use crate::parser::block::{BlockRule, DocumentBlockState};
use crate::plugins::cmark::block::hr::HrScanner;
use crate::plugins::cmark::block::paragraph::{Paragraph, ParagraphInterrupt};
use crate::render::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    PlainTextBlockDocumentRenderer,
    write_html_close,
    write_html_open,
};

#[derive(Debug)]
pub struct OrderedList {
    pub start: u32,
    pub marker: char,
}

struct OrderedListDocumentRenderer;

impl DocumentNodeRenderer<OrderedList> for OrderedListDocumentRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        list: &OrderedList,
        context: &mut DocumentRenderContext<'_>,
        output: &mut crate::DocumentWriter,
    ) {
        let mut attrs = node.attrs().clone();
        if list.start != 1 {
            attrs.push(("start".into(), list.start.to_string()));
        }
        render_list_container(node, context, output, "ol", &attrs);
    }
}

impl NodeValue for OrderedList {}

#[derive(Debug)]
pub struct BulletList {
    pub marker: char,
}

struct BulletListDocumentRenderer;

impl DocumentNodeRenderer<BulletList> for BulletListDocumentRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        _: &BulletList,
        context: &mut DocumentRenderContext<'_>,
        output: &mut crate::DocumentWriter,
    ) {
        render_list_container(node, context, output, "ul", node.attrs());
    }
}

impl NodeValue for BulletList {}

#[derive(Debug)]
pub struct ListItem;

struct ListItemDocumentRenderer;

impl DocumentNodeRenderer<ListItem> for ListItemDocumentRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        _: &ListItem,
        context: &mut DocumentRenderContext<'_>,
        output: &mut crate::DocumentWriter,
    ) {
        write_html_open(output, "li", node.attrs());
        context.render_children(node.id(), output);
        write_html_close(output, "li");
        context.cr(output);
    }
}

fn render_list_container(
    node: NodeRef<'_>,
    context: &mut DocumentRenderContext<'_>,
    output: &mut crate::DocumentWriter,
    tag: &str,
    attrs: &[crate::HtmlAttribute],
) {
    context.cr(output);
    write_html_open(output, tag, attrs);
    context.cr(output);
    context.render_children(node.id(), output);
    context.cr(output);
    write_html_close(output, tag);
    context.cr(output);
}

impl NodeValue for ListItem {}

pub fn add(md: &mut MarkdownIt) {
    md.block.add_rule::<ListScanner>().after::<HrScanner>();
    md.add_document_renderer::<OrderedList, _>("html", OrderedListDocumentRenderer);
    md.add_document_renderer::<BulletList, _>("html", BulletListDocumentRenderer);
    md.add_document_renderer::<ListItem, _>("html", ListItemDocumentRenderer);
    md.add_document_renderer::<OrderedList, _>("text", PlainTextBlockDocumentRenderer);
    md.add_document_renderer::<BulletList, _>("text", PlainTextBlockDocumentRenderer);
    md.add_document_renderer::<ListItem, _>("text", PlainTextBlockDocumentRenderer);
}

#[doc(hidden)]
pub struct ListScanner;

impl ListScanner {
    // Search `[-+*][\n ]`, returns next pos after marker on success
    // or -1 on fail.
    fn skip_bullet_list_marker(src: &str) -> Option<usize> {
        let mut chars = src.chars();

        let Some('*' | '-' | '+') = chars.next() else {
            return None;
        };

        match chars.next() {
            Some(' ' | '\t') | None => Some(1),
            Some(_) => None, // " -test " - is not a list item
        }
    }

    // Search `\d+[.)][\n ]`, returns next pos after marker on success
    // or -1 on fail.
    fn skip_ordered_list_marker(src: &str) -> Option<usize> {
        let mut chars = src.chars();
        let Some('0'..='9') = chars.next() else {
            return None;
        };

        let mut pos = 1;
        loop {
            pos += 1;
            match chars.next() {
                Some('0'..='9') => {
                    // List marker should have no more than 9 digits
                    // (prevents integer overflow in browsers)
                    if pos >= 10 {
                        return None;
                    }
                }
                Some(')' | '.') => {
                    // found valid marker
                    break;
                }
                Some(_) | None => {
                    return None;
                }
            }
        }

        match chars.next() {
            Some(' ' | '\t') | None => Some(pos),
            Some(_) => None, // " 1.test " - is not a list item
        }
    }

    fn mark_tight_paragraphs_document(document: &mut Document, item: NodeId) {
        document.rewrite_children(item, |document, nodes| {
            let mut index = 0;
            while index < nodes.len() {
                let node = nodes[index];
                if document.node(node).is::<Paragraph>() {
                    let children = document.take_children(node);
                    let count = children.len();
                    nodes.splice(index..index + 1, children);
                    document.discard_node(node);
                    index += count;
                } else {
                    index += 1;
                }
            }
        });
    }
}

fn scan_list_marker(
    current_line: &str,
    line_indent: i32,
    list_indent: Option<u32>,
    indent_nonspace: i32,
    blk_indent: usize,
    max_indent: i32,
    paragraph_interrupt: bool,
) -> Option<(usize, Option<u32>, char)> {
    if line_indent >= max_indent {
        return None;
    }

    // Special case:
    //  - item 1
    //   - item 2
    //    - item 3
    //     - item 4
    //      - this one is a paragraph continuation
    if let Some(list_indent) = list_indent {
        if indent_nonspace - list_indent as i32 >= max_indent && indent_nonspace < blk_indent as i32
        {
            return None;
        }
    }

    let mut is_terminating_paragraph = false;

    // limit conditions when list can interrupt
    // a paragraph (validation mode only)
    if paragraph_interrupt {
        // Next list item should still terminate previous list item;
        //
        // This code can fail if plugins use blkIndent as well as lists,
        // but I hope the spec gets fixed long before that happens.
        //
        if line_indent >= 0 {
            is_terminating_paragraph = true;
        }
    }

    let marker_value;
    let pos_after_marker;

    // Detect list type and position after marker
    if let Some(p) = ListScanner::skip_ordered_list_marker(current_line) {
        pos_after_marker = p;
        let int = str::parse(&current_line[..pos_after_marker - 1]).unwrap();
        marker_value = Some(int);

        // If we're starting a new ordered list right after
        // a paragraph, it should start with 1.
        if is_terminating_paragraph && int != 1 {
            return None;
        }
    } else {
        let p = ListScanner::skip_bullet_list_marker(current_line)?;
        pos_after_marker = p;
        marker_value = None;
    }

    // If we're starting a new unordered list right after
    // a paragraph, first line should not be empty.
    if is_terminating_paragraph {
        let mut chars = current_line[pos_after_marker..].chars();
        loop {
            match chars.next() {
                Some(' ' | '\t') => {}
                Some(_) => break,
                None => return None,
            }
        }
    }

    // We should terminate list on style change. Remember first one to compare.
    let marker_char = current_line[..pos_after_marker]
        .chars()
        .next_back()
        .unwrap();

    Some((pos_after_marker, marker_value, marker_char))
}

fn find_document_marker(
    state: &mut DocumentBlockState<'_>,
    silent: bool,
) -> Option<(usize, Option<u32>, char)> {
    let line = state.line;
    scan_list_marker(
        state.get_line(line),
        state.line_indent(line),
        state.list_indent,
        state.line_offsets[line].indent_nonspace,
        state.blk_indent,
        state.md.max_indent,
        silent && state.root_ext.contains::<ParagraphInterrupt>(),
    )
}

impl BlockRule for ListScanner {
    const MARKERS: &'static [char] = &[
        '*', '+', '-', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9',
    ];
    const NAMES: &'static [&'static str] = &["list"];

    fn check(state: &mut DocumentBlockState<'_>) -> Option<()> {
        if state.document.node(state.node).is::<BulletList>()
            || state.document.node(state.node).is::<OrderedList>()
        {
            return None;
        }

        find_document_marker(state, true).map(|_| ())
    }

    fn run(state: &mut DocumentBlockState<'_>) -> Option<(Option<NodeId>, usize)> {
        let (mut pos_after_marker, marker_value, marker_char) = find_document_marker(state, false)?;

        let new_node = if let Some(int) = marker_value {
            state.document.create_node(OrderedList {
                start: int,
                marker: marker_char,
            })
        } else {
            state.document.create_node(BulletList {
                marker: marker_char,
            })
        };

        let old_node = std::mem::replace(&mut state.node, new_node);

        //
        // Iterate list items
        //

        let start_line = state.line;
        let mut next_line = state.line;
        let mut prev_empty_end = false;
        let mut tight = true;
        let mut current_line;

        while next_line < state.line_max {
            let offsets = &state.line_offsets[next_line];
            let initial = offsets.indent_nonspace as usize + pos_after_marker;

            let (mut indent_after_marker, first_nonspace) = find_indent_of(
                &state.src[offsets.line_start..offsets.line_end],
                pos_after_marker + offsets.first_nonspace - offsets.line_start,
            );

            let reached_end_of_line = first_nonspace == offsets.line_end - offsets.line_start;
            let indent_nonspace = initial + indent_after_marker;

            #[allow(clippy::if_same_then_else)]
            if reached_end_of_line {
                // trimming space in "-    \n  3" case, indent is 1 here
                indent_after_marker = 1;
            } else if indent_after_marker as i32 > state.md.max_indent {
                // If we have more than the max indent, the indent is 1
                // (the rest is just indented code block)
                indent_after_marker = 1;
            }

            // "  -  test"
            //  ^^^^^ - calculating total length of this thing
            let indent = initial + indent_after_marker;

            // Run subparser & write tokens
            let old_node = std::mem::replace(&mut state.node, state.document.create_node(ListItem));

            // change current state, then restore it after parser subcall
            let old_tight = state.tight;
            let old_lineoffset = offsets.clone();

            //  - example list
            // ^ listIndent position will be here
            //   ^ blkIndent position will be here
            //
            let old_list_indent = state.list_indent;
            state.list_indent = Some(state.blk_indent as u32);
            state.blk_indent = indent;

            state.tight = true;
            state.line_offsets[next_line].first_nonspace =
                first_nonspace + state.line_offsets[next_line].line_start;
            state.line_offsets[next_line].indent_nonspace = indent_nonspace as i32;

            if reached_end_of_line && state.is_empty(next_line + 1) {
                // workaround for this case
                // (list item is empty, list terminates before "foo"):
                // ~~~~~~~~
                //   -
                //
                //     foo
                // ~~~~~~~~
                state.line = if state.line + 2 < state.line_max {
                    state.line + 2
                } else {
                    state.line_max
                }
            } else {
                state.line = next_line;
                // markdown-it counts both the list and its item as containers.
                let old_level = state.level;
                state.level = state.level.saturating_add(1);
                state.tokenize_nested();
                state.level = old_level;
            }

            // If any of list item is tight, mark list as tight
            if !state.tight || prev_empty_end {
                tight = false;
            }

            // Item become loose if finish with empty line,
            // but we should filter last element, because it means list finish
            prev_empty_end = (state.line - next_line) > 1 && state.is_empty(state.line - 1);

            state.blk_indent = state.list_indent.unwrap() as usize;
            state.list_indent = old_list_indent;
            state.line_offsets[next_line] = old_lineoffset;
            state.tight = old_tight;

            let end_line = state.line;
            let node = std::mem::replace(&mut state.node, old_node);
            let srcmap = state.get_map(next_line, end_line - 1);
            state.document.node_mut(node).set_srcmap(srcmap);
            state.document.push_child(state.node, node);
            next_line = state.line;

            if next_line >= state.line_max {
                break;
            }

            //
            // Try to check if list is terminated or continued.
            //
            if state.line_indent(next_line) < 0 {
                break;
            }

            if state.line_indent(next_line) >= state.md.max_indent {
                break;
            }

            // fail if terminating block found
            if state.test_rules_at_line() {
                break;
            }

            current_line = state.get_line(state.line).to_owned();

            // fail if list has another type
            #[allow(clippy::collapsible_else_if)]
            if marker_value.is_some() {
                if let Some(p) = Self::skip_ordered_list_marker(&current_line) {
                    pos_after_marker = p;
                } else {
                    break;
                }
            } else {
                if let Some(p) = Self::skip_bullet_list_marker(&current_line) {
                    pos_after_marker = p;
                } else {
                    break;
                }
            }

            let next_marker_char = current_line[..pos_after_marker]
                .chars()
                .next_back()
                .unwrap();
            if next_marker_char != marker_char {
                break;
            }
        }

        // mark paragraphs tight if needed
        if tight {
            let items = state.document.children(state.node).to_vec();
            for child in items {
                debug_assert!(state.document.node(child).is::<ListItem>());
                Self::mark_tight_paragraphs_document(&mut state.document, child);
            }
        }

        // Finalize list
        state.line = start_line;
        let node = std::mem::replace(&mut state.node, old_node);
        Some((Some(node), next_line - state.line))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn paragraph_and_setext_content_keep_list_interrupt_restrictions() {
        for preset in [crate::Preset::MarkdownItDefault, crate::Preset::CommonMark] {
            let md = crate::MarkdownIt::with_preset(preset);
            for marker in ["+", "*", "1.", "2. bar", "0) bar"] {
                assert_eq!(
                    md.render(&format!("foo\n{marker}")),
                    format!("<p>foo\n{marker}</p>\n")
                );
                assert_eq!(
                    md.render(&format!("foo\n{marker}\n===")),
                    format!("<h1>foo\n{marker}</h1>\n")
                );
            }
            assert_eq!(
                md.render("foo\n1. bar"),
                "<p>foo</p>\n<ol>\n<li>bar</li>\n</ol>\n"
            );
            assert_eq!(
                md.render("plain\n\n> foo\n-\n\nplain\n2. bar"),
                "<p>plain</p>\n<blockquote>\n<p>foo</p>\n</blockquote>\n<ul>\n<li></li>\n</ul>\n<p>plain\n2. bar</p>\n"
            );
        }
    }

    #[test]
    fn respects_max_nesting() {
        let mut md = crate::MarkdownIt::empty();
        crate::plugins::cmark::add(&mut md);
        md.max_nesting = 10;

        let html = md.render(&format!("{}x", "- ".repeat(100)));

        assert_eq!(html.matches("<ul>").count(), 5);
    }

    #[test]
    fn nested_list_content_respects_container_depth() {
        let mut md = crate::MarkdownIt::empty();
        crate::plugins::cmark::add(&mut md);
        let source = "- one\n  - two\n\n    continuation";

        for limit in [3, 4] {
            md.max_nesting = limit;
            assert_eq!(
                md.render(source),
                "<ul>\n<li>one\n<ul>\n<li></li>\n</ul>\n</li>\n</ul>\n"
            );
        }

        md.max_nesting = 5;
        assert_eq!(
            md.render(source),
            "<ul>\n<li>one\n<ul>\n<li>\n<p>two</p>\n<p>continuation</p>\n</li>\n</ul>\n</li>\n</ul>\n"
        );
    }

    #[test]
    fn list_depth_is_restored_for_siblings_and_following_blocks() {
        let mut md = crate::MarkdownIt::empty();
        crate::plugins::cmark::add(&mut md);
        md.max_nesting = 3;

        assert_eq!(
            md.render("1. one\n2. two\n\nafter\n\n> quote"),
            "<ol>\n<li>one</li>\n<li>two</li>\n</ol>\n<p>after</p>\n<blockquote>\n<p>quote</p>\n</blockquote>\n"
        );
    }
}
