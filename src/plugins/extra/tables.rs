//! GFM tables
//!
//! <https://github.github.com/gfm/#tables-extension->
use crate::MarkdownIt;
use crate::document::{NodeId, NodeRef};
use crate::parser::block::{BlockRule, BlockState};
use crate::plugins::cmark::block::blockquote::{BlockquoteInterrupt, BlockquoteScanner};
use crate::plugins::cmark::block::fence::FenceScanner;
use crate::plugins::cmark::block::heading::HeadingScanner;
use crate::plugins::cmark::block::list::ListScanner;
use crate::render::{
    NodeRenderer,
    PlainTextBlockRenderer,
    RenderContext,
    TransparentRenderer,
    write_html_close,
    write_html_open,
};

// Limit the number of empty cells synthesized for short table rows. Without
// this cap, a table with N header columns and N one-cell body rows produces
// O(N^2) AST nodes and output from O(N) input.
//
// Keep this aligned with markdown-it's limit. See:
// https://github.com/markdown-it/markdown-it/issues/1000
const MAX_AUTOCOMPLETED_CELLS: usize = 0x10000;

#[derive(Debug)]
pub struct Table {
    pub alignments: Vec<ColumnAlignment>,
}

struct TableRenderer;

impl NodeRenderer<Table> for TableRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        value: &Table,
        context: &mut RenderContext<'_>,
        output: &mut crate::Writer,
    ) {
        let old_context = context.ext().remove::<TableRenderContext>();
        context.ext().insert(TableRenderContext {
            head: false,
            alignments: value.alignments.clone(),
            index: 0,
        });

        render_block_container(node, context, output, "table");

        context.ext().remove::<TableRenderContext>();
        if let Some(old_context) = old_context {
            context.ext().insert(old_context);
        }
    }
}

#[derive(Debug, Default)]
pub struct TableRenderContext {
    pub head: bool,
    pub index: usize,
    pub alignments: Vec<ColumnAlignment>,
}

#[derive(Debug)]
pub struct TableHead;

struct TableHeadRenderer;

impl NodeRenderer<TableHead> for TableHeadRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        _: &TableHead,
        context: &mut RenderContext<'_>,
        output: &mut crate::Writer,
    ) {
        context
            .ext()
            .get_or_insert_default::<TableRenderContext>()
            .head = true;
        render_block_container(node, context, output, "thead");
        context
            .ext()
            .get_or_insert_default::<TableRenderContext>()
            .head = false;
    }
}

#[derive(Debug)]
pub struct TableBody;

struct TableBodyRenderer;

impl NodeRenderer<TableBody> for TableBodyRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        _: &TableBody,
        context: &mut RenderContext<'_>,
        output: &mut crate::Writer,
    ) {
        render_block_container(node, context, output, "tbody");
    }
}

#[derive(Debug)]
pub struct TableRow;

struct TableRowRenderer;

impl NodeRenderer<TableRow> for TableRowRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        _: &TableRow,
        context: &mut RenderContext<'_>,
        output: &mut crate::Writer,
    ) {
        context
            .ext()
            .get_or_insert_default::<TableRenderContext>()
            .index = 0;
        render_block_container(node, context, output, "tr");
    }
}

struct TableRowTextRenderer;

impl NodeRenderer<TableRow> for TableRowTextRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        _: &TableRow,
        context: &mut RenderContext<'_>,
        output: &mut crate::Writer,
    ) {
        context.cr(output);
        for (index, &cell) in node.children().iter().enumerate() {
            if index != 0 {
                output.write_char('\t');
            }
            context.render_node(cell, output);
        }
        context.cr(output);
    }
}

#[derive(Debug)]
pub struct TableCell;

struct TableCellRenderer;

impl NodeRenderer<TableCell> for TableCellRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        _: &TableCell,
        context: &mut RenderContext<'_>,
        output: &mut crate::Writer,
    ) {
        let table_context = context.ext().get_or_insert_default::<TableRenderContext>();
        let tag = if table_context.head { "th" } else { "td" };
        let alignment = table_context
            .alignments
            .get(table_context.index)
            .copied()
            .unwrap_or_default();
        table_context.index += 1;

        let mut attrs = node.attrs().to_vec();
        match alignment {
            ColumnAlignment::None => (),
            ColumnAlignment::Left => attrs.push(("style".into(), "text-align:left".to_owned())),
            ColumnAlignment::Right => attrs.push(("style".into(), "text-align:right".to_owned())),
            ColumnAlignment::Center => attrs.push(("style".into(), "text-align:center".to_owned())),
        }

        write_html_open(output, tag, &attrs);
        context.render_children(node.id(), output);
        write_html_close(output, tag);
        context.cr(output);
    }
}

fn render_block_container(
    node: NodeRef<'_>,
    context: &mut RenderContext<'_>,
    output: &mut crate::Writer,
    tag: &str,
) {
    context.cr(output);
    write_html_open(output, tag, node.attrs());
    context.cr(output);
    context.render_children(node.id(), output);
    context.cr(output);
    write_html_close(output, tag);
    context.cr(output);
}

pub fn add(md: &mut MarkdownIt) {
    md.block
        .add_rule::<TableScanner>()
        // A table header may look like a code fence, so match tables first.
        .before::<FenceScanner>()
        .before::<BlockquoteScanner>()
        .before::<ListScanner>()
        .before::<HeadingScanner>();
    md.add_renderer::<Table, _>("html", TableRenderer);
    md.add_renderer::<TableHead, _>("html", TableHeadRenderer);
    md.add_renderer::<TableBody, _>("html", TableBodyRenderer);
    md.add_renderer::<TableRow, _>("html", TableRowRenderer);
    md.add_renderer::<TableCell, _>("html", TableCellRenderer);
    md.add_renderer::<Table, _>("text", PlainTextBlockRenderer);
    md.add_renderer::<TableHead, _>("text", TransparentRenderer);
    md.add_renderer::<TableBody, _>("text", TransparentRenderer);
    md.add_renderer::<TableRow, _>("text", TableRowTextRenderer);
    md.add_renderer::<TableCell, _>("text", TransparentRenderer);
}

#[doc(hidden)]
pub struct TableScanner;

#[derive(Debug)]
struct RowContent {
    str: String,
    #[cfg(feature = "sourcemap")]
    srcmap: Vec<(usize, usize)>,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum ColumnAlignment {
    #[default]
    None,
    Left,
    Right,
    Center,
}

impl TableScanner {
    fn scan_row(line: &str) -> Vec<RowContent> {
        let mut result = Vec::new();
        let mut str = String::new();
        let mut segment_start = 0;
        #[cfg(feature = "sourcemap")]
        let mut srcmap = vec![(0, 0)];
        let mut is_escaped = false;
        let mut is_leading = true;

        for (pos, ch) in line.char_indices() {
            match ch {
                ' ' | '\t' if is_leading => {
                    segment_start += 1;
                    #[cfg(feature = "sourcemap")]
                    {
                        srcmap[0].1 += 1;
                    }
                }
                '|' => {
                    is_leading = false;
                    if is_escaped {
                        str.push_str(&line[segment_start..pos - 1]);
                        segment_start = pos;
                        #[cfg(feature = "sourcemap")]
                        srcmap.push((str.len(), pos));
                        is_escaped = false;
                    } else {
                        str.push_str(&line[segment_start..pos]);
                        result.push(RowContent {
                            str: std::mem::take(&mut str),
                            #[cfg(feature = "sourcemap")]
                            srcmap: std::mem::take(&mut srcmap),
                        });
                        segment_start = pos + 1;
                        #[cfg(feature = "sourcemap")]
                        {
                            srcmap = vec![(0, pos + 1)];
                        }
                        is_escaped = false;
                        is_leading = true;
                    }
                }
                '\\' => {
                    is_leading = false;
                    is_escaped = true;
                }
                _ => {
                    is_leading = false;
                    is_escaped = false;
                }
            }
        }

        str.push_str(&line[segment_start..]);
        result.push(RowContent {
            str,
            #[cfg(feature = "sourcemap")]
            srcmap,
        });

        // trim trailing spaces
        for content in result.iter_mut() {
            while content.str.ends_with([' ', '\t']) {
                content.str.pop();
            }
        }

        // remove last cell if empty
        if let Some(RowContent { str, .. }) = result.last() {
            if str.is_empty() {
                result.pop();
            }
        }

        // remove first cell if empty
        if let Some(RowContent { str, .. }) = result.first() {
            if str.is_empty() {
                result.remove(0);
            }
        }

        result
    }

    fn scan_alignment_row(line: &str) -> Option<Vec<ColumnAlignment>> {
        // quick check second line, only allow :-| and spaces
        // (this is for performance only)
        for ch in line.chars() {
            match ch {
                '|' | ':' | '-' | ' ' | '\t' => (),
                _ => return None,
            }
        }
        if line.len() < 2 {
            return None;
        }

        // if first character is '-', then second character must not be a space
        // (due to parsing ambiguity with list)
        if line.starts_with("- ") || line.starts_with("-\t") {
            return None;
        }

        let mut result = Vec::new();

        for RowContent { str, .. } in Self::scan_row(line) {
            let mut alignment: u8 = 0;
            let mut cell = str.as_str();

            if cell.starts_with(':') {
                alignment |= 1;
                cell = &cell[1..];
            }

            if cell.ends_with(':') {
                alignment |= 2;
                cell = &cell[..cell.len() - 1];
            }

            // only allow '-----' in the remainder
            if cell.is_empty() || cell.contains(|c| c != '-') {
                return None;
            }

            result.push(match alignment {
                0 => ColumnAlignment::None,
                1 => ColumnAlignment::Left,
                2 => ColumnAlignment::Right,
                3 => ColumnAlignment::Center,
                _ => unreachable!(),
            });
        }

        Some(result)
    }

    fn scan_header<'a>(
        line: usize,
        line_max: usize,
        max_indent: i32,
        get_line: impl Fn(usize) -> (&'a str, i32),
    ) -> Option<(Vec<RowContent>, Vec<ColumnAlignment>)> {
        // should have at least two lines
        if line + 2 > line_max {
            return None;
        }

        if get_line(line).1 >= max_indent {
            return None;
        }

        let next_line = line + 1;
        if get_line(next_line).1 < 0 {
            return None;
        }

        if get_line(next_line).1 >= max_indent {
            return None;
        }

        let header = get_line(line).0;
        if !header.contains('|') {
            return None;
        }

        let alignments = Self::scan_alignment_row(get_line(next_line).0)?;
        let header_row = Self::scan_row(header);

        // header row must match the delimiter row in the number of cells
        if header_row.len() != alignments.len() {
            return None;
        }

        // table without any columns is not a table, see markdown-it#724
        if header_row.is_empty() {
            return None;
        }

        Some((header_row, alignments))
    }
}

impl BlockRule for TableScanner {
    const NAMES: &'static [&'static str] = &["table", "tables"];
    fn check(state: &mut BlockState<'_>) -> Option<()> {
        if state.document.node(state.node).is::<TableBody>()
            || state.root_ext.get::<BlockquoteInterrupt>().is_some()
        {
            return None;
        }

        Self::scan_header(state.line, state.line_max, state.md.max_indent, |line| {
            (state.get_line(line), state.line_indent(line))
        })
        .map(|_| ())
    }

    fn run(state: &mut BlockState<'_>) -> Option<(Option<NodeId>, usize)> {
        let (header_row, alignments) =
            Self::scan_header(state.line, state.line_max, state.md.max_indent, |line| {
                (state.get_line(line), state.line_indent(line))
            })?;
        let table_cell_count = header_row.len();
        let table_node = state.document.create_node(Table { alignments });

        let thead_node = state.document.create_node(TableHead);
        #[cfg(feature = "sourcemap")]
        {
            let srcmap = state.get_map(state.line, state.line + 1);
            state.document.node_mut(thead_node).set_srcmap(srcmap);
        }

        let row_node = state.document.create_node(TableRow);
        #[cfg(feature = "sourcemap")]
        {
            let srcmap = state.get_map(state.line, state.line);
            state.document.node_mut(row_node).set_srcmap(srcmap);
        }

        fn add_cell(state: &mut BlockState<'_>, row_node: NodeId, content: RowContent) {
            let RowContent {
                str: cell,
                #[cfg(feature = "sourcemap")]
                srcmap,
            } = content;
            let cell_node = state.document.create_node(TableCell);

            #[cfg(feature = "sourcemap")]
            let start = state.line_offsets[state.line].first_nonspace;
            #[cfg(feature = "sourcemap")]
            state.document.node_mut(cell_node).set_srcmap(Some(
                crate::common::sourcemap::SourcePos::new(
                    start + srcmap.first().unwrap().1,
                    start + srcmap.last().unwrap().1 + cell.len() - srcmap.last().unwrap().0,
                ),
            ));

            if !cell.is_empty() {
                #[cfg(feature = "sourcemap")]
                let mapping = srcmap
                    .into_iter()
                    .map(|(dst, src)| (dst, src + start))
                    .collect();
                #[cfg(not(feature = "sourcemap"))]
                let mapping = crate::parser::InlineMapping;

                let pending = state.pending_inline(cell, mapping);
                state.document.push_child(cell_node, pending);
            }
            state.document.push_child(row_node, cell_node);
        }

        for content in header_row {
            add_cell(state, row_node, content);
        }

        state.document.push_child(thead_node, row_node);
        state.document.push_child(table_node, thead_node);

        let tbody_node = state.document.create_node(TableBody);
        let old_node = std::mem::replace(&mut state.node, tbody_node);

        //
        // Iterate table rows
        //

        let start_line = state.line;
        state.line += 2;
        let mut autocompleted_cells = 0usize;

        while state.line < state.line_max {
            //
            // Try to check if table is terminated or continued.
            //
            if state.line_indent(state.line) < 0 {
                break;
            }

            if state.line_indent(state.line) >= state.md.max_indent {
                break;
            }

            // stop if the line is empty
            if state.is_empty(state.line) {
                break;
            }

            // fail if terminating block found
            if state.test_rules_at_line() {
                break;
            }

            let line = state.get_line(state.line);
            #[cfg(feature = "sourcemap")]
            let line_len = line.len();

            let mut body_row = Self::scan_row(line);
            let missing_cells = table_cell_count.saturating_sub(body_row.len());
            let Some(total_autocompleted_cells) = autocompleted_cells.checked_add(missing_cells)
            else {
                break;
            };
            if total_autocompleted_cells > MAX_AUTOCOMPLETED_CELLS {
                break;
            }
            autocompleted_cells = total_autocompleted_cells;

            let row_node = state.document.create_node(TableRow);

            #[cfg(feature = "sourcemap")]
            {
                let srcmap = state.get_map(state.line, state.line);
                state.document.node_mut(row_node).set_srcmap(srcmap);
            }

            let mut end_of_line = RowContent {
                str: String::new(),
                #[cfg(feature = "sourcemap")]
                srcmap: vec![(0, line_len)],
            };
            for index in 0..table_cell_count {
                let content = body_row.get_mut(index).unwrap_or(&mut end_of_line);
                add_cell(
                    state,
                    row_node,
                    RowContent {
                        str: content.str.clone(),
                        #[cfg(feature = "sourcemap")]
                        srcmap: content.srcmap.clone(),
                    },
                );
            }

            state.document.push_child(state.node, row_node);
            state.line += 1;
        }

        let tbody_node = std::mem::replace(&mut state.node, old_node);
        if !state.document.node(tbody_node).children().is_empty() {
            #[cfg(feature = "sourcemap")]
            {
                let srcmap = state.get_map(start_line + 2, state.line - 1);
                state.document.node_mut(tbody_node).set_srcmap(srcmap);
            }

            state.document.push_child(table_node, tbody_node);
        } else {
            state.document.discard_node(tbody_node);
        }

        let line_count = state.line - start_line;
        state.line = start_line;
        Some((Some(table_node), line_count))
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_AUTOCOMPLETED_CELLS, TableScanner};

    #[test]
    fn tables_take_precedence_over_blockquote_markers_in_headers() {
        for tables_first in [false, true] {
            let mut md = crate::MarkdownIt::empty();
            if tables_first {
                super::add(&mut md);
            }
            crate::plugins::cmark::add(&mut md);
            if !tables_first {
                super::add(&mut md);
            }
            for (source, expected) in [
                (
                    ">|\n:-",
                    "<table>\n<thead>\n<tr>\n<th style=\"text-align:left\">&gt;</th>\n</tr>\n</thead>\n</table>\n",
                ),
                (
                    "> a |\n---|\n",
                    "<table>\n<thead>\n<tr>\n<th>&gt; a</th>\n</tr>\n</thead>\n</table>\n",
                ),
                (
                    "> a |\n> ---|\n",
                    "<blockquote>\n<table>\n<thead>\n<tr>\n<th>a</th>\n</tr>\n</thead>\n</table>\n</blockquote>\n",
                ),
                (
                    "> ordinary quote",
                    "<blockquote>\n<p>ordinary quote</p>\n</blockquote>\n",
                ),
            ] {
                assert_eq!(
                    md.render(source),
                    expected,
                    "{source:?}, tables_first={tables_first}"
                );
            }
        }
    }

    #[test]
    fn should_split_cells() {
        assert_eq!(TableScanner::scan_row("").len(), 0);
        assert_eq!(TableScanner::scan_row("a").len(), 1);
        assert_eq!(TableScanner::scan_row("a | b").len(), 2);
        assert_eq!(TableScanner::scan_row("a | b | c").len(), 3);
    }

    #[test]
    fn should_ignore_leading_trailing_empty_cells() {
        assert_eq!(TableScanner::scan_row("foo | bar").len(), 2);
        assert_eq!(TableScanner::scan_row("foo | bar |").len(), 2);
        assert_eq!(TableScanner::scan_row("| foo | bar").len(), 2);
        assert_eq!(TableScanner::scan_row("| foo | bar |").len(), 2);
        assert_eq!(TableScanner::scan_row("| | foo | bar | |").len(), 4);
        assert_eq!(TableScanner::scan_row("|").len(), 0);
        assert_eq!(TableScanner::scan_row("||").len(), 1);
    }

    #[test]
    fn should_trim_cell_content() {
        assert_eq!(TableScanner::scan_row("|foo|")[0].str, "foo");
        assert_eq!(TableScanner::scan_row("| foo |")[0].str, "foo");
        assert_eq!(TableScanner::scan_row("|\tfoo\t|")[0].str, "foo");
        assert_eq!(TableScanner::scan_row("| \t foo \t |")[0].str, "foo");
    }

    #[test]
    fn should_process_backslash_escapes() {
        assert_eq!(
            TableScanner::scan_row(r#"| foo\bar |"#)[0].str,
            r#"foo\bar"#
        );
        assert_eq!(
            TableScanner::scan_row(r#"| foo\|bar |"#)[0].str,
            r#"foo|bar"#
        );
        assert_eq!(
            TableScanner::scan_row(r#"| foo\\|bar |"#)[0].str,
            r#"foo\|bar"#
        );
        assert_eq!(
            TableScanner::scan_row(r#"| foo\\\|bar |"#)[0].str,
            r#"foo\\|bar"#
        );
        assert_eq!(
            TableScanner::scan_row(r#"| foo\\\\|bar |"#)[0].str,
            r#"foo\\\|bar"#
        );
    }

    #[test]
    #[cfg(feature = "sourcemap")]
    fn should_trim_cell_content_srcmaps() {
        let row = TableScanner::scan_row("| foo | \tbar\t |");
        assert_eq!(row[0].str, "foo");
        assert_eq!(row[0].srcmap, vec![(0, 2)]);
        assert_eq!(row[1].str, "bar");
        assert_eq!(row[1].srcmap, vec![(0, 9)]);
    }

    #[test]
    #[cfg(feature = "sourcemap")]
    fn should_process_backslash_escapes_srcmaps() {
        let row = TableScanner::scan_row(r#"|  foo\\|bar\\\|baz\  |"#);
        assert_eq!(row[0].str, r#"foo\|bar\\|baz\"#);
        assert_eq!(row[0].srcmap, vec![(0, 3), (4, 8), (10, 15)]);
    }

    #[test]
    fn require_pipe_in_header_row() {
        let md = &mut crate::MarkdownIt::empty();
        crate::plugins::extra::tables::add(md);
        let html = md.render("foo\n---\nbar");
        assert_eq!(html.trim(), "foo\n---\nbar");
        let html = md.render("|foo\n---\nbar");
        assert!(html.trim().starts_with("<table"));
        let html = md.render("foo\n|---\nbar");
        assert_eq!(html.trim(), "foo\n|---\nbar");
        let html = md.render("foo\n:---\nbar");
        assert_eq!(html.trim(), "foo\n:---\nbar");
        let html = md.render("|foo\n|---\nbar");
        assert!(html.trim().starts_with("<table"));
        let html = md.render("|foo\n:---\nbar");
        assert!(html.trim().starts_with("<table"));
    }

    #[test]
    fn should_limit_autocompleted_cells() {
        let column_count = 257;
        let missing_cells_per_row = column_count - 1;
        let accepted_rows = MAX_AUTOCOMPLETED_CELLS / missing_cells_per_row;
        let body_row_count = accepted_rows + 2;

        let src = format!(
            "{}\n{}\n{}",
            "x|".repeat(column_count),
            "-|".repeat(column_count),
            "x|\n".repeat(body_row_count),
        );

        let mut md = crate::MarkdownIt::empty();
        crate::plugins::cmark::add(&mut md);
        crate::plugins::extra::tables::add(&mut md);
        let html = md.render(&src);

        assert_eq!(html.matches("<td>").count(), column_count * accepted_rows);
        assert!(html.ends_with("<p>x|\nx|</p>\n"));
    }
}
