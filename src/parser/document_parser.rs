//! Transitional direct-to-arena parser support.

use std::borrow::Cow;
use std::fmt;
use std::ops::Range;
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
use crate::parser::extset::InlineRootExtSet;
use crate::parser::inline::{
    DelimiterRun,
    DocumentRuleSet,
    InlineProbeContext,
    Text,
    scan_delimiter_run,
};
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
        inline_rules: DocumentRuleSet,
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
    inline_ruleset: &'a DocumentRuleSet,
}

impl<'a> DocumentBlockState<'a> {
    fn new(
        src: &'a str,
        md: &'a MarkdownIt,
        rules: Vec<DocumentBlockRuleFns>,
        inline_ruleset: &'a DocumentRuleSet,
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
            inline_ruleset,
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
                    self.inline_ruleset,
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
        DocumentInlineState::parse(source, mapping, self.md, self.inline_ruleset)
    }

    fn get_map(&self, start_line: usize, end_line: usize) -> Option<SourcePos> {
        Some(SourcePos::new(
            self.line_offsets[start_line].first_nonspace,
            self.line_offsets[end_line].line_end,
        ))
    }
}

pub struct DocumentInlineState<'a> {
    pub(crate) src: Cow<'a, str>,
    pub(crate) pos: usize,
    pub(crate) pos_max: usize,
    md: &'a MarkdownIt,
    mapping: Cow<'a, [(usize, usize)]>,
    depth: u32,
    pub(crate) inline_ext: InlineRootExtSet,
    pub(crate) link_level: i32,
    ruleset: &'a DocumentRuleSet,
    nodes: Vec<NodeDraft>,
    pending_text: Option<(usize, usize)>,
}

impl<'a> DocumentInlineState<'a> {
    /// The unconsumed inline source visible to the current rule.
    pub fn remaining(&self) -> &str {
        &self.src[self.pos..self.pos_max]
    }

    /// Parser instance that owns this inline rule.
    pub fn markdown_it(&self) -> &MarkdownIt {
        self.md
    }

    pub(crate) fn nodes(&self) -> &[NodeDraft] {
        &self.nodes
    }

    pub(crate) fn nodes_mut(&mut self) -> &mut Vec<NodeDraft> {
        &mut self.nodes
    }

    fn parse(
        src: String,
        mapping: Vec<(usize, usize)>,
        md: &'a MarkdownIt,
        ruleset: &'a DocumentRuleSet,
    ) -> Vec<NodeDraft> {
        let mut state = Self {
            pos: 0,
            pos_max: src.len(),
            src: Cow::Owned(src),
            md,
            mapping: Cow::Owned(mapping),
            depth: 0,
            inline_ext: InlineRootExtSet::new(),
            link_level: 0,
            ruleset,
            nodes: Vec::new(),
            pending_text: None,
        };
        state.trim();
        state.tokenize();
        state.finish()
    }

    /// Parse a byte range relative to `remaining()` as independent children.
    ///
    /// Preserves whitespace and original source coordinates. Returns `None` for
    /// reversed, out-of-bounds, or non-UTF-8-boundary ranges. An empty valid range
    /// returns an empty vector. Parent state is unchanged. At the nesting limit,
    /// the child range is emitted as literal text without running rules.
    pub fn parse_subrange(&self, range: Range<usize>) -> Option<Vec<NodeDraft>> {
        self.remaining().get(range.clone())?;
        let start = self.pos + range.start;
        let end = self.pos + range.end;
        let mut child = DocumentInlineState {
            src: Cow::Borrowed(self.src.as_ref()),
            pos: start,
            pos_max: end,
            md: self.md,
            mapping: Cow::Borrowed(self.mapping.as_ref()),
            depth: self.depth.saturating_add(1),
            inline_ext: InlineRootExtSet::new(),
            link_level: self.link_level,
            ruleset: self.ruleset,
            nodes: Vec::new(),
            pending_text: None,
        };
        stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
            child.tokenize();
            child.finish_nodes();
        });
        Some(child.nodes)
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
        if self.depth >= self.md.max_nesting {
            if self.pos < self.pos_max {
                self.push_text(self.pos, self.pos_max);
                self.pos = self.pos_max;
            }
            return;
        }

        while self.pos < self.pos_max {
            let mut matched = None;
            for rule in &self.ruleset.runs {
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

    /// Start an independent probe session for `range` relative to
    /// [`Self::remaining`].
    ///
    /// The returned context owns its cursor and scratch storage; the parent
    /// state is left untouched, including pending text and inline extensions.
    /// Ranges that are reversed, out of bounds, or not on UTF-8 boundaries
    /// return `None`. Unsupported syntax is reported later by
    /// [`InlineProbeContext::next_token`].
    pub fn probe_subrange(&self, range: Range<usize>) -> Option<InlineProbeContext<'_>> {
        self.remaining().get(range.clone())?;
        Some(InlineProbeContext::new(
            self.src.as_ref(),
            self.pos + range.start,
            self.pos + range.end,
            self.md,
            self.ruleset,
            self.depth.saturating_add(1),
            self.link_level,
        ))
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

    pub(crate) fn flush_text(&mut self) {
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
                let mut content = self.src.into_owned();
                content.truncate(end);

                if start != 0 {
                    content.drain(..start);
                }

                let mut text = NodeDraft::new(Text { content });
                text.set_srcmap(srcmap);
                self.nodes.push(text);
            }
        } else {
            self.finish_nodes();
        }
        self.nodes
    }

    fn finish_nodes(&mut self) {
        let needs_finalization = !self.nodes().is_empty();
        self.flush_text();

        if needs_finalization {
            for index in 0..self.ruleset.finalizers.len() {
                let finalize = self.ruleset.finalizers[index];
                finalize(self);
            }
        }
    }

    fn source_pos(&self, pos: usize) -> usize {
        let line = match self.mapping.binary_search_by(|entry| entry.0.cmp(&pos)) {
            Ok(index) => index,
            Err(index) => index - 1,
        };
        self.mapping[line].1 + (pos - self.mapping[line].0)
    }

    pub(crate) fn get_map(&self, start: usize, end: usize) -> Option<SourcePos> {
        Some(SourcePos::new(self.source_pos(start), self.source_pos(end)))
    }

    pub(crate) fn scan_delims(&self, start: usize, can_split_word: bool) -> DelimiterRun {
        scan_delimiter_run(self.md, &self.src, start, self.pos_max, can_split_word)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::inline::{
        DocumentProbeFn,
        DocumentRuleFns,
        InlineProbeError,
        InlineProbeKind,
        InlineProbeResult,
        InlineProbeToken,
    };

    fn probe_rule(probe: DocumentProbeFn) -> DocumentRuleFns {
        DocumentRuleFns {
            run: panic_run,
            probe,
            marker: '\0',
        }
    }

    fn panic_run(_: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
        unreachable!("rule run must not be called")
    }

    #[derive(Debug, Default)]
    struct FinalizerCalls(usize);

    fn count_finalizer(state: &mut DocumentInlineState<'_>) {
        assert!(state.pending_text.is_none());
        state.inline_ext.get_or_insert_default::<FinalizerCalls>().0 += 1;
    }

    fn parent_state<'a>(
        md: &'a MarkdownIt,
        ruleset: &'a DocumentRuleSet,
    ) -> DocumentInlineState<'a> {
        let mut inline_ext = InlineRootExtSet::new();
        inline_ext.insert(FinalizerCalls(41));
        DocumentInlineState {
            src: Cow::Owned("前{ 雪\n次 }尾".to_owned()),
            pos: 3,
            pos_max: 14,
            md,
            mapping: Cow::Owned(vec![(0, 10), (9, 30)]),
            depth: 0,
            inline_ext,
            link_level: 2,
            ruleset,
            nodes: vec![NodeDraft::new(Text {
                content: "sentinel".into(),
            })],
            pending_text: Some((0, 3)),
        }
    }

    /// Inline state with a small source and no pending text, for probe tests.
    fn probe_state<'a>(
        md: &'a MarkdownIt,
        ruleset: &'a DocumentRuleSet,
        source: &str,
    ) -> DocumentInlineState<'a> {
        DocumentInlineState {
            src: Cow::Owned(source.to_owned()),
            pos: 0,
            pos_max: source.len(),
            md,
            mapping: Cow::Owned(vec![(0, 0)]),
            depth: 0,
            inline_ext: InlineRootExtSet::new(),
            link_level: 0,
            ruleset,
            nodes: vec![],
            pending_text: None,
        }
    }

    #[test]
    fn finishing_nodes_preserves_source_and_byte_mapping() {
        let md = MarkdownIt::empty();
        let ruleset = DocumentRuleSet {
            runs: vec![],
            probes: vec![],
            finalizers: vec![count_finalizer],
        };
        let mut state = DocumentInlineState {
            src: Cow::Owned("前x雪尾".to_owned()),
            pos: 7,
            pos_max: 7,
            md: &md,
            mapping: Cow::Owned(vec![(0, 10)]),
            depth: 0,
            inline_ext: InlineRootExtSet::new(),
            link_level: 1,
            ruleset: &ruleset,
            nodes: vec![NodeDraft::new(Text {
                content: "x".to_owned(),
            })],
            pending_text: Some((4, 7)),
        };

        state.finish_nodes();

        assert_eq!(state.src, "前x雪尾");
        assert_eq!(state.mapping.as_ref(), &[(0, 10)]);
        assert_eq!((state.pos, state.pos_max, state.link_level), (7, 7, 1));
        assert_eq!(state.nodes.len(), 2);
        assert_eq!(state.nodes[1].cast::<Text>().unwrap().content, "雪");
        assert_eq!(state.nodes[1].srcmap(), Some(SourcePos::new(14, 17)));
        assert_eq!(state.inline_ext.get::<FinalizerCalls>().unwrap().0, 1);
    }

    #[test]
    fn top_level_plain_text_still_skips_finalizers() {
        let md = MarkdownIt::empty();
        let ruleset = DocumentRuleSet {
            runs: vec![],
            probes: vec![],
            finalizers: vec![|_| panic!("plain pending text must skip finalizers")],
        };
        let nodes = DocumentInlineState::parse(" plain ".to_owned(), vec![(0, 0)], &md, &ruleset);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].cast::<Text>().unwrap().content, "plain");
        assert_eq!(nodes[0].srcmap(), Some(SourcePos::new(1, 6)));
    }

    #[test]
    fn subrange_preserves_parent_whitespace_and_multiline_mapping() {
        let md = MarkdownIt::empty();
        let ruleset = DocumentRuleSet {
            runs: vec![],
            probes: vec![],
            finalizers: vec![count_finalizer],
        };
        let state = parent_state(&md, &ruleset);
        let nodes = state.parse_subrange(1..10).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].cast::<Text>().unwrap().content, " 雪\n次 ");
        assert_eq!(nodes[0].srcmap(), Some(SourcePos::new(14, 34)));
        assert_eq!(state.src, "前{ 雪\n次 }尾");
        assert_eq!(state.mapping.as_ref(), &[(0, 10), (9, 30)]);
        assert_eq!(
            (state.pos, state.pos_max, state.depth, state.link_level),
            (3, 14, 0, 2)
        );
        assert_eq!(state.pending_text, Some((0, 3)));
        assert_eq!(state.nodes.len(), 1);
        assert_eq!(state.nodes[0].cast::<Text>().unwrap().content, "sentinel");
        assert_eq!(state.inline_ext.get::<FinalizerCalls>().unwrap().0, 41);
    }

    #[test]
    fn subrange_validates_ranges_and_empty_output() {
        let md = MarkdownIt::empty();
        let ruleset = DocumentRuleSet {
            runs: vec![],
            probes: vec![],
            finalizers: vec![|_| panic!("empty/plain range")],
        };
        let state = parent_state(&md, &ruleset);
        for range in [0..0, 11..11] {
            assert!(state.parse_subrange(range).unwrap().is_empty());
        }
        let reversed = Range { start: 2, end: 1 };
        for range in [reversed, 0..12, 0..usize::MAX, 3..4, 3..3] {
            assert!(state.parse_subrange(range).is_none());
        }
        assert_eq!(state.pending_text, Some((0, 3)));
        assert_eq!(state.inline_ext.get::<FinalizerCalls>().unwrap().0, 41);
    }

    #[test]
    fn child_finalizer_sees_only_child_nodes_and_scratch() {
        fn emit(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
            assert_eq!(state.depth, 1);
            assert_eq!(state.link_level, 2);
            assert!(state.inline_ext.get::<FinalizerCalls>().is_none());
            state.inline_ext.insert(FinalizerCalls(0));
            let content = state.remaining().to_owned();
            Some((
                Some(NodeDraft::new(Text { content })),
                state.remaining().len(),
            ))
        }
        fn finalize(state: &mut DocumentInlineState<'_>) {
            assert!(state.pending_text.is_none());
            assert_eq!(state.nodes.len(), 1);
            assert_eq!(state.inline_ext.get::<FinalizerCalls>().unwrap().0, 0);
            state.inline_ext.get_mut::<FinalizerCalls>().unwrap().0 += 1;
            state.nodes[0].cast_mut::<Text>().unwrap().content.push('!');
            state.link_level = 99;
        }
        let md = MarkdownIt::empty();
        let ruleset = DocumentRuleSet {
            runs: vec![emit],
            probes: vec![],
            finalizers: vec![finalize],
        };
        let state = parent_state(&md, &ruleset);
        for _ in 0..2 {
            let nodes = state.parse_subrange(1..10).unwrap();
            assert_eq!(nodes[0].cast::<Text>().unwrap().content, " 雪\n次 !");
        }
        assert_eq!(state.inline_ext.get::<FinalizerCalls>().unwrap().0, 41);
        assert_eq!(state.link_level, 2);
    }

    #[test]
    fn subrange_at_nesting_limit_emits_literal_text() {
        let mut md = MarkdownIt::empty();
        md.max_nesting = 1;
        let ruleset = DocumentRuleSet {
            runs: vec![|_| panic!("nesting limit must skip rules")],
            probes: vec![],
            finalizers: vec![|_| panic!("literal text must skip finalizers")],
        };
        let state = parent_state(&md, &ruleset);
        let nodes = state.parse_subrange(0..11).unwrap();
        assert_eq!(nodes[0].cast::<Text>().unwrap().content, "{ 雪\n次 }");
    }

    #[test]
    fn normal_parse_does_not_call_probe() {
        fn emit(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
            let content = state.remaining().to_owned();
            Some((
                Some(NodeDraft::new(Text { content })),
                state.remaining().len(),
            ))
        }
        let md = MarkdownIt::empty();
        let ruleset = DocumentRuleSet {
            runs: vec![emit],
            probes: vec![probe_rule(|_| panic!("normal parsing must not call probe"))],
            finalizers: vec![],
        };
        let nodes = DocumentInlineState::parse("x".to_owned(), vec![(0, 0)], &md, &ruleset);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].cast::<Text>().unwrap().content, "x");
    }

    #[test]
    fn probe_advances_pending_text_between_tokens() {
        fn probe_sequence(context: &mut InlineProbeContext<'_>) -> InlineProbeResult {
            let remaining = context.remaining();
            if remaining.starts_with('a') {
                InlineProbeResult::NoMatch
            } else if remaining.starts_with('b') {
                assert_eq!(context.trailing_text(), "a");
                InlineProbeResult::Match {
                    len: 1,
                    kind: InlineProbeKind::Token,
                }
            } else if remaining.starts_with('c') {
                assert_eq!(context.trailing_text(), "");
                InlineProbeResult::Match {
                    len: 1,
                    kind: InlineProbeKind::Text,
                }
            } else {
                InlineProbeResult::NoMatch
            }
        }
        let md = MarkdownIt::empty();
        let ruleset = DocumentRuleSet {
            runs: vec![],
            probes: vec![probe_rule(probe_sequence)],
            finalizers: vec![],
        };
        let state = probe_state(&md, &ruleset, "abc");
        let mut context = state.probe_subrange(0..3).unwrap();

        assert_eq!(
            context.next_token().unwrap(),
            Some(InlineProbeToken {
                range: 0..1,
                kind: InlineProbeKind::Text,
            })
        );
        assert_eq!(
            context.next_token().unwrap(),
            Some(InlineProbeToken {
                range: 1..2,
                kind: InlineProbeKind::Token,
            })
        );
        assert_eq!(
            context.next_token().unwrap(),
            Some(InlineProbeToken {
                range: 2..3,
                kind: InlineProbeKind::Text,
            })
        );
        assert_eq!(context.next_token().unwrap(), None);
        assert_eq!(context.trailing_text(), "c");

        assert_eq!(state.pending_text, None);
        assert_eq!((state.pos, state.pos_max, state.link_level), (0, 3, 0));
    }

    #[test]
    fn probe_stops_at_nesting_limit() {
        let mut md = MarkdownIt::empty();
        md.max_nesting = 1;
        let ruleset = DocumentRuleSet {
            runs: vec![],
            probes: vec![probe_rule(|_| panic!("nesting limit must skip probes"))],
            finalizers: vec![],
        };
        let state = probe_state(&md, &ruleset, "abc");
        let mut context = state.probe_subrange(0..3).unwrap();

        assert_eq!(
            context.next_token().unwrap(),
            Some(InlineProbeToken {
                range: 0..3,
                kind: InlineProbeKind::Text,
            })
        );
        assert_eq!(context.next_token().unwrap(), None);
    }

    #[test]
    fn probe_unsupported_rule_stops_dispatch_with_sticky_error() {
        let md = MarkdownIt::empty();
        let ruleset = DocumentRuleSet {
            runs: vec![],
            probes: vec![
                DocumentRuleFns {
                    run: panic_run,
                    probe: |_| InlineProbeResult::Unsupported,
                    marker: 'x',
                },
                DocumentRuleFns {
                    run: panic_run,
                    probe: |_| panic!("lower-priority probe must not run"),
                    marker: 'x',
                },
            ],
            finalizers: vec![],
        };
        let state = probe_state(&md, &ruleset, "x");
        let mut context = state.probe_subrange(0..1).unwrap();
        let error = InlineProbeError::UnsupportedRule {
            rule_index: 0,
            marker: 'x',
        };

        assert_eq!(context.next_token(), Err(error));
        assert_eq!(context.next_token(), Err(error));
        assert_eq!(context.remaining(), "x");
    }

    #[test]
    fn probe_skips_unsupported_rules_with_other_markers() {
        let md = MarkdownIt::empty();
        let ruleset = DocumentRuleSet {
            runs: vec![],
            probes: vec![DocumentRuleFns {
                run: panic_run,
                probe: |_| panic!("marker must filter probe dispatch"),
                marker: 'y',
            }],
            finalizers: vec![],
        };
        let state = probe_state(&md, &ruleset, "x");
        let mut context = state.probe_subrange(0..1).unwrap();

        assert_eq!(
            context.next_token().unwrap(),
            Some(InlineProbeToken {
                range: 0..1,
                kind: InlineProbeKind::Text,
            })
        );
        assert_eq!(context.next_token().unwrap(), None);
    }

    #[test]
    fn probe_rejects_invalid_lengths() {
        fn assert_invalid(source: &str, len: usize, probe: DocumentProbeFn) {
            let md = MarkdownIt::empty();
            let ruleset = DocumentRuleSet {
                runs: vec![],
                probes: vec![DocumentRuleFns {
                    run: panic_run,
                    probe,
                    marker: '\0',
                }],
                finalizers: vec![],
            };
            let state = probe_state(&md, &ruleset, source);
            let mut context = state.probe_subrange(0..source.len()).unwrap();
            let error = InlineProbeError::InvalidLength { rule_index: 0, len };

            assert_eq!(context.next_token(), Err(error));
            assert_eq!(context.next_token(), Err(error));
            assert_eq!(context.remaining(), source);
        }

        assert_invalid("x", 0, |_| InlineProbeResult::Match {
            len: 0,
            kind: InlineProbeKind::Text,
        });
        assert_invalid("x", 2, |_| InlineProbeResult::Match {
            len: 2,
            kind: InlineProbeKind::Text,
        });
        assert_invalid("éx", 1, |_| InlineProbeResult::Match {
            len: 1,
            kind: InlineProbeKind::Text,
        });
    }

    #[test]
    fn probe_subrange_validates_ranges_and_empty_output() {
        let md = MarkdownIt::empty();
        let ruleset = DocumentRuleSet {
            runs: vec![],
            probes: vec![],
            finalizers: vec![],
        };
        let state = probe_state(&md, &ruleset, "éx");

        for range in [0..0, 0..2, 0..3] {
            assert!(state.probe_subrange(range).is_some());
        }
        for range in [1..2, 0..4, Range { start: 2, end: 1 }] {
            assert!(state.probe_subrange(range).is_none());
        }

        let mut empty = state.probe_subrange(0..0).unwrap();
        assert_eq!(empty.next_token().unwrap(), None);

        let mut full = state.probe_subrange(0..3).unwrap();
        assert_eq!(
            full.next_token().unwrap(),
            Some(InlineProbeToken {
                range: 0..2,
                kind: InlineProbeKind::Text,
            })
        );
        assert_eq!(
            full.next_token().unwrap(),
            Some(InlineProbeToken {
                range: 2..3,
                kind: InlineProbeKind::Text,
            })
        );
        assert_eq!(full.next_token().unwrap(), None);
    }

    #[test]
    fn probe_subrange_uses_relative_ranges_and_keeps_parent_state() {
        let md = MarkdownIt::empty();
        let ruleset = DocumentRuleSet {
            runs: vec![],
            probes: vec![],
            finalizers: vec![],
        };
        let state = parent_state(&md, &ruleset);
        let mut context = state.probe_subrange(0..11).unwrap();

        assert_eq!(context.depth(), 1);
        assert_eq!(context.link_level(), 2);
        assert_eq!(context.remaining(), "{ 雪\n次 }");
        assert_eq!(context.markdown_it().max_nesting, md.max_nesting);
        assert_eq!(
            context.next_token().unwrap(),
            Some(InlineProbeToken {
                range: 0..1,
                kind: InlineProbeKind::Text,
            })
        );
        assert_eq!(
            context.next_token().unwrap(),
            Some(InlineProbeToken {
                range: 1..2,
                kind: InlineProbeKind::Text,
            })
        );

        assert_eq!(state.pending_text, Some((0, 3)));
        assert_eq!(
            (state.pos, state.pos_max, state.depth, state.link_level),
            (3, 14, 0, 2)
        );
        assert_eq!(state.nodes.len(), 1);
        assert_eq!(state.inline_ext.get::<FinalizerCalls>().unwrap().0, 41);
    }

    #[test]
    fn probe_code_pair_cache_is_private_to_each_session() {
        use crate::generics::inline::code_pair::CodePairScanner;

        let mut md = MarkdownIt::empty();
        md.inline.add_migrated_rule::<CodePairScanner<'`'>>();
        let ruleset = md.inline.document_rules().unwrap();
        let state = probe_state(&md, &ruleset, "`x`");

        let mut short = state.probe_subrange(0..2).unwrap();
        assert_eq!(
            short.next_token().unwrap(),
            Some(InlineProbeToken {
                range: 0..1,
                kind: InlineProbeKind::Text,
            })
        );
        assert_eq!(
            short.next_token().unwrap(),
            Some(InlineProbeToken {
                range: 1..2,
                kind: InlineProbeKind::Text,
            })
        );
        assert_eq!(short.next_token().unwrap(), None);

        let mut long = state.probe_subrange(0..3).unwrap();
        assert_eq!(
            long.next_token().unwrap(),
            Some(InlineProbeToken {
                range: 0..3,
                kind: InlineProbeKind::Token,
            })
        );
        assert_eq!(long.next_token().unwrap(), None);

        assert_eq!(state.pending_text, None);
        assert_eq!((state.pos, state.pos_max, state.link_level), (0, 3, 0));
    }

    #[test]
    fn probe_dispatch_keeps_wildcard_position() {
        let md = MarkdownIt::empty();
        let ruleset = DocumentRuleSet {
            runs: vec![],
            probes: vec![
                probe_rule(|_| InlineProbeResult::Match {
                    len: 1,
                    kind: InlineProbeKind::Token,
                }),
                DocumentRuleFns {
                    run: panic_run,
                    probe: |_| panic!("specific rule must not run after a wildcard match"),
                    marker: 'x',
                },
            ],
            finalizers: vec![],
        };
        let state = probe_state(&md, &ruleset, "x");
        let mut context = state.probe_subrange(0..1).unwrap();

        assert_eq!(
            context.next_token().unwrap(),
            Some(InlineProbeToken {
                range: 0..1,
                kind: InlineProbeKind::Token,
            })
        );
    }
}
