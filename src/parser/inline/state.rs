//! Inline parsing state, source mapping, and nested parse sessions.

use std::borrow::Cow;
use std::ops::Range;

use super::DocumentRuleSet;
use super::probe::InlineProbeContext;
use crate::MarkdownIt;
use crate::common::extset::{InlineRootExtSet, RootExtSet};
use crate::common::sourcemap::SourcePos;
use crate::document::{NodeDraft, Text};

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

    pub(in crate::parser) fn parse(
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
}

#[cfg(test)]
mod tests;
