//! Inline parsing state, source mapping, and nested parse sessions.

use std::borrow::Cow;
use std::ops::Range;

use super::DocumentRuleSet;
use crate::MarkdownIt;
use crate::common::extset::{InlineRootExtSet, RootExtSet};
use crate::common::sourcemap::SourcePos;
use crate::document::{Document, NodeId, Text};

// TODO: adjust API visibility
pub struct DocumentInlineState<'a> {
    /// Markdown source.
    pub(crate) src: Cow<'a, str>,

    /// Current byte offset in `src`, it must respect char boundaries.
    pub(crate) pos: usize,

    /// Maximum allowed byte offset in `src`, it must respect char boundaries.
    pub(crate) pos_max: usize,

    /// Link to parser instance.
    pub(crate) md: &'a MarkdownIt,

    /// For each line, it holds offset of the start of the line in original
    /// markdown source and offset of the start of the line in `src`.
    mapping: Cow<'a, [(usize, usize)]>,

    /// Counter used to prevent recursion by image and link rules.
    pub(crate) depth: u32,

    pub(crate) inline_ext: InlineRootExtSet,

    pub(crate) root_ext: Option<&'a RootExtSet>,

    /// Counter used to disable inline linkifier execution
    /// inside raw html and markdown links.
    pub(crate) link_level: i32,

    pub(crate) ruleset: &'a DocumentRuleSet,

    /// Nodes accumulated for the current inline container; rules append to it.
    nodes: Vec<NodeId>,

    /// Arena shared by nested inline sessions.
    pub document: &'a mut Document,

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

    pub(crate) fn nodes(&self) -> &[NodeId] {
        &self.nodes
    }

    pub(crate) fn nodes_mut(&mut self) -> &mut Vec<NodeId> {
        &mut self.nodes
    }

    pub(in crate::parser) fn parse(
        document: &'a mut Document,
        src: String,
        mapping: Vec<(usize, usize)>,
        md: &'a MarkdownIt,
        ruleset: &'a DocumentRuleSet,
        root_ext: Option<&'a RootExtSet>,
    ) -> Vec<NodeId> {
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
            document,
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
    /// returns an empty vector. Parent cursor and scratch are unchanged; children
    /// are allocated in the shared arena. At the nesting limit,
    /// the child range is emitted as literal text without running rules.
    pub fn parse_subrange(&mut self, range: Range<usize>) -> Option<Vec<NodeId>> {
        self.parse_subrange_with_link_level(range, self.link_level)
    }

    /// Parse an isolated child range with an explicit initial link level.
    ///
    /// This overrides only the child's link level. Source mapping, nesting
    /// limits, whitespace preservation and scratch isolation are unchanged.
    pub(crate) fn parse_subrange_with_link_level(
        &mut self,
        range: Range<usize>,
        link_level: i32,
    ) -> Option<Vec<NodeId>> {
        self.parse_subrange_at_depth(range, link_level, self.depth.saturating_add(1))
    }

    /// Parse an isolated range without consuming an extra nesting level.
    pub(crate) fn parse_subrange_at_current_depth(
        &mut self,
        range: Range<usize>,
    ) -> Option<Vec<NodeId>> {
        self.parse_subrange_at_depth(range, self.link_level, self.depth)
    }

    fn parse_subrange_at_depth(
        &mut self,
        range: Range<usize>,
        link_level: i32,
        depth: u32,
    ) -> Option<Vec<NodeId>> {
        let mut child = self.child_state(range, depth, link_level)?;

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
                if let Some(node) = node {
                    self.flush_text();
                    let srcmap = self.get_map(self.pos - len, self.pos);
                    self.document.node_mut(node).set_srcmap(srcmap);
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

    /// Advance over one checked span without emitting nodes, for boundary scans.
    pub(crate) fn skip_token(&mut self) -> Option<usize> {
        stacker::maybe_grow(64 * 1024, 1024 * 1024, || self.skip_token_inner())
    }

    fn skip_token_inner(&mut self) -> Option<usize> {
        if self.pos == self.pos_max {
            return None;
        }

        if self.depth >= self.md.max_nesting {
            let len = self.pos_max - self.pos;
            self.pending_text = Some((self.pos, self.pos_max));
            self.pos += len;
            return Some(len);
        }

        let marker = self.remaining().chars().next().unwrap();
        for rule_index in 0..self.ruleset.checks.len() {
            let entry = self.ruleset.checks[rule_index];
            if !entry.matches_marker(marker) {
                continue;
            }

            #[cfg(debug_assertions)]
            let matched = crate::parser::validation::check_inline(self, rule_index, entry.check);

            #[cfg(not(debug_assertions))]
            let matched = (entry.check)(self);
            if let Some(len) = matched {
                // Extend the pending text when the match is contiguous;
                // otherwise start an opaque token.
                if self
                    .pending_text
                    .is_none_or(|(_, end)| end != self.pos + len)
                {
                    self.pending_text = None;
                }
                self.pos += len;
                return Some(len);
            }
        }

        let len = marker.len_utf8();
        self.pending_text = Some((
            self.pending_text.map_or(self.pos, |(start, _)| start),
            self.pos + len,
        ));
        self.pos += len;

        Some(len)
    }

    /// Create a child state over `range` with fresh scratch, nodes and pending text.
    pub(crate) fn child_state(
        &mut self,
        range: Range<usize>,
        depth: u32,
        link_level: i32,
    ) -> Option<DocumentInlineState<'_>> {
        self.remaining().get(range.clone())?;

        let start = self.pos + range.start;
        let end = self.pos + range.end;

        #[cfg(debug_assertions)]
        crate::parser::validation::inline_range(self.src.as_ref(), start, end);

        Some(DocumentInlineState {
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
            document: &mut *self.document,
            pending_text: None,
        })
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
        let text = self.document.create_node(Text {
            content: self.src[start..end].to_owned(),
        });
        let srcmap = self.get_map(start, end);
        self.document.node_mut(text).set_srcmap(srcmap);
        self.nodes.push(text);
    }

    fn finish(mut self) -> Vec<NodeId> {
        if self.nodes.is_empty() {
            if let Some((start, end)) = self.pending_text.take() {
                let srcmap = self.get_map(start, end);
                let mut content = self.src.into_owned();
                content.truncate(end);

                if start != 0 {
                    content.drain(..start);
                }

                let text = self.document.create_node(Text { content });
                self.document.node_mut(text).set_srcmap(srcmap);
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
