// Parser state class
//
use memchr::memchr2_iter;

use crate::common::sourcemap::SourcePos;
use crate::common::utils::calc_right_whitespace_with_tabstops;
use crate::parser::extset::RootExtSet;
use crate::{MarkdownIt, Node};

#[derive(Debug)]
#[readonly::make]
/// Sandbox object containing data required to parse block structures.
pub struct BlockState<'a, 'b>
where
    'b: 'a,
{
    /// Markdown source.
    #[readonly]
    pub src: &'b str,

    /// Link to parser instance.
    #[readonly]
    pub md: &'a MarkdownIt,

    pub root_ext: &'b mut RootExtSet,

    /// Current node, your rule is supposed to add children to it.
    pub node: Node,

    pub line_offsets: Vec<LineOffset>,

    /// Current block content indent (for example, if we are
    /// inside a list, it would be positioned after list marker).
    pub blk_indent: usize,

    /// Current line in src.
    pub line: usize,

    /// Maximum allowed line in src.
    pub line_max: usize,

    /// True if there are no empty lines between paragraphs, used to
    /// toggle loose/tight mode for lists.
    pub tight: bool,

    /// indent of the current list block.
    pub list_indent: Option<u32>,

    pub level: u32,
}

/// Holds start/end/etc. positions for a specific source text line.
#[derive(Debug, Clone)]
pub struct LineOffset {
    /// `line_start` is the actual start of the line.
    ///
    ///     # const IGNORE : &str = stringify! {
    ///     "  >  blockquote\r\n"
    ///      ^-- it will always point here (must not be modified by rules)
    ///     # };
    pub line_start: usize,

    /// `line_end` is first newline character after the line,
    /// or position after string length if there aren't any newlines left.
    ///
    ///     # const IGNORE : &str = stringify! {
    ///     "  >  blockquote\r\n"
    ///                     ^-- it will point here
    ///     # };
    pub line_end: usize,

    /// `first_nonspace` is the byte offset of the first non-space character in
    /// the current line.
    ///
    ///     # const IGNORE : &str = stringify! {
    ///     "   >  blockquote\r\n"
    ///            ^-- it will point here when paragraph is parsed
    ///         ^----- it is initially pointed here
    ///     # };
    ///
    /// It will be modified by rules (list and blockquote), chars before it
    /// must be treated as whitespaces.
    ///
    pub first_nonspace: usize,

    /// `indent_nonspace` is the indent (amount of virtual spaces from start)
    /// of first non-space character in the current line, taking into account
    /// tab expansion.
    ///
    /// For example, in case of ` \t foo`, indent is 5 (tab ends at multiple of 4,
    /// then one space after it). Only tabs and spaces are counted for it,
    /// so no funny unicode business (if cmark supported unicode spaces, they'd
    /// be counted as 1 each regardless of utf8 width).
    ///
    /// You should compare `indent_nonspace` with `state.blkindent` when determining
    /// real indent after taking into account lists.
    ///
    /// Most block rules in commonmark are indented 0..=3, and >=4 is code block.
    /// Special value of ident_nonspace=-1 is used by this library as a sign
    /// that this rule can only be a paragraph continuation (used in blockquotes),
    /// so you must take into account that any math can end up negative.
    ///
    pub indent_nonspace: i32,
}

impl<'a, 'b> BlockState<'a, 'b> {
    pub fn new(src: &'b str, md: &'a MarkdownIt, root_ext: &'b mut RootExtSet, node: Node) -> Self {
        let mut result = Self {
            src,
            md,
            root_ext,
            node,
            line_offsets: Vec::new(),
            blk_indent: 0,
            line: 0,
            line_max: 0,
            tight: false,
            list_indent: None,
            level: 0,
        };

        result.generate_caches();
        result
    }

    fn generate_caches(&mut self) {
        self.line_offsets = build_line_offsets(self.src);
        self.line_max = self.line_offsets.len();
    }

    #[must_use]
    pub fn test_rules_at_line(&mut self) -> bool {
        for rule in self.md.block.ruler.iter() {
            if rule.0(self).is_some() {
                return true;
            }
        }
        false
    }

    #[must_use]
    #[inline]
    pub fn is_empty(&self, line: usize) -> bool {
        if let Some(offsets) = self.line_offsets.get(line) {
            offsets.first_nonspace >= offsets.line_end
        } else {
            false
        }
    }

    pub fn skip_empty_lines(&self, from: usize) -> usize {
        let mut line = from;
        while line != self.line_max && self.is_empty(line) {
            line += 1;
        }
        line
    }

    /// return line indent of specific line, taking into account blockquotes and lists;
    /// it may be negative if a text has less indentation than current list item
    #[must_use]
    #[inline]
    pub fn line_indent(&self, line: usize) -> i32 {
        if line < self.line_max {
            self.line_offsets[line].indent_nonspace - self.blk_indent as i32
        } else {
            0
        }
    }

    /// return a single line, trimming initial spaces
    #[must_use]
    #[inline]
    pub fn get_line(&self, line: usize) -> &str {
        if line < self.line_max {
            let pos = self.line_offsets[line].first_nonspace;
            let max = self.line_offsets[line].line_end;
            &self.src[pos..max]
        } else {
            ""
        }
    }

    /// Cut a range of lines begin..end (not including end) from the source without preceding indent.
    /// Returns a string (lines) plus a mapping (start of each line in result -> start of each line in source).
    pub fn get_lines(
        &self,
        begin: usize,
        end: usize,
        indent: usize,
        keep_last_lf: bool,
    ) -> (String, Vec<(usize, usize)>) {
        debug_assert!(begin <= end);

        let mut line = begin;
        let mut result = String::new();
        let mut mapping = Vec::new();

        while line < end {
            let offsets = &self.line_offsets[line];
            let last = offsets.line_end;
            let add_last_lf = line + 1 < end || keep_last_lf;

            let (num_spaces, first) = calc_right_whitespace_with_tabstops(
                &self.src[offsets.line_start..offsets.first_nonspace],
                offsets.indent_nonspace - indent as i32,
            );

            mapping.push((result.len(), offsets.line_start + first));
            result += &" ".repeat(num_spaces);
            result += &self.src[offsets.line_start + first..last];
            if add_last_lf {
                result.push('\n');
            }
            line += 1;
        }

        (result, mapping)
    }

    #[must_use]
    #[inline]
    pub fn get_map(&self, start_line: usize, end_line: usize) -> Option<SourcePos> {
        debug_assert!(start_line <= end_line);

        Some(SourcePos::new(
            self.line_offsets[start_line].first_nonspace,
            self.line_offsets[end_line].line_end,
        ))
    }

    #[must_use]
    #[inline]
    pub fn get_map_from_offsets(&self, start_pos: usize, end_pos: usize) -> Option<SourcePos> {
        debug_assert!(start_pos <= end_pos);

        Some(SourcePos::new(start_pos, end_pos))
    }
}

fn build_line_offsets(src: &str) -> Vec<LineOffset> {
    let bytes = src.as_bytes();
    let mut result = Vec::new();
    let mut line_start = 0;

    for line_end in memchr2_iter(b'\n', b'\r', bytes) {
        // the LF half of CRLF was already consumed when CR was visited.
        if line_end < line_start {
            continue;
        }

        result.push(build_line_offset(bytes, line_start, line_end));
        line_start = line_end + 1;
        if bytes[line_end] == b'\r' && bytes.get(line_start) == Some(&b'\n') {
            line_start += 1;
        }
    }

    // A final line break terminates the preceding line; it does not create an
    // additional empty line in the block parser's line cache. Empty input is
    // represented by one empty line for compatibility with the block parser.
    if line_start < bytes.len() || result.is_empty() {
        result.push(build_line_offset(bytes, line_start, bytes.len()));
    }

    result
}

#[inline]
fn build_line_offset(bytes: &[u8], line_start: usize, line_end: usize) -> LineOffset {
    let mut first_nonspace = line_start;
    let mut indent_nonspace = 0;
    while first_nonspace < line_end {
        match bytes[first_nonspace] {
            b' ' => indent_nonspace += 1,
            b'\t' => indent_nonspace += 4 - indent_nonspace % 4,
            _ => break,
        }
        first_nonspace += 1;
    }

    LineOffset {
        line_start,
        line_end,
        first_nonspace,
        indent_nonspace,
    }
}

#[cfg(test)]
mod tests {
    use super::{LineOffset, build_line_offsets};

    fn compact(offsets: &[LineOffset]) -> Vec<(usize, usize, usize, i32)> {
        offsets
            .iter()
            .map(|line| {
                (
                    line.line_start,
                    line.line_end,
                    line.first_nonspace,
                    line.indent_nonspace,
                )
            })
            .collect()
    }

    #[test]
    fn empty_input_has_one_empty_line() {
        assert_eq!(compact(&build_line_offsets("")), vec![(0, 0, 0, 0)]);
    }

    #[test]
    fn supports_lf_crlf_and_cr_line_endings() {
        assert_eq!(
            compact(&build_line_offsets("a\nb")),
            vec![(0, 1, 0, 0), (2, 3, 2, 0)]
        );
        assert_eq!(
            compact(&build_line_offsets("a\r\nb")),
            vec![(0, 1, 0, 0), (3, 4, 3, 0)]
        );
        assert_eq!(
            compact(&build_line_offsets("a\rb")),
            vec![(0, 1, 0, 0), (2, 3, 2, 0)]
        );
    }

    #[test]
    fn final_line_break_does_not_add_an_empty_line() {
        assert_eq!(compact(&build_line_offsets("a\n")), vec![(0, 1, 0, 0)]);
        assert_eq!(
            compact(&build_line_offsets("\n\n")),
            vec![(0, 0, 0, 0), (1, 1, 1, 0)]
        );
    }

    #[test]
    fn expands_only_leading_spaces_and_tabs() {
        assert_eq!(
            compact(&build_line_offsets(" \t foo\n\t \tbar")),
            vec![(0, 6, 3, 5), (7, 13, 10, 8)]
        );
    }

    #[test]
    fn offsets_remain_on_utf8_boundaries() {
        assert_eq!(
            compact(&build_line_offsets("中\n é")),
            vec![(0, 3, 0, 0), (4, 7, 5, 1)]
        );
    }

    #[test]
    fn handles_deep_indentation() {
        let src = "                \titem";
        assert_eq!(
            compact(&build_line_offsets(src)),
            vec![(0, src.len(), 17, 20)]
        );
    }
}
