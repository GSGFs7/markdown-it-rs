use std::fmt::Display;
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

/// Result reported by one inline rule for the current probe position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineProbeResult {
    /// The rule cannot decide without running; probing stops with an error.
    Unsupported,
    /// The rule has determined that it cannot match at this position.
    NoMatch,
    /// The rule claims the span starting at this position.
    Match { len: usize, kind: InlineProbeKind },
}

/// One classified span relative to the start of a probe session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineProbeToken {
    /// UTF-8 byte range relative to the beginning of this probe session.
    pub range: Range<usize>,
    /// Classification of the span.
    pub kind: InlineProbeKind,
}

/// Error reported while probing an inline range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineProbeError {
    /// A rule reached a position it cannot classify without running.
    UnsupportedRule {
        /// Index in the sorted ruleset; only meaningful for locating the rule.
        rule_index: usize,
        /// Marker declared by the rule.
        marker: char,
    },
    /// A rule returned a zero, out-of-bounds, or non-UTF-8-boundary length.
    InvalidLength {
        /// Index in the sorted ruleset; only meaningful for locating the rule.
        rule_index: usize,
        /// Length reported by the rule.
        len: usize,
    },
}

impl Display for InlineProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedRule { rule_index, marker } => write!(
                f,
                "inline rule {rule_index} does not support probing marker {marker:?}"
            ),
            Self::InvalidLength { rule_index, len } => write!(
                f,
                "inline rule {rule_index} reported invalid probe length {len}"
            ),
        }
    }
}

impl std::error::Error for InlineProbeError {}

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
    error: Option<InlineProbeError>,
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
            error: None,
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

    /// Link nesting level inherited from the parent inline state.
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

    /// Classify the next span, or return `None` at the end of the session.
    ///
    /// Rules are tried in the order they were registered. `NoMatch` moves on
    /// to the next candidate; `Unsupported` stops the session with
    /// [`InlineProbeError::UnsupportedRule`]. Errors are sticky: every later
    /// call returns the same error. At the nesting limit the whole remaining
    /// range is reported as one [`InlineProbeKind::Text`] span without calling
    /// any rule.
    pub fn next_token(&mut self) -> Result<Option<InlineProbeToken>, InlineProbeError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        if self.pos == self.end {
            return Ok(None);
        }
        if self.depth >= self.md.max_nesting {
            return Ok(Some(
                self.accept(self.end - self.pos, InlineProbeKind::Text),
            ));
        }

        let marker = self.remaining().chars().next().unwrap();
        for rule_index in 0..self.ruleset.probes.len() {
            let entry = self.ruleset.probes[rule_index];
            if !entry.matches_marker(marker) {
                continue;
            }
            match (entry.probe)(self) {
                InlineProbeResult::NoMatch => {}
                InlineProbeResult::Unsupported => {
                    let error = InlineProbeError::UnsupportedRule {
                        rule_index,
                        marker: entry.marker,
                    };
                    self.error = Some(error);
                    return Err(error);
                }
                InlineProbeResult::Match { len, kind } => {
                    if len == 0 || self.remaining().get(..len).is_none() {
                        let error = InlineProbeError::InvalidLength { rule_index, len };
                        self.error = Some(error);
                        return Err(error);
                    }
                    return Ok(Some(self.accept(len, kind)));
                }
            }
        }

        Ok(Some(self.accept(marker.len_utf8(), InlineProbeKind::Text)))
    }
}
