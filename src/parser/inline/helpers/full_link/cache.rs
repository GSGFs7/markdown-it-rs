use std::any::TypeId;
use std::collections::{HashMap, HashSet};

use super::{LinkPrefixScanner, LinkScanner, LinkScannerEnd};
use crate::parser::inline::{InlineState, scan_hooks_require_scan};
use crate::plugins::cmark::block::reference::ReferenceMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct LabelScanKey {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) depth: u32,
    pub(super) link_level: i32,
    pub(super) enable_nested: bool,
}

#[derive(Debug)]
pub(crate) struct LinkLabelBounds {
    pub(super) close_positions: Vec<usize>,
    pub(super) plain_closes: Option<HashMap<usize, usize>>,
    pub(super) target_positions: Vec<usize>,
    pub(super) non_memoizable_positions: Option<Vec<usize>>,
    pub(super) labels: HashMap<LabelScanKey, (usize, usize)>,
    pub(super) failed_labels: HashSet<(usize, usize, i32, bool)>,
}

impl LinkLabelBounds {
    pub(crate) fn can_memoize_labels(&self, start: usize, end: usize) -> bool {
        let Some(positions) = &self.non_memoizable_positions else {
            return false;
        };
        let next = positions.partition_point(|&pos| pos < start);
        positions.get(next).is_none_or(|&pos| pos >= end)
    }
}

/// Index markers that prevent memoization within a label scan's window.
fn non_memoizable_positions(source: &str, state: &InlineState<'_>) -> Option<Vec<usize>> {
    use crate::plugins::cmark::block::reference::DefaultReferenceMap;

    if state
        .root_ext
        .and_then(|ext| ext.get::<ReferenceMap>())
        .is_some_and(|map| map.downcast_ref::<DefaultReferenceMap>().is_none())
    {
        return None;
    }

    let uncached_rules: Vec<_> = state
        .ruleset
        .checks
        .iter()
        .filter(|rule| !is_memoizable_check(rule.type_id))
        .collect();
    if uncached_rules.is_empty() {
        return Some(Vec::new());
    }

    Some(
        source
            .char_indices()
            .filter(|&(_, ch)| uncached_rules.iter().any(|rule| rule.matches_marker(ch)))
            .map(|(pos, _)| pos)
            .collect(),
    )
}

/// Check rules whose `check` is a pure boundary probe.
///
/// A memoized label scan skips re-running checks, so this list must cover every
/// built-in rule that can run inside a label. The
/// `memoizable_checks_cover_shipped_presets` test derives that set from the
/// shipped parsers, so a missing rule fails instead of silently disabling the
/// cache (which would restore the quadratic blowup). Mutable built-ins such as
/// `FootnoteInlineScanner` are deliberately absent.
pub(super) fn is_memoizable_check(type_id: TypeId) -> bool {
    use crate::parser::inline::TextScanner;
    use crate::parser::inline::helpers::code_pair::CodePairScanner;
    use crate::parser::inline::helpers::emph_pair::EmphPairScanner;
    use crate::plugins::cmark::inline::autolink::AutolinkScanner;
    use crate::plugins::cmark::inline::entity::EntityScanner;
    use crate::plugins::cmark::inline::escape::EscapeScanner;
    use crate::plugins::cmark::inline::newline::NewlineScanner;
    use crate::plugins::directives::TextDirective;
    use crate::plugins::extra::footnote::FootnoteReferenceScanner;
    use crate::plugins::html::html_inline::HtmlInlineScanner;

    [
        TypeId::of::<LinkScanner<false>>(),
        TypeId::of::<LinkPrefixScanner<'!', true>>(),
        TypeId::of::<LinkScannerEnd>(),
        TypeId::of::<TextScanner>(),
        TypeId::of::<FootnoteReferenceScanner>(),
        TypeId::of::<CodePairScanner<'`'>>(),
        TypeId::of::<EmphPairScanner<'*', true>>(),
        TypeId::of::<EmphPairScanner<'_', false>>(),
        TypeId::of::<EmphPairScanner<'~', true>>(),
        TypeId::of::<EmphPairScanner<'=', true>>(),
        TypeId::of::<EscapeScanner>(),
        TypeId::of::<EntityScanner>(),
        TypeId::of::<NewlineScanner>(),
        TypeId::of::<AutolinkScanner>(),
        TypeId::of::<HtmlInlineScanner>(),
        TypeId::of::<TextDirective>(),
        #[cfg(feature = "linkify")]
        TypeId::of::<crate::plugins::extra::linkify::LinkifyScanner>(),
        #[cfg(feature = "linkify")]
        TypeId::of::<crate::plugins::extra::linkify::LinkifyFuzzyScanner>(),
        #[cfg(feature = "linkify")]
        TypeId::of::<crate::plugins::extra::linkify::LinkifyEmailScanner>(),
        #[cfg(feature = "katex")]
        TypeId::of::<crate::plugins::extra::math::MathInlineScanner>(),
    ]
    .contains(&type_id)
}

/// Index label ends directly, bypassing check sessions for plain input.
///
/// Returns `None` when the source contains syntax the index pass cannot
/// handle; label ends must then be scanned normally.
pub(super) fn plain_label_closes(
    source: &str,
    end: usize,
    rules: &crate::parser::inline::RuleSet,
) -> Option<HashMap<usize, usize>> {
    let source = &source[..end];
    if !is_plain_label_source(source) || !has_only_supported_checks(source, rules) {
        return None;
    }
    Some(index_bracket_closes(source))
}

/// Whether the source is simple enough to index directly.
///
/// Parenthesized text must contain no brackets or image markers: a checked
/// target can hide those bytes and change the raw bracket pairing.
fn is_plain_label_source(source: &str) -> bool {
    let bytes = source.as_bytes();
    let is_plain = |byte: u8| {
        byte.is_ascii_alphanumeric() || byte.is_ascii_whitespace() || b"[]!()".contains(&byte)
    };
    if !bytes.iter().all(|&byte| is_plain(byte)) {
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

/// Whether every check rule triggered by this source is handled by the index pass.
fn has_only_supported_checks(source: &str, rules: &crate::parser::inline::RuleSet) -> bool {
    rules.checks.iter().all(|rule| {
        is_index_supported(rule.type_id) || !source.chars().any(|ch| rule.matches_marker(ch))
    })
}

fn is_index_supported(type_id: std::any::TypeId) -> bool {
    use std::any::TypeId;
    [
        TypeId::of::<LinkScanner<false>>(),
        TypeId::of::<LinkPrefixScanner<'!', true>>(),
        TypeId::of::<LinkScannerEnd>(),
        TypeId::of::<crate::parser::inline::TextScanner>(),
        TypeId::of::<crate::plugins::extra::footnote::FootnoteReferenceScanner>(),
    ]
    .contains(&type_id)
}

/// Map each `[` byte index to its matching `]`.
fn index_bracket_closes(source: &str) -> HashMap<usize, usize> {
    let mut open = Vec::new();
    let mut closes = HashMap::new();
    for (index, byte) in source.bytes().enumerate() {
        match byte {
            b'[' => open.push(index),
            b']' => {
                if let Some(open) = open.pop() {
                    closes.insert(open, index);
                }
            }
            _ => {}
        }
    }
    closes
}

pub(super) fn has_possible_link_label_close(
    state: &mut InlineState<'_>,
    offset: usize,
    references: Option<&ReferenceMap>,
) -> bool {
    if !state
        .remaining()
        .get(offset..)
        .is_some_and(|tail| tail.starts_with('['))
    {
        return false;
    }

    let source = state.src.as_ref();
    let end = state.pos_max;
    let open = state.pos + offset;
    // Hooks may borrow the shared scan state, so run them before borrowing the
    // cache, but always initialize link bounds before accepting their request.
    let requires_scan = scan_hooks_require_scan(state, open, end);
    let mut shared = state.scan_limit_cache.borrow_mut();
    let cache = shared.link_bounds_or_insert_with(|| LinkLabelBounds {
        close_positions: memchr::memchr_iter(b']', source.as_bytes()).collect(),
        plain_closes: None,
        target_positions: memchr::memchr_iter(b'(', source.as_bytes()).collect(),
        non_memoizable_positions: non_memoizable_positions(source, state),
        labels: HashMap::new(),
        failed_labels: HashSet::new(),
    });

    // Preserve source-wide check side effects without bypassing memoization.
    // Otherwise nested unclosed labels repeatedly scan overlapping suffixes.
    if requires_scan {
        return true;
    }

    let last_before_end = |positions: &[usize]| {
        positions[..positions.partition_point(|&pos| pos < end)]
            .last()
            .copied()
    };

    if !last_before_end(&cache.target_positions).is_some_and(|last| last > open)
        && references.is_none()
    {
        return false;
    }

    // Reject impossible candidates before building the optional pairing index.
    // Keep link bounds and memoization available even when a hook requires a
    // scan, but do not build an index that the hook path never consults.
    if cache.plain_closes.is_none()
        && !last_before_end(&cache.close_positions).is_some_and(|last| last > open)
    {
        return false;
    }

    let closes = cache.plain_closes.get_or_insert_with(|| {
        plain_label_closes(source, source.len(), state.ruleset).unwrap_or_default()
    });
    let Some(&close) = closes.get(&open).filter(|&&close| close < end) else {
        return last_before_end(&cache.close_positions).is_some_and(|last| last > open);
    };

    if state.depth >= state.markdown_it().max_nesting || source[open + 1..close].contains('[') {
        return true;
    }

    if source[close + 1..end].starts_with(['(', '[']) {
        return true;
    }

    references.is_some_and(|map| {
        map.downcast_ref::<crate::plugins::cmark::block::reference::DefaultReferenceMap>()
            .is_none_or(|map| {
                use crate::plugins::cmark::block::reference::CustomReferenceMap;
                map.get(&source[open + 1..close]).is_some()
            })
    })
}

pub(super) fn cache_label_scan(context: &InlineState<'_>, key: LabelScanKey, close: Option<usize>) {
    let mut shared = context.scan_limit_cache.borrow_mut();
    let epoch = shared.epoch();
    // Only reached while `memoize` is set, which requires `link_bounds`.
    let bounds = shared
        .link_bounds_mut()
        .expect("label memoization requires link bounds");
    if close.is_none() {
        bounds
            .failed_labels
            .insert((key.start, key.end, key.link_level, key.enable_nested));
        return;
    }
    bounds.labels.insert(key, (epoch, close.unwrap()));
}
