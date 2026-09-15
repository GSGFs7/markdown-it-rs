use std::ops::Range;

use crate::parser::extset::InlineRootExtSet;
use crate::parser::main::MarkdownIt;

/// How a probed span is classified.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineProbeKind {
    /// Original source bytes appended to pending text, with no independent node.
    Text,
    /// A separately recognized token; clears pending text for following probes.
    Token,
}

/// Semantic changes applied only after a probe match is validated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InlineProbeEffects {
    /// Signed change to this session's link nesting level.
    pub link_level_delta: i32,
}

/// Result reported by one inline rule for the current probe position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineProbeResult {
    /// The rule does not claim the current position in probe mode; normal
    /// `run` may still match it.
    NoMatch,
    /// The rule claims the span starting at this position.
    Match { len: usize, kind: InlineProbeKind },
    /// The rule claims the span and reports semantic effects.
    MatchWithEffects {
        len: usize,
        kind: InlineProbeKind,
        effects: InlineProbeEffects,
    },
}

/// One classified span relative to the start of a probe session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineProbeToken {
    /// UTF-8 byte range relative to the beginning of this probe session.
    pub range: Range<usize>,
    /// Classification of the span.
    pub kind: InlineProbeKind,
}

/// Independent token-probing session over a fixed source range.
///
/// A session owns its cursor and scratch storage; the parent inline state is
/// never modified. [`InlineProbeContext::next_token`] walks the range and
/// classifies it into [`InlineProbeToken`] spans. Rules only inspect the
/// current position through this context; the engine consumes the reported
/// length after a [`InlineProbeResult::Match`].
pub struct InlineProbeContext<'a> {
    source: &'a str,
    start: usize,
    pos: usize,
    end: usize,
    md: &'a MarkdownIt,
    ruleset: &'a super::DocumentRuleSet,
    depth: u32,
    link_level: i32,
    pending_text: Option<(usize, usize)>,
    scratch: InlineRootExtSet,
}

impl<'a> InlineProbeContext<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        source: &'a str,
        start: usize,
        end: usize,
        md: &'a MarkdownIt,
        ruleset: &'a super::DocumentRuleSet,
        depth: u32,
        link_level: i32,
    ) -> Self {
        debug_assert!(start <= end && end <= source.len());
        debug_assert!(source.is_char_boundary(start));
        debug_assert!(source.is_char_boundary(end));
        Self {
            source,
            start,
            pos: start,
            end,
            md,
            ruleset,
            depth,
            link_level,
            pending_text: None,
            scratch: InlineRootExtSet::new(),
        }
    }

    /// Source bytes from the cursor to the end of the probed range.
    #[must_use]
    pub fn remaining(&self) -> &str {
        &self.source[self.pos..self.end]
    }

    /// Original source of the pending [`InlineProbeKind::Text`] span, or `""`.
    #[must_use]
    pub fn trailing_text(&self) -> &str {
        self.pending_text
            .map_or("", |(start, end)| &self.source[start..end])
    }

    /// Parser instance that owns the rules being probed.
    #[must_use]
    pub fn markdown_it(&self) -> &MarkdownIt {
        self.md
    }

    /// Current link nesting level of this session.
    #[must_use]
    pub fn link_level(&self) -> i32 {
        self.link_level
    }

    /// Inline nesting depth of this probe session.
    #[must_use]
    pub fn depth(&self) -> u32 {
        self.depth
    }

    /// Scratch storage private to this probe session.
    pub fn scratch_mut(&mut self) -> &mut InlineRootExtSet {
        &mut self.scratch
    }

    /// Borrow the source window and scratch storage together.
    ///
    /// This crate-private entry point lets rules such as `code_pair` scan the
    /// remaining source while keeping their cache in the session scratch.
    pub(crate) fn with_scratch<T>(
        &mut self,
        f: impl FnOnce(&str, usize, usize, &mut InlineRootExtSet) -> T,
    ) -> T {
        f(self.source, self.pos, self.end, &mut self.scratch)
    }

    fn accept(&mut self, len: usize, kind: InlineProbeKind) -> InlineProbeToken {
        let old_pos = self.pos;
        let end = old_pos + len;
        self.pending_text = match kind {
            InlineProbeKind::Text => {
                Some((self.pending_text.map_or(old_pos, |(start, _)| start), end))
            }
            InlineProbeKind::Token => None,
        };
        self.pos = end;
        InlineProbeToken {
            range: old_pos - self.start..end - self.start,
            kind,
        }
    }

    fn next_token_inner(&mut self) -> Option<InlineProbeToken> {
        if self.pos == self.end {
            return None;
        }
        if self.depth >= self.md.max_nesting {
            return Some(self.accept(self.end - self.pos, InlineProbeKind::Text));
        }

        let marker = self.remaining().chars().next().unwrap();
        for rule_index in 0..self.ruleset.probes.len() {
            let entry = self.ruleset.probes[rule_index];
            if !entry.matches_marker(marker) {
                continue;
            }

            let (len, kind, effects) = match (entry.probe)(self) {
                InlineProbeResult::NoMatch => continue,
                InlineProbeResult::Match { len, kind } => {
                    (len, kind, InlineProbeEffects::default())
                }
                InlineProbeResult::MatchWithEffects { len, kind, effects } => (len, kind, effects),
            };

            assert!(
                len > 0 && self.remaining().get(..len).is_some(),
                "inline rule {rule_index} reported invalid probe length {len}"
            );

            let next_link_level = self
                .link_level
                .checked_add(effects.link_level_delta)
                .unwrap_or_else(|| {
                    panic!(
                        "inline rule {rule_index} overflowed probe link level: {} + {}",
                        self.link_level, effects.link_level_delta
                    )
                });

            self.link_level = next_link_level;
            return Some(self.accept(len, kind));
        }

        Some(self.accept(marker.len_utf8(), InlineProbeKind::Text))
    }

    /// Classify the next span, or return `None` at the end of the session.
    ///
    /// At the nesting limit the remaining range becomes one
    /// [`InlineProbeKind::Text`] span. Invalid lengths or overflowing effects
    /// panic before the cursor or effects are applied; scratch may already be
    /// mutated.
    pub fn next_token(&mut self) -> Option<InlineProbeToken> {
        stacker::maybe_grow(64 * 1024, 1024 * 1024, || self.next_token_inner())
    }

    /// Create an isolated child probe over a range of `remaining()`.
    ///
    /// Invalid UTF-8 ranges or an exhausted nesting budget return `None`.
    /// The parent remains unchanged. The child inherits the current link
    /// level and starts with empty pending text and private scratch storage.
    pub fn probe_subrange(&self, range: Range<usize>) -> Option<InlineProbeContext<'_>> {
        self.remaining().get(range.clone())?;

        let child_depth = self.depth.checked_add(1)?;
        if child_depth >= self.md.max_nesting {
            return None;
        }

        Some(InlineProbeContext::new(
            self.source,
            self.pos + range.start,
            self.pos + range.end,
            self.md,
            self.ruleset,
            child_depth,
            self.link_level(),
        ))
    }
}
