//! Inline parsing state, source mapping, and nested parse sessions.

use std::borrow::Cow;
use std::cell::{RefCell, RefMut};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;

use super::{InlineRunRunner, RuleSet};
use crate::MarkdownIt;
use crate::common::extset::{InlineRootExtSet, RootExtSet};
use crate::document::{Document, NodeId, Text};

// TODO: adjust API visibility

/// Marks an inline parse that has work deferred until core postprocessing.
#[derive(Debug, Default)]
pub(crate) struct DeferredPostprocess;

// TODO(inline-postprocess): replace this root list with explicit inline
// ownership or a flat token model; marker nodes would leak internal structure
// into the AST.
/// Roots of inline ranges whose work was deferred to core postprocessing.
///
/// Ranges are appended in completion order (children before parents) and
/// consumed by core rules after inline parsing. Keeping the grouping on the
/// document root avoids exposing internal marker nodes in the AST.
#[derive(Debug, Default)]
pub(crate) struct InlinePostprocessRanges(pub(crate) Vec<Vec<NodeId>>);

/// Per-inline cache shared by nested boundary scans.
///
/// Mirrors markdown-it's inline skip cache: once a scan at the nesting limit
/// jumps to the end of a window, later scans over the same window see the same
/// cutoff. The window end is part of the key so a truncated subrange does not
/// affect a scan over a different range.
#[derive(Debug, Default)]
pub(crate) struct ScanLimitCache {
    truncated: HashSet<(usize, usize)>,
    skip_tokens: HashMap<(usize, usize, i32), (usize, bool)>,
    pub(crate) link_bounds: Option<super::helpers::full_link::LinkLabelBounds>,
    /// Extension caches shared by sessions over the same source.
    ext: InlineRootExtSet,
}

impl ScanLimitCache {
    /// Monotonic version of the truncation set, used to invalidate cached
    /// label boundaries after a new nesting cutoff is recorded.
    ///
    /// Relies on `truncated` only ever growing; adding removal would make this
    /// a non-monotonic counter and silently revive stale cache entries.
    pub(crate) fn epoch(&self) -> usize {
        self.truncated.len()
    }
}

pub struct InlineState<'a> {
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
    #[cfg(feature = "sourcemap")]
    mapping: Cow<'a, [(usize, usize)]>,

    /// Counter used to prevent recursion by image and link rules.
    pub(crate) depth: u32,

    pub(crate) inline_ext: InlineRootExtSet,

    /// Nesting-limit decisions shared by nested boundary scans.
    pub(crate) scan_limit_cache: Rc<RefCell<ScanLimitCache>>,

    pub(crate) root_ext: Option<&'a RootExtSet>,

    /// Counter used to disable inline linkifier execution
    /// inside raw html and markdown links.
    pub(crate) link_level: i32,

    pub(crate) ruleset: &'a RuleSet,

    /// Nodes accumulated for the current inline container; rules append to it.
    nodes: Vec<NodeId>,

    /// Arena shared by nested inline sessions.
    pub document: &'a mut Document,

    pending_text: Option<(usize, usize)>,
}

impl<'a> InlineState<'a> {
    /// The unconsumed inline source visible to the current rule.
    pub fn remaining(&self) -> &str {
        &self.src[self.pos..self.pos_max]
    }

    /// Borrow the current source window and document independently.
    pub fn remaining_and_document(&mut self) -> (&str, &mut Document) {
        (&self.src[self.pos..self.pos_max], &mut *self.document)
    }

    /// Parser instance that owns this inline rule.
    pub fn markdown_it(&self) -> &MarkdownIt {
        self.md
    }

    /// Extension caches shared by sessions over this source.
    pub(crate) fn shared_ext(&self) -> RefMut<'_, InlineRootExtSet> {
        RefMut::map(self.scan_limit_cache.borrow_mut(), |cache| &mut cache.ext)
    }

    /// Defer this session's work to core postprocessing.
    pub(crate) fn defer_to_core(&mut self) {
        self.inline_ext
            .get_or_insert_default::<DeferredPostprocess>();
    }

    /// Nodes accumulated by this inline session.
    pub fn nodes(&self) -> &[NodeId] {
        &self.nodes
    }

    /// Mutate the nodes accumulated for this inline container.
    ///
    /// Callers that change existing entries must keep rule-local index caches
    /// in sync.
    pub fn nodes_mut(&mut self) -> &mut Vec<NodeId> {
        &mut self.nodes
    }

    pub(in crate::parser) fn parse(
        document: &'a mut Document,
        src: String,
        mapping: crate::parser::InlineMapping,
        md: &'a MarkdownIt,
        ruleset: &'a RuleSet,
        root_ext: Option<&'a RootExtSet>,
    ) -> Vec<NodeId> {
        #[cfg(not(feature = "sourcemap"))]
        let _ = mapping;
        let mut state = Self {
            pos: 0,
            pos_max: src.len(),
            src: Cow::Owned(src),
            md,
            #[cfg(feature = "sourcemap")]
            mapping: Cow::Owned(mapping),
            depth: 0,
            inline_ext: InlineRootExtSet::new(),
            scan_limit_cache: Rc::new(RefCell::new(ScanLimitCache::default())),
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
    /// are allocated in the shared arena. Children finish local cleanup before
    /// returning; document-level core transformations run later. At the nesting
    /// limit, the child range is emitted as literal text without running rules.
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
        self.parse_child(range, depth, link_level, |_| {})
    }

    /// Parse `range` as a child session after applying `configure`.
    ///
    /// Runs directly so extensions installed by `configure` survive.
    pub(in crate::parser) fn parse_child(
        &mut self,
        range: Range<usize>,
        depth: u32,
        link_level: i32,
        configure: impl FnOnce(&mut InlineState<'_>),
    ) -> Option<Vec<NodeId>> {
        let mut child = self.child_state(range, depth, link_level)?;
        configure(&mut child);
        Some(child.run_child_session())
    }

    /// Run this child session to completion and return its nodes.
    ///
    /// Uses session cleanup and a fresh stack segment.
    fn run_child_session(mut self) -> Vec<NodeId> {
        stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
            self.tokenize();
            self.finish()
        })
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

    pub(super) fn tokenize(&mut self) {
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

                let result = match &rule.run {
                    InlineRunRunner::Static(run) => run(self),
                    InlineRunRunner::Dynamic(run) => run(self),
                };

                if let Some(result) = result {
                    matched = Some(result);
                    break;
                }
            }

            if let Some((node, len)) = matched {
                self.pos += len;
                if let Some(node) = node {
                    self.flush_text();
                    #[cfg(feature = "sourcemap")]
                    let srcmap = self.get_map(self.pos - len, self.pos);
                    #[cfg(feature = "sourcemap")]
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

        let span = (self.pos, self.pos_max);
        let skip_key = (self.pos, self.pos_max, self.link_level);
        let cached = self
            .scan_limit_cache
            .borrow()
            .skip_tokens
            .get(&skip_key)
            .copied();
        if let Some((len, literal)) = cached {
            // Reuse checked spans across depths; the key includes window end
            // and link context to isolate unrelated sessions.
            self.pending_text = literal.then(|| {
                (
                    self.pending_text.map_or(self.pos, |(start, _)| start),
                    self.pos + len,
                )
            });
            self.pos += len;
            return Some(len);
        }
        let truncated = if self.depth > self.md.max_nesting {
            // At the nesting limit every scan is treated as truncated; record it
            // so later scans over the same window see the same cutoff.
            self.scan_limit_cache.borrow_mut().truncated.insert(span);
            true
        } else {
            self.scan_limit_cache.borrow().truncated.contains(&span)
        };
        if truncated {
            let len = self.pos_max - self.pos;
            self.pending_text = Some((self.pos, self.pos_max));
            self.pos += len;
            return Some(len);
        }

        let marker = self.remaining().chars().next().unwrap();
        for rule_index in 0..self.ruleset.checks.len() {
            let entry = &self.ruleset.checks[rule_index];
            if !entry.matches_marker(marker) {
                continue;
            }

            let check = |state: &mut InlineState<'_>| entry.check.call(state);

            #[cfg(debug_assertions)]
            let matched = crate::parser::validation::check_inline(self, rule_index, check);
            #[cfg(not(debug_assertions))]
            let matched = check(self);

            if let Some(len) = matched {
                // Extend the pending text when the match is contiguous;
                // otherwise start an opaque token.
                if self
                    .pending_text
                    .is_none_or(|(_, end)| end != self.pos + len)
                {
                    self.pending_text = None;
                }
                self.cache_skip_token(marker, skip_key, len, self.pending_text.is_some());
                self.pos += len;
                return Some(len);
            }
        }

        let len = marker.len_utf8();
        self.cache_skip_token(marker, skip_key, len, true);
        self.pending_text = Some((
            self.pending_text.map_or(self.pos, |(start, _)| start),
            self.pos + len,
        ));
        self.pos += len;

        Some(len)
    }

    fn cache_skip_token(&self, marker: char, key: (usize, usize, i32), len: usize, literal: bool) {
        if matches!(marker, '[' | '!') || literal {
            let mut cache = self.scan_limit_cache.borrow_mut();
            if cache
                .link_bounds
                .as_ref()
                .is_some_and(|bounds| bounds.can_memoize_labels(self.pos, self.pos_max))
            {
                cache.skip_tokens.insert(key, (len, literal));
            }
        }
    }

    /// Create a child state over `range` with fresh scratch, nodes and pending text.
    pub(crate) fn child_state(
        &mut self,
        range: Range<usize>,
        depth: u32,
        link_level: i32,
    ) -> Option<InlineState<'_>> {
        self.remaining().get(range.clone())?;

        let start = self.pos + range.start;
        let end = self.pos + range.end;

        #[cfg(debug_assertions)]
        crate::parser::validation::inline_range(self.src.as_ref(), start, end);

        Some(InlineState {
            src: Cow::Borrowed(self.src.as_ref()),
            pos: start,
            pos_max: end,
            md: self.md,
            #[cfg(feature = "sourcemap")]
            mapping: Cow::Borrowed(self.mapping.as_ref()),
            depth,
            inline_ext: InlineRootExtSet::new(),
            scan_limit_cache: Rc::clone(&self.scan_limit_cache),
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
        #[cfg(feature = "sourcemap")]
        let srcmap = self.get_map(start, end);
        #[cfg(feature = "sourcemap")]
        self.document.node_mut(text).set_srcmap(srcmap);
        self.nodes.push(text);
    }

    fn finish(mut self) -> Vec<NodeId> {
        // Only reuse an owned input buffer; cloning a borrowed parent source
        // for each child range would be quadratic.
        if self.nodes.is_empty() && matches!(self.src, Cow::Owned(_)) {
            if let Some((start, end)) = self.pending_text.take() {
                #[cfg(feature = "sourcemap")]
                let srcmap = self.get_map(start, end);
                let mut content = std::mem::take(&mut self.src).into_owned();
                content.truncate(end);

                if start != 0 {
                    content.drain(..start);
                }

                let text = self.document.create_node(Text { content });
                #[cfg(feature = "sourcemap")]
                self.document.node_mut(text).set_srcmap(srcmap);
                self.nodes.push(text);
            }
        }
        self.finish_nodes();

        if self.inline_ext.contains::<DeferredPostprocess>() {
            let root = self.document.root();
            self.document
                .node_mut(root)
                .ext_mut()
                .get_or_insert_default::<InlinePostprocessRanges>()
                .0
                .push(self.nodes.clone());
        }

        self.nodes
    }

    pub(super) fn finish_nodes(&mut self) {
        self.flush_text();
    }

    #[cfg(feature = "sourcemap")]
    fn source_pos(&self, pos: usize) -> usize {
        let line = match self.mapping.binary_search_by(|entry| entry.0.cmp(&pos)) {
            Ok(index) => index,
            Err(index) => index - 1,
        };
        self.mapping[line].1 + (pos - self.mapping[line].0)
    }

    #[cfg(feature = "sourcemap")]
    pub(crate) fn get_map(
        &self,
        start: usize,
        end: usize,
    ) -> Option<crate::common::sourcemap::SourcePos> {
        Some(crate::common::sourcemap::SourcePos::new(
            self.source_pos(start),
            self.source_pos(end),
        ))
    }
}

#[cfg(test)]
mod tests;
