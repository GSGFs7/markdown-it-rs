//! Document parsing through transient drafts and arena storage.

use std::borrow::Cow;
use std::ops::Range;
use std::sync::Arc;

use crate::common::sourcemap::SourcePos;
use crate::common::utils::calc_right_whitespace_with_tabstops;
use crate::document::{Document, NodeDraft};
use crate::parser::block::{BlockRuleFns, LineOffset, build_line_offsets};
use crate::parser::core::{
    DocumentCoreRule,
    DocumentFinalizeDraftFn,
    DocumentPrepareStateFn,
    Root,
};
use crate::parser::extset::{InlineRootExtSet, RootExtSet};
use crate::parser::inline::probe::InlineProbeContext;
use crate::parser::inline::{DelimiterRun, DocumentRuleSet, Text, scan_delimiter_run};
use crate::parser::main::MarkdownIt;
use crate::parser::node::{ConsumeOnly, NodeValue};

/// Inline content queued during the block pass and resolved once all
/// reference definitions have been collected.
#[derive(Debug)]
struct PendingInline {
    content: String,
    mapping: Vec<(usize, usize)>,
}

impl NodeValue for PendingInline {}

/// Replace every [`PendingInline`] draft in `draft` with parsed inline nodes.
fn resolve_pending_inline(
    draft: &mut NodeDraft,
    md: &MarkdownIt,
    ruleset: &DocumentRuleSet,
    root_ext: &RootExtSet,
) {
    let children = std::mem::take(draft.children_mut());
    draft.children_mut().reserve(children.len());
    for mut child in children {
        if let Some(pending) = child.cast_mut::<PendingInline>() {
            let content = std::mem::take(&mut pending.content);
            let mapping = std::mem::take(&mut pending.mapping);
            draft.children_mut().extend(DocumentInlineState::parse(
                content,
                mapping,
                md,
                ruleset,
                Some(root_ext),
            ));
        } else {
            stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
                resolve_pending_inline(&mut child, md, ruleset, root_ext);
            });
            draft.push_child(child);
        }
    }
}

pub(crate) struct DocumentParseContext<'a> {
    source: Arc<str>,
    md: &'a MarkdownIt,
    root: NodeDraft,
    root_ext: RootExtSet,
    inline_preparations: Vec<DocumentPrepareStateFn>,
    draft_finalizers: Vec<DocumentFinalizeDraftFn>,
}

impl<'a> DocumentParseContext<'a> {
    pub(crate) fn new(source: &str, md: &'a MarkdownIt) -> Self {
        let source: Arc<str> = Arc::from(source);
        let mut root = NodeDraft::new(Root::new(Arc::clone(&source)));
        root.set_srcmap(Some(SourcePos::new(0, source.len())));
        root.ext_mut().insert(md.render_options.clone());
        Self {
            source,
            md,
            root,
            root_ext: RootExtSet::new(),
            inline_preparations: Vec::new(),
            draft_finalizers: Vec::new(),
        }
    }

    pub(crate) fn parse(mut self) -> Document {
        let mut preparations = Vec::new();
        let mut seen_block = false;
        let mut seen_inline = false;
        let mut supported = true;
        for rule in self.md.document_core_rules() {
            match rule {
                DocumentCoreRule::ParseBlocks => {
                    supported &= !seen_block && !seen_inline;
                    seen_block = true;
                }
                DocumentCoreRule::ParseInlines => {
                    supported &= seen_block && !seen_inline;
                    seen_inline = true;
                }
                DocumentCoreRule::PrepareState(prepare) => {
                    // Preserve whether source analysis runs before or after blocks.
                    supported &= !seen_inline;
                    if seen_block {
                        self.inline_preparations.push(prepare);
                    } else {
                        preparations.push(prepare);
                    }
                }
                DocumentCoreRule::FinalizeDraft(finalize) => {
                    // Finalizers must follow the inline pass.
                    supported &= seen_inline;
                    self.draft_finalizers.push(finalize);
                }
            }
        }
        assert!(
            supported && seen_block && seen_inline,
            "direct parsing requires exactly one block stage followed by exactly one inline stage, supported core rules, preparations before inlines, and draft finalizers after inlines",
        );

        let block_rules = self.md.block.document_rules();
        let inline_rules = self.md.inline.document_rules();

        for prepare in preparations {
            prepare(&self.source, self.md, &mut self.root_ext);
        }
        if block_rules.is_empty() && self.md.inline.has_only_text_rule() && self.md.max_nesting > 0
        {
            self.prepare_inlines();
            return self.parse_text_fallback();
        }

        let mut state = DocumentBlockState::new(&self.source, self.md, block_rules, self.root);
        state.root_ext = self.root_ext;
        state.tokenize();
        self.root = state.node;
        self.root_ext = state.root_ext;
        self.prepare_inlines();

        // Inline parsing runs after the block pass so later reference
        // definitions can resolve earlier uses.
        resolve_pending_inline(&mut self.root, self.md, &inline_rules, &self.root_ext);
        self.finish()
    }

    fn prepare_inlines(&mut self) {
        for prepare in &self.inline_preparations {
            prepare(&self.source, self.md, &mut self.root_ext);
        }
    }

    fn finish(mut self) -> Document {
        // Post-inline core rules may now reorder the resolved draft.
        for finalize in self.draft_finalizers {
            finalize(&mut self.root, &self.root_ext);
        }
        // Persist the cross-block extension set on the root payload.
        if let Some(data) = self.root.cast_mut::<Root>() {
            data.ext = self.root_ext;
        }

        let mut document = Document::from_draft(self.source, self.root);
        self.md.run_document_transforms(&mut document);
        document
    }

    fn parse_text_fallback(mut self) -> Document {
        for line in build_line_offsets(&self.source) {
            if line.first_nonspace >= line.line_end {
                continue;
            }

            let mut content = self.source[line.first_nonspace..line.line_end].to_owned();
            content.push('\n');
            let mut text = NodeDraft::new(Text { content });
            text.set_srcmap(Some(SourcePos::new(line.first_nonspace, line.line_end + 1)));
            self.root.push_child(text);
        }

        self.finish()
    }
}

/// State passed to block rules while building the block draft tree.
pub struct DocumentBlockState<'a> {
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
    pub node: NodeDraft,

    /// Whether there are no empty lines between paragraphs.
    pub tight: bool,

    /// Indent of the current list block.
    pub list_indent: Option<u32>,

    /// Current nesting level, incremented by recursive block rules.
    pub level: u32,

    /// Cross-block storage shared with inline parsing (e.g. link references).
    pub root_ext: RootExtSet,

    rules: Vec<BlockRuleFns>,
}

impl<'a> DocumentBlockState<'a> {
    fn new(src: &'a str, md: &'a MarkdownIt, rules: Vec<BlockRuleFns>, node: NodeDraft) -> Self {
        let line_offsets = build_line_offsets(src);
        let line_max = line_offsets.len();
        Self {
            src,
            md,
            line_offsets,
            line: 0,
            line_max,
            blk_indent: 0,
            node,
            tight: false,
            list_indent: None,
            level: 0,
            root_ext: RootExtSet::new(),
            rules,
        }
    }

    /// Generate tokens for input range.
    fn tokenize(&mut self) {
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

                if let Some((mut node, len)) = matched {
                    self.line += len;
                    if !node.is::<ConsumeOnly>() {
                        node.set_srcmap(self.get_map(self.line - len, self.line - 1));
                        self.node.push_child(node);
                    }
                } else {
                    let start = self.line_offsets[self.line].first_nonspace;
                    let mut content = self.get_line(self.line).to_owned();
                    content.push('\n');
                    let mapping = vec![(0, start)];
                    let pending = self.pending_inline(content, mapping);
                    self.node.push_child(pending);
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
            if check(self).is_some() {
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

    /// Create a placeholder node for inline content parsed after the block pass.
    pub fn pending_inline(&self, source: String, mapping: Vec<(usize, usize)>) -> NodeDraft {
        NodeDraft::new(PendingInline {
            content: source,
            mapping,
        })
    }

    /// Return the source span covering lines `start_line..=end_line`.
    #[must_use]
    pub fn get_map(&self, start_line: usize, end_line: usize) -> Option<SourcePos> {
        debug_assert!(start_line <= end_line);

        Some(SourcePos::new(
            self.line_offsets[start_line].first_nonspace,
            self.line_offsets[end_line].line_end,
        ))
    }
}

pub struct DocumentInlineState<'a> {
    /// Markdown source.
    pub(crate) src: Cow<'a, str>,

    /// Current byte offset in `src`, it must respect char boundaries.
    pub(crate) pos: usize,

    /// Maximum allowed byte offset in `src`, it must respect char boundaries.
    pub(crate) pos_max: usize,

    /// Link to parser instance.
    md: &'a MarkdownIt,

    /// For each line, it holds offset of the start of the line in original
    /// markdown source and offset of the start of the line in `src`.
    mapping: Cow<'a, [(usize, usize)]>,

    /// Counter used to prevent recursion by image and link rules.
    depth: u32,

    pub(crate) inline_ext: InlineRootExtSet,

    pub(crate) root_ext: Option<&'a RootExtSet>,

    /// Counter used to disable inline linkifier execution
    /// inside raw html and markdown links.
    pub(crate) link_level: i32,

    pub(crate) ruleset: &'a DocumentRuleSet,

    /// Nodes accumulated for the current inline container; rules append to it.
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
        root_ext: Option<&'a RootExtSet>,
    ) -> Vec<NodeDraft> {
        let mut state = Self {
            pos: 0,
            pos_max: src.len(),
            src: Cow::Owned(src),
            md,
            mapping: Cow::Owned(mapping),
            depth: 0,
            inline_ext: InlineRootExtSet::new(),
            root_ext,
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
        self.parse_subrange_with_link_level(range, self.link_level)
    }

    /// Parse an isolated child range with an explicit initial link level.
    ///
    /// This overrides only the child's link level. Source mapping, nesting
    /// limits, whitespace preservation and scratch isolation are unchanged.
    pub(crate) fn parse_subrange_with_link_level(
        &self,
        range: Range<usize>,
        link_level: i32,
    ) -> Option<Vec<NodeDraft>> {
        self.parse_subrange_at_depth(range, link_level, self.depth.saturating_add(1))
    }

    /// Parse an isolated range without consuming an extra nesting level.
    pub(crate) fn parse_subrange_at_current_depth(
        &self,
        range: Range<usize>,
    ) -> Option<Vec<NodeDraft>> {
        self.parse_subrange_at_depth(range, self.link_level, self.depth)
    }

    fn parse_subrange_at_depth(
        &self,
        range: Range<usize>,
        link_level: i32,
        depth: u32,
    ) -> Option<Vec<NodeDraft>> {
        self.remaining().get(range.clone())?;

        let start = self.pos + range.start;
        let end = self.pos + range.end;
        let mut child = DocumentInlineState {
            src: Cow::Borrowed(self.src.as_ref()),
            pos: start,
            pos_max: end,
            md: self.md,
            mapping: Cow::Borrowed(self.mapping.as_ref()),
            depth,
            inline_ext: InlineRootExtSet::new(),
            root_ext: self.root_ext,
            link_level,
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
            let marker = self.src[self.pos..self.pos_max].chars().next().unwrap();
            let mut matched = None;
            for rule in &self.ruleset.runs {
                if !rule.matches_marker(marker) {
                    continue;
                }
                if let Some(result) = (rule.run)(self) {
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

    /// Inspect the current position without entering a child parse level.
    pub(crate) fn probe_current(&self) -> InlineProbeContext<'_> {
        self.probe_at_depth(0..self.remaining().len(), self.depth)
    }

    /// Probe a later offset from the current position at the current depth.
    pub(crate) fn probe_from(&self, offset: usize) -> InlineProbeContext<'_> {
        self.probe_at_depth(offset..self.remaining().len(), self.depth)
    }

    fn probe_at_depth(&self, range: Range<usize>, depth: u32) -> InlineProbeContext<'_> {
        InlineProbeContext::new(
            self.src.as_ref(),
            self.pos + range.start,
            self.pos + range.end,
            self.md,
            self.ruleset,
            depth,
            self.link_level,
        )
        .with_root_ext(self.root_ext)
    }

    /// Start an independent probe session for `range` relative to
    /// [`Self::remaining`].
    ///
    /// The returned context owns its cursor and scratch storage; the parent
    /// state is left untouched, including pending text and inline extensions.
    /// Ranges that are reversed, out of bounds, or not on UTF-8 boundaries
    /// return `None`. Classification happens through
    /// [`InlineProbeContext::next_token`]; rules without a probe are skipped
    /// and unclaimed characters become text.
    pub fn probe_subrange(&self, range: Range<usize>) -> Option<InlineProbeContext<'_>> {
        self.remaining().get(range.clone())?;
        Some(self.probe_at_depth(range, self.depth.saturating_add(1)))
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
mod tests;
