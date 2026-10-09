// Parser state class
//
use memchr::memchr2_iter;

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

pub(crate) fn build_line_offsets(src: &str) -> Vec<LineOffset> {
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

    // A final line break ends the preceding line without adding an empty one;
    // empty input still gets one line for the block parser.
    if line_start < bytes.len() {
        let offset = build_line_offset(bytes, line_start, bytes.len());

        // A trailing whitespace-only segment is indentation of a line that
        // never starts, so it is dropped. A whitespace-only input keeps one line.
        if offset.first_nonspace < offset.line_end || result.is_empty() {
            result.push(offset);
        }
    } else if result.is_empty() {
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
    fn drops_trailing_whitespace_only_segment() {
        assert_eq!(
            compact(&build_line_offsets("<style\n  ")),
            vec![(0, 6, 0, 0)]
        );
        assert_eq!(
            compact(&build_line_offsets("a\n\t")),
            vec![(0, 1, 0, 0)]
        );
        // Whitespace-only tail after content is dropped; alone it keeps one line.
        assert_eq!(compact(&build_line_offsets("   ")), vec![(0, 3, 3, 3)]);
        assert_eq!(compact(&build_line_offsets("  \n  ")), vec![(0, 2, 2, 2)]);
        // Interior whitespace-only lines are preserved.
        assert_eq!(
            compact(&build_line_offsets("a\n  \nb")),
            vec![(0, 1, 0, 0), (2, 4, 4, 2), (5, 6, 5, 0)]
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
