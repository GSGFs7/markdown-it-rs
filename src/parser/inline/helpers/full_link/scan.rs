//! Link recognition with shared boundary results.
//!
//! The recognizer works on a source window plus inherited nesting/link context; it
//! never emits nodes and never calls the link rules through the normal inline
//! dispatcher. `[` / `![` are handled by an explicit label loop, while every
//! other construct is advanced through its boundary check in a label-local
//! session, preserving literal runs and rule scratch. Nested candidates call this same
//! scanner, but bracket links are suppressed in the child state so the normal
//! dispatcher cannot re-enter this module.

use super::config::InlineTargetFallback;
use super::reference::ParseLinkResult;
use super::target::{LinkTargetScan, scan_link_target};
use crate::parser::inline::{InlineState, RuleSet};
use crate::plugins::cmark::block::reference::{DefaultReferenceMap, ReferenceMap};
use crate::plugins::html::html_inline::special_html_disabled;

#[derive(Debug, Clone)]
pub(crate) struct Candidate {
    label_start: usize,
    label_end: usize,
    end: usize,
    href: Option<String>,
    title: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct Outcome {
    candidate: Option<Candidate>,
    link_level: i32,
    html_disabled: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LabelOutcome {
    close: Option<usize>,
    link_level: i32,
    html_disabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct CandidateKey {
    start: usize,
    end: usize,
    enable_nested: bool,
    fallback: InlineTargetFallback,
    link_level: i32,
    html_disabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct LabelKey {
    start: usize,
    end: usize,
    link_level: i32,
    html_disabled: bool,
    enable_nested: bool,
}

/// A successful probe at a deeper nesting level also fits a shallower budget.
/// New source-wide cutoffs invalidate this implication, so retain the cutoff
/// epoch with the result. Keep only one depth per boundary instead of a result
/// for every overlapping nesting budget.
#[derive(Debug)]
struct DepthCached<T> {
    depth: u32,
    epoch: usize,
    outcome: T,
}

/// Recognition results and cutoff history shared over one inline source.
#[derive(Debug, Default)]
pub(crate) struct LinkScanCache {
    candidates: std::collections::HashMap<CandidateKey, DepthCached<Outcome>>,
    labels: std::collections::HashMap<LabelKey, DepthCached<LabelOutcome>>,
    /// Failed boundaries remain failed after a nesting cutoff, including in
    /// later, shallower probes. Share them across depths rather than keeping
    /// one overlapping failed scan for every nesting budget.
    failed_labels: std::collections::HashMap<LabelKey, LabelOutcome>,
    /// Earliest prefix that has already been scanned to the end of the source
    /// solely to preserve HTML side effects.
    forced_suffix_start: Option<usize>,
    /// Optional raw bracket index for plain sources: open -> (close, max depth).
    plain_closes: Option<std::collections::HashMap<usize, (usize, usize)>>,
    plain_closes_ready: bool,
    /// Windows whose scan hit the nesting limit. markdown-it exposes this as a
    /// source-wide cutoff: a later, shallower probe over the same window still
    /// fails.
    cutoffs: std::collections::BTreeSet<(usize, usize)>,
}

impl LinkScanCache {
    fn candidate(&self, key: CandidateKey, depth: u32) -> Option<Outcome> {
        let cached = self.candidates.get(&key)?;
        (depth <= cached.depth && cached.epoch == self.cutoffs.len())
            .then(|| cached.outcome.clone())
    }

    fn insert_candidate(&mut self, key: CandidateKey, depth: u32, outcome: Outcome) {
        self.candidates.insert(
            key,
            DepthCached {
                depth,
                epoch: self.cutoffs.len(),
                outcome,
            },
        );
    }

    fn label(&self, key: LabelKey, depth: u32) -> Option<LabelOutcome> {
        if let Some(outcome) = self.failed_labels.get(&key) {
            return Some(*outcome);
        }
        let cached = self.labels.get(&key)?;
        (depth <= cached.depth && cached.epoch == self.cutoffs.len()).then_some(cached.outcome)
    }

    fn insert_label(&mut self, key: LabelKey, depth: u32, outcome: LabelOutcome) {
        if outcome.close.is_none() {
            self.failed_labels.insert(key, outcome);
        } else {
            self.labels.insert(
                key,
                DepthCached {
                    depth,
                    epoch: self.cutoffs.len(),
                    outcome,
                },
            );
        }
    }

    fn cutoff(&self, start: usize, end: usize) -> bool {
        self.cutoffs.contains(&(start, end))
    }

    fn insert_cutoff(&mut self, start: usize, end: usize) {
        self.cutoffs.insert((start, end));
    }

    fn cutoff_before(&self, start: usize, end: usize, limit: usize) -> bool {
        self.cutoffs
            .range((start, usize::MIN)..(limit, usize::MAX))
            .next()
            .is_some_and(|&(_, cutoff_end)| end <= cutoff_end)
    }

    /// `None` when the source is not eligible for the plain fast path.
    fn plain_close(
        &mut self,
        source: &str,
        rules: &RuleSet,
        start: usize,
    ) -> Option<Option<(usize, usize)>> {
        if !self.plain_closes_ready {
            self.plain_closes_ready = true;
            if plain_source_supported(source, rules) {
                let mut open: Vec<(usize, usize, usize)> = Vec::new();
                let mut closes = std::collections::HashMap::new();
                for (index, byte) in source.bytes().enumerate() {
                    match byte {
                        b'[' => {
                            let depth = open.len() + 1;
                            open.push((index, depth, depth));
                        }
                        b']' => {
                            if let Some((open_index, depth, max_depth)) = open.pop() {
                                closes.insert(open_index, (index, max_depth - depth + 1));
                                if let Some(parent) = open.last_mut() {
                                    parent.2 = parent.2.max(max_depth);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                self.plain_closes = Some(closes);
            }
        }
        self.plain_closes
            .as_ref()
            .map(|closes| closes.get(&start).copied())
    }

    fn forced_suffix_scanned(&mut self, start: usize) -> bool {
        match self.forced_suffix_start {
            Some(previous) if previous <= start => true,
            Some(previous) => {
                self.forced_suffix_start = Some(start.min(previous));
                false
            }
            None => {
                self.forced_suffix_start = Some(start);
                false
            }
        }
    }
}

/// Whether the source only contains syntax the raw bracket index can model.
///
/// No TypeId allowlist is involved: a plain window must contain no marker that
/// any registered non-text rule could consume, and at most the text rule may
/// use the wildcard marker.
fn plain_source_supported(source: &str, rules: &RuleSet) -> bool {
    let bytes = source.as_bytes();
    let is_plain = |byte: u8| {
        byte.is_ascii_alphanumeric() || byte.is_ascii_whitespace() || b"[]!()".contains(&byte)
    };
    if !bytes.iter().all(|&byte| is_plain(byte)) {
        return false;
    }

    let mut wildcard_rules = 0usize;
    for rule in &rules.checks {
        if rule.marker == '\0' {
            wildcard_rules += 1;
            continue;
        }
        if !matches!(rule.marker, '[' | ']' | '!') && source.contains(rule.marker) {
            return false;
        }
    }
    if wildcard_rules > 1 {
        return false;
    }

    let mut depth = 0usize;
    for &byte in bytes {
        match byte {
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            b'[' | b']' | b'!' if depth != 0 => return false,
            _ => {}
        }
    }
    true
}

/// Reject candidates using indexes before allocating recognition results.
/// A hook can still require a scan to preserve source-wide side effects.
fn candidate_preflight(
    state: &mut InlineState<'_>,
    start: usize,
    references: Option<&ReferenceMap>,
) -> Option<bool> {
    let end_max = state.pos_max;
    if state.src.as_bytes().get(start) != Some(&b'[') {
        return None;
    }

    // Cheap rejection for the common pathological case: no raw `]` in the
    // remaining window means no label can close at any nesting level.
    let label_start = start + 1;
    let may_have_reference = references.is_some_and(|references| {
        references
            .downcast_ref::<DefaultReferenceMap>()
            .is_none_or(|map| map.iter().next().is_some())
    });
    let has_target = may_have_reference || {
        let source = state.src.as_ref();
        state
            .scan_limit_cache
            .borrow_mut()
            .has_link_target(source, label_start, end_max)
    };
    // Without side-effect hooks there is no reason even to search/index
    // closers when no inline or reference target is possible.
    if !has_target && state.scan_hooks().is_empty() {
        return None;
    }
    let has_close = {
        let source = state.src.as_ref();
        state
            .scan_limit_cache
            .borrow_mut()
            .has_link_close(source, label_start, end_max)
    };
    let forced_scan =
        !has_close && crate::parser::inline::scan_hooks_require_scan(state, label_start, end_max);
    if !has_close && !forced_scan {
        return None;
    }
    if forced_scan
        && state
            .scan_limit_cache
            .borrow_mut()
            .link_scan
            .forced_suffix_scanned(label_start)
    {
        // The whole suffix has already been probed for source-wide effects.
        return None;
    }

    if !forced_scan && !has_target {
        return None;
    }

    Some(may_have_reference)
}

/// Scan one link/image candidate whose `[` is at `start` (absolute).
#[allow(clippy::too_many_arguments)]
fn scan_candidate_abs(
    state: &mut InlineState<'_>,
    start: usize,
    enable_nested: bool,
    fallback: InlineTargetFallback,
    references: Option<&ReferenceMap>,
    depth: u32,
    link_level: i32,
    html_disabled: bool,
    retain: bool,
) -> Outcome {
    // Impossible candidates need only the source indexes. Do not retain a
    // hash-map entry for every literal opener in a source without link syntax.
    let Some(may_have_reference) = candidate_preflight(state, start, references) else {
        return Outcome {
            candidate: None,
            link_level,
            html_disabled: special_html_disabled(state),
        };
    };
    let key = CandidateKey {
        start,
        end: state.pos_max,
        enable_nested,
        fallback,
        link_level,
        html_disabled,
    };
    if let Some(outcome) = {
        state
            .scan_limit_cache
            .borrow()
            .link_scan
            .candidate(key, depth)
    } {
        return outcome;
    }
    let outcome = scan_candidate_abs_inner(
        state,
        start,
        enable_nested,
        fallback,
        references,
        depth,
        link_level,
        html_disabled,
        may_have_reference,
    );
    // Only nested probes populate this cache: overlapping label scans can
    // revisit them. Outer rules advance through the source and can reuse
    // nested/label boundaries without retaining another owned result per
    // opener. Nesting cutoffs and failed-label history are kept separately.
    if retain {
        state
            .scan_limit_cache
            .borrow_mut()
            .link_scan
            .insert_candidate(key, depth, outcome.clone());
    }
    outcome
}

#[allow(clippy::too_many_arguments)]
fn scan_candidate_abs_inner(
    state: &mut InlineState<'_>,
    start: usize,
    enable_nested: bool,
    fallback: InlineTargetFallback,
    references: Option<&ReferenceMap>,
    depth: u32,
    link_level: i32,
    html_disabled: bool,
    may_have_reference: bool,
) -> Outcome {
    let end_max = state.pos_max;
    let fail = |link_level, html_disabled| Outcome {
        candidate: None,
        link_level,
        html_disabled,
    };

    let label_start = start + 1;
    let plain = state.scan_limit_cache.borrow_mut().link_scan.plain_close(
        state.src.as_ref(),
        state.ruleset,
        start,
    );
    let (close, link_level, html_disabled) = match plain {
        Some(Some((close, max_depth))) if close < end_max => {
            let max_nesting = state.markdown_it().max_nesting;
            if close != label_start && depth.saturating_add(max_depth as u32) > max_nesting {
                let budget = max_nesting.saturating_sub(depth) as usize;
                state
                    .scan_limit_cache
                    .borrow_mut()
                    .link_scan
                    .insert_cutoff(start.saturating_add(budget), end_max);
                return fail(link_level, html_disabled);
            }
            if state
                .scan_limit_cache
                .borrow()
                .link_scan
                .cutoff_before(start, end_max, close)
            {
                return fail(link_level, html_disabled);
            }
            let inner_has_target = close > label_start
                && memchr::memchr(b'(', &state.src.as_bytes()[label_start..close]).is_some();
            if close > label_start && (inner_has_target || may_have_reference) {
                let label = scan_label(
                    state,
                    label_start,
                    end_max,
                    references,
                    depth.saturating_add(1),
                    link_level,
                    html_disabled,
                    enable_nested,
                );
                let Some(close) = label.close else {
                    return fail(label.link_level, label.html_disabled);
                };
                (close, label.link_level, label.html_disabled)
            } else {
                (close, link_level, html_disabled)
            }
        }
        Some(Some(_)) => return fail(link_level, html_disabled),
        Some(None) => {
            let budget = state.markdown_it().max_nesting.saturating_sub(depth) as usize;
            let cutoff = start.saturating_add(budget);
            if cutoff < end_max && state.src.as_bytes().get(cutoff) == Some(&b'[') {
                state
                    .scan_limit_cache
                    .borrow_mut()
                    .link_scan
                    .insert_cutoff(cutoff, end_max);
            }
            return fail(link_level, html_disabled);
        }
        None => {
            let label = scan_label(
                state,
                label_start,
                end_max,
                references,
                depth.saturating_add(1),
                link_level,
                html_disabled,
                enable_nested,
            );
            let Some(close) = label.close else {
                return fail(label.link_level, label.html_disabled);
            };
            (close, label.link_level, label.html_disabled)
        }
    };
    let md = state.markdown_it();

    let target = {
        let source = state.src.as_ref();
        scan_link_target(md, source, close + 1, end_max, fallback)
    };

    match target {
        LinkTargetScan::Inline(target) => Outcome {
            candidate: Some(Candidate {
                label_start,
                label_end: close,
                end: target.end,
                href: target.href,
                title: target.title,
            }),
            link_level,
            html_disabled,
        },
        LinkTargetScan::Reject => fail(link_level, html_disabled),
        LinkTargetScan::Reference { resume } => {
            let Some(references) = references else {
                return fail(link_level, html_disabled);
            };

            let explicit = if state.src.as_bytes().get(resume) == Some(&b'[') && resume < end_max {
                let reference = scan_label(
                    state,
                    resume + 1,
                    end_max,
                    Some(references),
                    depth.saturating_add(1),
                    link_level,
                    html_disabled,
                    true,
                );
                reference.close.map(|close| (close, reference))
            } else {
                None
            };

            let (label, end, link_level, html_disabled) = match explicit {
                Some((reference_close, reference)) => {
                    let label = if reference_close == resume + 1 {
                        let source = state.src.as_ref();
                        &source[label_start..close]
                    } else {
                        let source = state.src.as_ref();
                        &source[resume + 1..reference_close]
                    };
                    (
                        label,
                        reference_close + 1,
                        reference.link_level,
                        reference.html_disabled,
                    )
                }
                None => {
                    let source = state.src.as_ref();
                    (
                        &source[label_start..close],
                        close + 1,
                        link_level,
                        html_disabled,
                    )
                }
            };

            let Some((href, title)) = references.get(label) else {
                return fail(link_level, html_disabled);
            };
            Outcome {
                candidate: Some(Candidate {
                    label_start,
                    label_end: close,
                    end,
                    href: Some(href.to_owned()),
                    title: title.map(str::to_owned),
                }),
                link_level,
                html_disabled,
            }
        }
    }
}

/// Scan a label body starting immediately after `[`.
#[allow(clippy::too_many_arguments)]
fn scan_label(
    state: &mut InlineState<'_>,
    start: usize,
    end_max: usize,
    references: Option<&ReferenceMap>,
    depth: u32,
    link_level: i32,
    html_disabled: bool,
    enable_nested: bool,
) -> LabelOutcome {
    let key = LabelKey {
        start,
        end: end_max,
        link_level,
        html_disabled,
        enable_nested,
    };
    if let Some(outcome) = { state.scan_limit_cache.borrow().link_scan.label(key, depth) } {
        return outcome;
    }

    let outcome = scan_label_inner(
        state,
        start,
        end_max,
        references,
        depth,
        link_level,
        html_disabled,
        enable_nested,
    );
    state
        .scan_limit_cache
        .borrow_mut()
        .link_scan
        .insert_label(key, depth, outcome);
    outcome
}

#[allow(clippy::too_many_arguments)]
fn scan_label_inner(
    state: &mut InlineState<'_>,
    start: usize,
    end_max: usize,
    references: Option<&ReferenceMap>,
    depth: u32,
    mut link_level: i32,
    mut html_disabled: bool,
    enable_nested: bool,
) -> LabelOutcome {
    let fail = |link_level, html_disabled| LabelOutcome {
        close: None,
        link_level,
        html_disabled,
    };

    if state.src.as_bytes().get(start) == Some(&b']') {
        return LabelOutcome {
            close: Some(start),
            link_level,
            html_disabled,
        };
    }

    if depth > state.markdown_it().max_nesting {
        state
            .scan_limit_cache
            .borrow_mut()
            .link_scan
            .insert_cutoff(start, end_max);
        return fail(link_level, html_disabled);
    }

    // One child per label, rather than one per token: checks share pending
    // literal text and scratch while the parent cursor stays untouched.
    let Some(mut session) =
        state.child_state_at(start - state.pos..end_max - state.pos, depth, link_level)
    else {
        return fail(link_level, html_disabled);
    };
    session.suppress_bracket_links = true;
    let state = &mut session;
    let mut level = 1usize;

    while state.pos < end_max {
        let pos = state.pos;
        let ch = state.remaining().chars().next().unwrap();
        if ch == ']' {
            level -= 1;
            if level == 0 {
                return LabelOutcome {
                    close: Some(pos),
                    link_level,
                    html_disabled,
                };
            }
            state.advance_probe(1, true);
            continue;
        }

        if state
            .scan_limit_cache
            .borrow()
            .link_scan
            .cutoff(pos, end_max)
        {
            return fail(link_level, html_disabled);
        }

        if ch == '[' {
            // Failed labels are indexed by the context entering the probe.
            // HTML inside it may change the returned link level or disable
            // special HTML; looking up that returned context misses the failure
            // and rescans the same unclosed suffix at every enclosing opener.
            let nested_label = LabelKey {
                start: pos + 1,
                end: end_max,
                link_level,
                html_disabled,
                enable_nested,
            };
            let nested = scan_candidate_abs(
                state,
                pos,
                false,
                InlineTargetFallback::Reference,
                references,
                depth,
                link_level,
                html_disabled,
                true,
            );
            link_level = nested.link_level;
            html_disabled = nested.html_disabled;
            state.link_level = link_level;
            if let Some(candidate) = nested.candidate {
                if !enable_nested {
                    return fail(link_level, html_disabled);
                }
                state.advance_probe(candidate.end - pos, false);
            } else {
                // An uncloseable raw opener also blocks its enclosing label.
                // Stop here instead of probing all the same inner openers at
                // each enclosing depth on the way out of a nesting cutoff.
                let unclosed = state
                    .scan_limit_cache
                    .borrow()
                    .link_scan
                    .failed_labels
                    .contains_key(&nested_label);
                if unclosed {
                    return fail(link_level, html_disabled);
                }
                level += 1;
                state.advance_probe(1, true);
            }
            continue;
        }

        let image = ch == '!' && state.remaining().starts_with("![");
        if image {
            let nested = scan_candidate_abs(
                state,
                pos + 1,
                true,
                InlineTargetFallback::Reject,
                references,
                depth,
                link_level,
                html_disabled,
                true,
            );
            link_level = nested.link_level;
            html_disabled = nested.html_disabled;
            state.link_level = link_level;
            if let Some(candidate) = nested.candidate {
                state.advance_probe(candidate.end - pos, false);
            } else {
                state.advance_probe(1, true);
            }
            continue;
        }

        if state.probe_token().is_none() {
            return fail(link_level, html_disabled);
        }
        link_level = state.link_level;
        html_disabled = special_html_disabled(state);
    }

    fail(link_level, html_disabled)
}

/// Public-in-crate bridge used by `candidate.rs`. Positions in the returned
/// result are relative to `state.remaining()`.
pub(super) fn scan_candidate(
    state: &mut InlineState<'_>,
    pos: usize,
    enable_nested: bool,
    fallback: InlineTargetFallback,
    references: Option<&ReferenceMap>,
) -> Option<ParseLinkResult> {
    let base = state.pos;
    let outcome = scan_candidate_abs(
        state,
        base + pos,
        enable_nested,
        fallback,
        references,
        state.depth,
        state.link_level,
        special_html_disabled(state),
        false,
    );
    outcome.candidate.map(|candidate| ParseLinkResult {
        label_start: candidate.label_start - base,
        label_end: candidate.label_end - base,
        href: candidate.href,
        title: candidate.title,
        end: candidate.end - base,
    })
}

#[cfg(test)]
#[path = "tests/scan.rs"]
mod tests;
