//! Block parsing state and execution.

use super::{BlockRuleFns, LineOffset, build_line_offsets};
use crate::MarkdownIt;
use crate::common::extset::RootExtSet;
use crate::document::{Document, NodeId};
use crate::parser::pipeline::PendingInline;

/// State passed to block rules while building block nodes directly.
pub struct BlockState<'a> {
    /// Markdown source.
    pub src: &'a str,

    /// Link to the parser instance.
    pub md: &'a MarkdownIt,

    /// Start/end/etc. positions for each source line.
    pub line_offsets: Vec<LineOffset>,

    /// Current line index.
    pub line: usize,

    /// Maximum allowed line index.
    pub line_max: usize,

    /// Current block content indent.
    pub blk_indent: usize,

    /// Current node, block rules add children to it.
    pub node: NodeId,

    /// Arena receiving parsed nodes.
    pub document: &'a mut Document,

    /// Whether there are no empty lines between paragraphs.
    pub tight: bool,

    /// Indent of the current list block.
    pub list_indent: Option<u32>,

    /// Current nesting level, incremented by recursive block rules.
    pub level: u32,

    /// Cross-block storage shared with inline parsing (e.g. link references).
    pub root_ext: RootExtSet,

    rules: &'a [BlockRuleFns],
}

impl<'a> BlockState<'a> {
    pub(in crate::parser) fn new(
        src: &'a str,
        md: &'a MarkdownIt,
        rules: &'a [BlockRuleFns],
        document: &'a mut Document,
    ) -> Self {
        let line_offsets = build_line_offsets(src);
        let line_max = line_offsets.len();
        Self {
            src,
            md,
            line_offsets,
            line: 0,
            line_max,
            blk_indent: 0,
            node: document.root(),
            document,
            tight: false,
            list_indent: None,
            level: 0,
            root_ext: RootExtSet::new(),
            rules,
        }
    }

    /// Generate tokens for input range.
    pub(in crate::parser) fn tokenize(&mut self) {
        stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
            let mut has_empty_lines = false;

            while self.line < self.line_max {
                self.line = self.skip_empty_lines(self.line);
                if self.line >= self.line_max {
                    break;
                }

                // Termination condition for nested calls, used by blockquotes & lists.
                if self.line_indent(self.line) < 0 {
                    break;
                }

                // If nesting level exceeded, skip the tail.
                if self.level >= self.md.max_nesting {
                    self.line = self.line_max;
                    break;
                }

                let mut matched = None;
                for index in 0..self.rules.len() {
                    let run = self.rules[index].1;
                    if let Some(result) = run(self) {
                        matched = Some(result);
                        break;
                    }
                }

                if let Some((node, len)) = matched {
                    self.line += len;
                    if let Some(node) = node {
                        #[cfg(feature = "sourcemap")]
                        {
                            let srcmap = self.get_map(self.line - len, self.line - 1);
                            self.document.node_mut(node).set_srcmap(srcmap);
                        }

                        self.document.push_child(self.node, node);
                    }
                } else {
                    #[cfg(feature = "sourcemap")]
                    let start = self.line_offsets[self.line].first_nonspace;
                    let mut content = self.get_line(self.line).to_owned();
                    content.push('\n');

                    #[cfg(feature = "sourcemap")]
                    let mapping = vec![(0, start)];
                    #[cfg(not(feature = "sourcemap"))]
                    let mapping = crate::parser::InlineMapping;

                    let pending = self.pending_inline(content, mapping);
                    self.document.push_child(self.node, pending);
                    self.line += 1;
                }

                // Set `tight` if we had an empty line before current tag.
                self.tight = !has_empty_lines;

                if self.is_empty(self.line - 1) {
                    has_empty_lines = true;
                }

                if self.line < self.line_max && self.is_empty(self.line) {
                    has_empty_lines = true;
                    self.line += 1;
                }
            }
        });
    }

    /// Tokenize the contents of a nested block container.
    ///
    /// Block rules that recursively invoke the block parser must use this
    /// method so [`MarkdownIt::max_nesting`] can stop excessively deep input.
    pub fn tokenize_nested(&mut self) {
        let old_level = self.level;
        self.level = self.level.saturating_add(1);
        self.tokenize();
        self.level = old_level;
    }

    /// Run every block rule's check at the current line without consuming it;
    /// returns `true` if any rule matches.
    pub fn test_rules_at_line(&mut self) -> bool {
        for index in 0..self.rules.len() {
            let check = self.rules[index].0;

            // debug checks
            #[cfg(debug_assertions)]
            let matched = crate::parser::validation::check_block(self, index, check);

            #[cfg(not(debug_assertions))]
            let matched = check(self);
            if matched.is_some() {
                return true;
            }
        }
        false
    }

    /// Whether the given line is empty.
    pub fn is_empty(&self, line: usize) -> bool {
        self.line_offsets
            .get(line)
            .is_some_and(|offsets| offsets.first_nonspace >= offsets.line_end)
    }

    fn skip_empty_lines(&self, from: usize) -> usize {
        let mut line = from;
        while line != self.line_max && self.is_empty(line) {
            line += 1;
        }
        line
    }

    /// Return the indent of a specific line, taking blockquotes and lists into
    /// account; it may be negative if the text is less indented than the
    /// current list item.
    pub fn line_indent(&self, line: usize) -> i32 {
        self.line_offsets.get(line).map_or(0, |offsets| {
            offsets.indent_nonspace - self.blk_indent as i32
        })
    }

    /// Return a single line, trimming initial spaces.
    pub fn get_line(&self, line: usize) -> &str {
        let Some(offsets) = self.line_offsets.get(line) else {
            return "";
        };
        &self.src[offsets.first_nonspace..offsets.line_end]
    }

    /// Cut the range of lines `begin..end` (excluding `end`) from the source
    /// without preceding indent.
    ///
    /// Returns the lines plus a mapping from the start of each result line to
    /// the start of each source line.
    pub fn get_lines(
        &self,
        begin: usize,
        end: usize,
        indent: usize,
        keep_last_lf: bool,
    ) -> (String, crate::parser::InlineMapping) {
        let mut result = String::new();
        #[cfg(feature = "sourcemap")]
        let mut mapping = crate::parser::InlineMapping::default();
        #[cfg(not(feature = "sourcemap"))]
        let mapping = crate::parser::InlineMapping;

        for line in begin..end {
            let offsets = &self.line_offsets[line];
            let add_last_lf = line + 1 < end || keep_last_lf;

            // Strip the indent, expanding tabs with the container's tab stops;
            // a cut inside a tab is re-added as spaces.
            let target = indent as i32;
            let mut line_indent = 0i32;
            let mut first = offsets.content_start;

            while first < offsets.line_end && line_indent < target {
                match self.src.as_bytes()[first] {
                    b' ' => line_indent += 1,
                    b'\t' => line_indent += 4 - (line_indent + offsets.tab_offset) % 4,
                    // Consumed list markers count as whitespace.
                    _ if first < offsets.first_nonspace => line_indent += 1,
                    _ => break,
                }
                first += 1;
            }

            #[cfg(feature = "sourcemap")]
            mapping.push((result.len(), first));

            if line_indent > target {
                result += &" ".repeat((line_indent - target) as usize);
            }
            result += &self.src[first..offsets.line_end];
            if add_last_lf {
                result.push('\n');
            }
        }
        (result, mapping)
    }

    /// Create a placeholder node for inline content parsed after the block pass.
    pub fn pending_inline(
        &mut self,
        source: String,
        mapping: crate::parser::InlineMapping,
    ) -> NodeId {
        self.document.create_node(PendingInline {
            content: source,
            mapping,
        })
    }

    /// Return the source span covering lines `start_line..=end_line`.
    #[must_use]
    #[cfg(feature = "sourcemap")]
    pub fn get_map(
        &self,
        start_line: usize,
        end_line: usize,
    ) -> Option<crate::common::sourcemap::SourcePos> {
        debug_assert!(start_line <= end_line);

        Some(crate::common::sourcemap::SourcePos::new(
            self.line_offsets[start_line].first_nonspace,
            self.line_offsets[end_line].line_end,
        ))
    }
}

#[cfg(test)]
mod tests;
