//! Transitional direct-to-arena parser support.

use std::fmt;
use std::sync::Arc;

use crate::common::sourcemap::SourcePos;
use crate::common::utils::calc_right_whitespace_with_tabstops;
use crate::parser::block::{
    DocumentRuleFns as DocumentBlockRuleFns,
    LineOffset,
    build_line_offsets,
};
use crate::parser::core::Root;
use crate::parser::document::{Document, NodeDraft};
use crate::parser::inline::{DocumentRuleFn as DocumentInlineRuleFn, Text};
use crate::parser::main::MarkdownIt;
use crate::parser::render_options::RenderOptions;

/// Error returned while a parser configuration still contains rules that
/// have not been migrated to the direct arena pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentParseError {
    UnsupportedConfiguration,
}

impl fmt::Display for DocumentParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedConfiguration => f.write_str(
                "parser configuration contains rules without direct arena implementations",
            ),
        }
    }
}

impl std::error::Error for DocumentParseError {}

pub(crate) struct DocumentParseContext<'a> {
    source: &'a str,
    root: NodeDraft,
}

impl<'a> DocumentParseContext<'a> {
    pub(crate) fn new(source: &'a str, options: &RenderOptions) -> Self {
        let mut root = NodeDraft::new(Root::new(source.to_owned()));
        root.set_srcmap(Some(SourcePos::new(0, source.len())));
        root.ext_mut().insert(options.clone());
        Self { source, root }
    }

    pub(crate) fn parse(
        mut self,
        md: &MarkdownIt,
        block_rules: Vec<DocumentBlockRuleFns>,
        inline_rules: Vec<DocumentInlineRuleFn>,
    ) -> Document {
        let mut state = DocumentBlockState::new(self.source, md, block_rules, &inline_rules);
        state.tokenize();
        *self.root.children_mut() = state.nodes;

        Document::from_draft(Arc::<str>::from(self.source), self.root)
    }

    pub(crate) fn parse_text_fallback(mut self) -> Document {
        for line in build_line_offsets(self.source) {
            if line.first_nonspace >= line.line_end {
                continue;
            }

            let mut content = self.source[line.first_nonspace..line.line_end].to_owned();
            content.push('\n');
            let mut text = NodeDraft::new(Text { content });
            text.set_srcmap(Some(SourcePos::new(line.first_nonspace, line.line_end + 1)));
            self.root.push_child(text);
        }

        Document::from_draft(Arc::<str>::from(self.source), self.root)
    }
}

pub(crate) struct DocumentBlockState<'a> {
    pub(crate) src: &'a str,
    pub(crate) md: &'a MarkdownIt,
    pub(crate) line_offsets: Vec<LineOffset>,
    pub(crate) line: usize,
    pub(crate) line_max: usize,
    pub(crate) blk_indent: usize,
    pub(crate) nodes: Vec<NodeDraft>,
    rules: Vec<DocumentBlockRuleFns>,
    inline_rules: &'a [DocumentInlineRuleFn],
}

impl<'a> DocumentBlockState<'a> {
    fn new(
        src: &'a str,
        md: &'a MarkdownIt,
        rules: Vec<DocumentBlockRuleFns>,
        inline_rules: &'a [DocumentInlineRuleFn],
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
            nodes: Vec::new(),
            rules,
            inline_rules,
        }
    }

    fn tokenize(&mut self) {
        while self.line < self.line_max {
            if self.md.max_nesting == 0 {
                break;
            }
            self.line = self.skip_empty_lines(self.line);
            if self.line >= self.line_max {
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

            if let Some((mut node, len)) = matched {
                self.line += len;
                node.set_srcmap(self.get_map(self.line - len, self.line - 1));
                self.nodes.push(node);
            } else {
                let start = self.line_offsets[self.line].first_nonspace;
                let mut content = self.get_line(self.line).to_owned();
                content.push('\n');
                let mapping = vec![(0, start)];
                self.nodes.extend(DocumentInlineState::parse(
                    content,
                    mapping,
                    self.md,
                    self.inline_rules,
                ));
                self.line += 1;
            }

            if self.line < self.line_max && self.is_empty(self.line) {
                self.line += 1;
            }
        }
    }

    pub(crate) fn test_rules_at_line(&mut self) -> bool {
        for index in 0..self.rules.len() {
            let check = self.rules[index].0;
            if check(self).is_some() {
                return true;
            }
        }
        false
    }

    pub(crate) fn is_empty(&self, line: usize) -> bool {
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

    pub(crate) fn line_indent(&self, line: usize) -> i32 {
        self.line_offsets.get(line).map_or(0, |offsets| {
            offsets.indent_nonspace - self.blk_indent as i32
        })
    }

    pub(crate) fn get_line(&self, line: usize) -> &str {
        let Some(offsets) = self.line_offsets.get(line) else {
            return "";
        };
        &self.src[offsets.first_nonspace..offsets.line_end]
    }

    pub(crate) fn get_lines(
        &self,
        begin: usize,
        end: usize,
        indent: usize,
        keep_last_lf: bool,
    ) -> (String, Vec<(usize, usize)>) {
        let mut result = String::new();
        let mut mapping = Vec::new();

        for line in begin..end {
            let offsets = &self.line_offsets[line];
            let add_last_lf = line + 1 < end || keep_last_lf;
            let (num_spaces, first) = calc_right_whitespace_with_tabstops(
                &self.src[offsets.line_start..offsets.first_nonspace],
                offsets.indent_nonspace - indent as i32,
            );

            mapping.push((result.len(), offsets.line_start + first));
            result += &" ".repeat(num_spaces);
            result += &self.src[offsets.line_start + first..offsets.line_end];
            if add_last_lf {
                result.push('\n');
            }
        }
        (result, mapping)
    }

    pub(crate) fn parse_inline(
        &self,
        source: String,
        mapping: Vec<(usize, usize)>,
    ) -> Vec<NodeDraft> {
        DocumentInlineState::parse(source, mapping, self.md, self.inline_rules)
    }

    fn get_map(&self, start_line: usize, end_line: usize) -> Option<SourcePos> {
        Some(SourcePos::new(
            self.line_offsets[start_line].first_nonspace,
            self.line_offsets[end_line].line_end,
        ))
    }
}

pub struct DocumentInlineState<'a> {
    pub(crate) src: String,
    pub(crate) pos: usize,
    pub(crate) pos_max: usize,
    md: &'a MarkdownIt,
    mapping: Vec<(usize, usize)>,
    rules: &'a [DocumentInlineRuleFn],
    nodes: Vec<NodeDraft>,
    pending_text: Option<(usize, usize)>,
}

impl<'a> DocumentInlineState<'a> {
    /// The unconsumed inline source visible to the current rule.
    pub fn remaining(&self) -> &str {
        &self.src[self.pos..self.pos_max]
    }

    fn parse(
        src: String,
        mapping: Vec<(usize, usize)>,
        md: &'a MarkdownIt,
        rules: &'a [DocumentInlineRuleFn],
    ) -> Vec<NodeDraft> {
        let mut state = Self {
            pos: 0,
            pos_max: src.len(),
            src,
            md,
            mapping,
            rules,
            nodes: Vec::new(),
            pending_text: None,
        };
        state.trim();
        state.tokenize();
        state.finish()
    }

    fn trim(&mut self) {
        let mut bytes = self.src.as_bytes().iter();
        while let Some(b' ' | b'\t') = bytes.next_back() {
            self.pos_max -= 1;
        }
        while let Some(b' ' | b'\t') = bytes.next() {
            self.pos += 1;
        }
    }

    fn tokenize(&mut self) {
        while self.pos < self.pos_max {
            let mut matched = None;
            for rule in self.rules {
                if let Some(result) = rule(self) {
                    matched = Some(result);
                    break;
                }
            }

            if let Some((node, len)) = matched {
                self.pos += len;
                if let Some(mut node) = node {
                    self.flush_text();
                    node.set_srcmap(self.get_map(self.pos - len, self.pos));
                    self.nodes.push(node);
                }
            } else {
                let ch = self.src[self.pos..self.pos_max].chars().next().unwrap();
                let len = ch.len_utf8();
                self.push_text(self.pos, self.pos + len);
                self.pos += len;
            }
        }
    }

    pub(crate) fn is_rule_marker(&self, marker: char) -> bool {
        self.md.inline.is_document_marker(marker)
    }

    pub(crate) fn trailing_text(&self) -> &str {
        self.pending_text
            .map_or("", |(start, end)| &self.src[start..end])
    }

    pub(crate) fn pop_trailing_text(&mut self, count: usize) {
        if count == 0 {
            return;
        }
        let (start, end) = self.pending_text.expect("trailing text must exist");
        assert!(count <= end - start && self.src.is_char_boundary(end - count));
        self.pending_text = (start < end - count).then_some((start, end - count));
    }

    pub(crate) fn push_text(&mut self, start: usize, end: usize) {
        match self.pending_text {
            Some((pending_start, pending_end)) if pending_end == start => {
                self.pending_text = Some((pending_start, end));
            }
            Some(_) => {
                self.flush_text();
                self.pending_text = Some((start, end));
            }
            None => self.pending_text = Some((start, end)),
        }
    }

    fn flush_text(&mut self) {
        let Some((start, end)) = self.pending_text.take() else {
            return;
        };
        let mut text = NodeDraft::new(Text {
            content: self.src[start..end].to_owned(),
        });
        text.set_srcmap(self.get_map(start, end));
        self.nodes.push(text);
    }

    fn finish(mut self) -> Vec<NodeDraft> {
        if self.nodes.is_empty() {
            if let Some((start, end)) = self.pending_text.take() {
                let srcmap = self.get_map(start, end);
                self.src.truncate(end);
                if start != 0 {
                    self.src.drain(..start);
                }
                let mut text = NodeDraft::new(Text { content: self.src });
                text.set_srcmap(srcmap);
                self.nodes.push(text);
            }
        } else {
            self.flush_text();
        }
        self.nodes
    }

    fn source_pos(&self, pos: usize) -> usize {
        let line = match self.mapping.binary_search_by(|entry| entry.0.cmp(&pos)) {
            Ok(index) => index,
            Err(index) => index - 1,
        };
        self.mapping[line].1 + (pos - self.mapping[line].0)
    }

    fn get_map(&self, start: usize, end: usize) -> Option<SourcePos> {
        Some(SourcePos::new(self.source_pos(start), self.source_pos(end)))
    }
}
