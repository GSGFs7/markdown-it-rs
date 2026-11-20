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
use crate::reference::ReferenceMap;

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
    special_scan_disabled: bool,
}

/// Where a label probe ended.
#[derive(Debug, Clone, Copy)]
enum LabelEnd {
    /// The label closed at this offset.
    Close(usize),
    /// A nested bracket link appeared where nested links are not allowed, so
    /// the label can only fail for an enclosing probe that forbids links.
    BlockedByLink,
    /// The label never closed, or the probe hit the nesting cutoff.
    Unclosed,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LabelOutcome {
    end: LabelEnd,
    link_level: i32,
    special_scan_disabled: bool,
}

impl LabelOutcome {
    fn close(&self) -> Option<usize> {
        match self.end {
            LabelEnd::Close(pos) => Some(pos),
            LabelEnd::BlockedByLink | LabelEnd::Unclosed => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct CandidateKey {
    start: usize,
    end: usize,
    enable_nested: bool,
    fallback: InlineTargetFallback,
    link_level: i32,
    special_scan_disabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct LabelKey {
    start: usize,
    end: usize,
    link_level: i32,
    special_scan_disabled: bool,
    enable_nested: bool,
}

impl LabelKey {
    /// Key for the label body that starts directly after the `[` at `opener`.
    fn after(
        opener: usize,
        end: usize,
        link_level: i32,
        special_scan_disabled: bool,
        enable_nested: bool,
    ) -> Self {
        Self {
            start: opener + 1,
            end,
            link_level,
            special_scan_disabled,
            enable_nested,
        }
    }
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

#[derive(Debug)]
struct PlainBrackets {
    closes: std::collections::HashMap<usize, (usize, usize)>,
    /// Opening brackets before any syntax other than literal text.
    leading_openers: Vec<usize>,
    /// Number of leading openers whose labels failed at a recorded cutoff.
    rejected_openers: usize,
    source_len: usize,
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
    plain_brackets: Option<PlainBrackets>,
    plain_brackets_ready: bool,
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
        if let Some(outcome) = self.failed_label(key) {
            return Some(outcome);
        }
        let cached = self.labels.get(&key)?;
        (depth <= cached.depth && cached.epoch == self.cutoffs.len()).then_some(cached.outcome)
    }

    fn insert_label(&mut self, key: LabelKey, depth: u32, outcome: LabelOutcome) {
        if outcome.close().is_none() {
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

    fn failed_label(&self, key: LabelKey) -> Option<LabelOutcome> {
        self.failed_labels.get(&key).copied().or_else(|| {
            let plain = self.plain_brackets.as_ref()?;
            let last = *plain
                .leading_openers
                .get(plain.rejected_openers.checked_sub(1)?)?;
            (!key.enable_nested
                && key.link_level == 0
                && !key.special_scan_disabled
                && key.end == plain.source_len
                && (plain.leading_openers[0] + 1..=last + 1).contains(&key.start))
            .then_some(LabelOutcome {
                end: LabelEnd::Unclosed,
                link_level: 0,
                special_scan_disabled: false,
            })
        })
    }

    /// A run of literal text and opening brackets descends exactly once per
    /// opener. Record the first body that exceeds the budget, together with
    /// the enclosing failures, without starting a probe session per opener.
    fn reject_leading_openers(&mut self, start: usize, budget: usize) -> bool {
        let Some(plain) = self.plain_brackets.as_mut() else {
            return false;
        };
        let openers = &plain.leading_openers;
        let rejected = plain.rejected_openers;
        if rejected > 0 && start <= openers[rejected - 1] {
            return true;
        }
        if openers.get(rejected) != Some(&start)
            || plain.closes.is_empty()
            || !self.failed_labels.is_empty()
            || self.cutoffs.last().is_some_and(|&(pos, _)| pos > start)
        {
            return false;
        }
        let Some(cutoff_index) = rejected.checked_add(budget) else {
            return false;
        };
        // Keep another opener after the cutoff so its body cannot be empty.
        if cutoff_index >= openers.len().saturating_sub(1) {
            return false;
        }
        let cutoff = openers[cutoff_index] + 1;
        let end = plain.source_len;
        plain.rejected_openers = cutoff_index + 1;
        self.insert_cutoff(cutoff, end);
        true
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
        if !self.plain_brackets_ready {
            self.plain_brackets_ready = true;
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
                // Save the positions only where avoiding probe sessions can
                // offset building another index.
                let leading_openers = if source.len() >= 512
                    && rules
                        .checks
                        .iter()
                        .all(|rule| rule.marker != '\0' || rule.literal)
                {
                    source
                        .bytes()
                        .enumerate()
                        .take_while(|&(_, byte)| byte == b'[' || byte.is_ascii_alphanumeric())
                        .filter_map(|(pos, byte)| (byte == b'[').then_some(pos))
                        .collect()
                } else {
                    Vec::new()
                };
                self.plain_brackets = Some(PlainBrackets {
                    closes,
                    leading_openers,
                    rejected_openers: 0,
                    source_len: source.len(),
                });
            }
        }
        self.plain_brackets
            .as_ref()
            .map(|plain| plain.closes.get(&start).copied())
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
    let may_have_reference = references.is_some_and(|references| !references.is_empty());
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
    special_scan_disabled: bool,
    retain: bool,
) -> Outcome {
    // Impossible candidates need only the source indexes. Do not retain a
    // hash-map entry for every literal opener in a source without link syntax.
    let Some(may_have_reference) = candidate_preflight(state, start, references) else {
        return Outcome {
            candidate: None,
            link_level,
            special_scan_disabled: state.special_scan_disabled(),
        };
    };
    let key = CandidateKey {
        start,
        end: state.pos_max,
        enable_nested,
        fallback,
        link_level,
        special_scan_disabled,
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
        special_scan_disabled,
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
    special_scan_disabled: bool,
    may_have_reference: bool,
) -> Outcome {
    let end_max = state.pos_max;
    let fail = |link_level, special_scan_disabled| Outcome {
        candidate: None,
        link_level,
        special_scan_disabled,
    };

    let label_start = start + 1;
    let budget = state.markdown_it().max_nesting.saturating_sub(depth) as usize;
    let plain = {
        let mut shared = state.scan_limit_cache.borrow_mut();
        let plain = shared
            .link_scan
            .plain_close(state.src.as_ref(), state.ruleset, start);
        if plain.is_some()
            && depth == 0
            && !enable_nested
            && !may_have_reference
            && link_level == 0
            && !special_scan_disabled
            && end_max == state.src.len()
            && shared.link_scan.reject_leading_openers(start, budget)
        {
            return fail(link_level, special_scan_disabled);
        }
        plain
    };
    // `Some(close)` when raw bracket pairing alone decides the label end;
    // `None` when only a real scan can tell how the nesting budget is spent.
    let paired = match plain {
        Some(Some((close, _))) if close >= end_max => {
            return fail(link_level, special_scan_disabled);
        }
        // An empty label cannot be cut off, so it never needs the cutoff check.
        Some(Some((close, _))) if close == label_start => Some(close),
        Some(Some((close, max_depth))) if max_depth <= budget => {
            if state
                .scan_limit_cache
                .borrow()
                .link_scan
                .cutoff_before(label_start, end_max, close)
            {
                return fail(link_level, special_scan_disabled);
            }
            // A target inside the label may consume brackets differently than
            // plain pairing does, so only skip the scan when neither an inline
            // target nor a reference can appear inside it.
            let inner_has_target =
                memchr::memchr(b'(', &state.src.as_bytes()[label_start..close]).is_some();
            if inner_has_target || may_have_reference {
                None
            } else {
                Some(close)
            }
        }
        // Raw pairing cannot determine where a scan will spend its nesting budget.
        _ => None,
    };
    let (close, link_level, special_scan_disabled) = match paired {
        Some(close) => (close, link_level, special_scan_disabled),
        None => {
            let label = scan_label(
                state,
                label_start,
                end_max,
                references,
                depth.saturating_add(1),
                link_level,
                special_scan_disabled,
                enable_nested,
            );
            let Some(close) = label.close() else {
                return fail(label.link_level, label.special_scan_disabled);
            };
            (close, label.link_level, label.special_scan_disabled)
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
            special_scan_disabled,
        },
        LinkTargetScan::Reject => fail(link_level, special_scan_disabled),
        LinkTargetScan::Reference { resume } => {
            let Some(references) = references else {
                return fail(link_level, special_scan_disabled);
            };

            let explicit = if state.src.as_bytes().get(resume) == Some(&b'[') && resume < end_max {
                let reference = scan_label(
                    state,
                    resume + 1,
                    end_max,
                    Some(references),
                    depth.saturating_add(1),
                    link_level,
                    special_scan_disabled,
                    true,
                );
                reference.close().map(|close| (close, reference))
            } else {
                None
            };

            let (label, end, link_level, special_scan_disabled) = match explicit {
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
                        reference.special_scan_disabled,
                    )
                }
                None => {
                    let source = state.src.as_ref();
                    (
                        &source[label_start..close],
                        close + 1,
                        link_level,
                        special_scan_disabled,
                    )
                }
            };

            let Some((href, title)) = references.get(label) else {
                return fail(link_level, special_scan_disabled);
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
                special_scan_disabled,
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
    special_scan_disabled: bool,
    enable_nested: bool,
) -> LabelOutcome {
    let key = LabelKey {
        start,
        end: end_max,
        link_level,
        special_scan_disabled,
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
        special_scan_disabled,
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
    mut special_scan_disabled: bool,
    enable_nested: bool,
) -> LabelOutcome {
    let fail = |link_level, special_scan_disabled| LabelOutcome {
        end: LabelEnd::Unclosed,
        link_level,
        special_scan_disabled,
    };

    if state.src.as_bytes().get(start) == Some(&b']') {
        return LabelOutcome {
            end: LabelEnd::Close(start),
            link_level,
            special_scan_disabled,
        };
    }

    if depth > state.markdown_it().max_nesting {
        state
            .scan_limit_cache
            .borrow_mut()
            .link_scan
            .insert_cutoff(start, end_max);
        return fail(link_level, special_scan_disabled);
    }

    // One child per label, rather than one per token: checks share pending
    // literal text and scratch while the parent cursor stays untouched.
    let Some(mut session) =
        state.child_state_at(start - state.pos..end_max - state.pos, depth, link_level)
    else {
        return fail(link_level, special_scan_disabled);
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
                    end: LabelEnd::Close(pos),
                    link_level,
                    special_scan_disabled,
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
            return fail(link_level, special_scan_disabled);
        }

        if ch == '[' {
            // Failed labels are indexed by the context entering the probe.
            // HTML inside it may change the returned link level or disable
            // special HTML; looking up that returned context misses the failure
            // and rescans the same unclosed suffix at every enclosing opener.
            let nested_allows_links = false;
            let nested_label = LabelKey::after(
                pos,
                end_max,
                link_level,
                special_scan_disabled,
                nested_allows_links,
            );
            let nested = scan_candidate_abs(
                state,
                pos,
                nested_allows_links,
                InlineTargetFallback::Reference,
                references,
                depth,
                link_level,
                special_scan_disabled,
                true,
            );
            link_level = nested.link_level;
            special_scan_disabled = nested.special_scan_disabled;
            state.link_level = link_level;
            if let Some(candidate) = nested.candidate {
                if !enable_nested {
                    return LabelOutcome {
                        end: LabelEnd::BlockedByLink,
                        ..fail(link_level, special_scan_disabled)
                    };
                }
                state.advance_probe(candidate.end - pos, false);
            } else {
                // A failed plain link label can terminate the outer scan too,
                // unless this outer label accepts the link that blocked it.
                let failed = state
                    .scan_limit_cache
                    .borrow()
                    .link_scan
                    .failed_label(nested_label);
                if let Some(failed) = failed
                    && (!enable_nested || !matches!(failed.end, LabelEnd::BlockedByLink))
                {
                    return LabelOutcome { ..failed };
                }
                level += 1;
                state.advance_probe(1, true);
            }
            continue;
        }

        let image = ch == '!' && state.remaining().starts_with("![");
        if image {
            let image_label =
                LabelKey::after(pos + 1, end_max, link_level, special_scan_disabled, true);
            let nested = scan_candidate_abs(
                state,
                pos + 1,
                true,
                InlineTargetFallback::Reject,
                references,
                depth,
                link_level,
                special_scan_disabled,
                true,
            );
            link_level = nested.link_level;
            special_scan_disabled = nested.special_scan_disabled;
            state.link_level = link_level;
            if let Some(candidate) = nested.candidate {
                state.advance_probe(candidate.end - pos, false);
            } else if state
                .scan_limit_cache
                .borrow()
                .link_scan
                .failed_label(image_label)
                .is_some()
            {
                // A label that fails even with nested links allowed also
                // blocks the outer label when its opener is treated as raw.
                return fail(link_level, special_scan_disabled);
            } else {
                state.advance_probe(1, true);
            }
            continue;
        }

        if state.probe_token().is_none() {
            return fail(link_level, special_scan_disabled);
        }
        link_level = state.link_level;
        special_scan_disabled = state.special_scan_disabled();
    }

    fail(link_level, special_scan_disabled)
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
        state.special_scan_disabled(),
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
