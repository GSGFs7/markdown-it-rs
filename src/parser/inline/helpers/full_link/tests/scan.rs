use super::*;
use crate::MarkdownIt;
use crate::document::{Document, Root};

#[test]
fn unclosed_labels_with_html_side_effects_have_bounded_work() {
    use std::cell::Cell;

    use crate::parser::inline::InlineRule;
    use crate::plugins::html::html_inline::HtmlInlineScanner;

    thread_local! {
        static CHECKS: Cell<usize> = const { Cell::new(0) };
    }
    struct CountHtml;
    impl InlineRule for CountHtml {
        const MARKER: char = '<';

        fn check(_: &mut InlineState<'_>) -> Option<usize> {
            CHECKS.with(|checks| {
                checks.set(checks.get() + 1);
                assert!(checks.get() <= 4096, "repeated unclosed label scans");
            });
            None
        }

        fn run(_: &mut InlineState<'_>) -> Option<(Option<crate::document::NodeId>, usize)> {
            None
        }
    }

    let mut md = MarkdownIt::empty();
    crate::plugins::cmark::add(&mut md);
    crate::plugins::html::add(&mut md);
    crate::plugins::extra::add(&mut md);
    md.inline
        .add_rule::<CountHtml>()
        .before::<HtmlInlineScanner>();
    let seed = String::from_utf8_lossy(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fuzz/seeds/extras_render/unclosed-anchor-label-regression.md"
    )));
    for source in [
        seed.into_owned(),
        format!("{}<a href='/'>text](x)", "[".repeat(64)),
        format!("{}<?broken](x)", "[".repeat(64)),
    ] {
        CHECKS.with(|checks| checks.set(0));
        md.render(&source);
    }
}

#[test]
fn label_probe_preserves_unmatched_marker_runs() {
    let md = MarkdownIt::new();
    for ticks in [2, 3, 32] {
        let label = format!("{}a", "`".repeat(ticks));
        let source = format!("[{label}](/url) `");
        assert_eq!(
            md.render(&source),
            format!("<p><a href=\"/url\">{label}</a> `</p>\n"),
        );
    }
}

#[test]
fn impossible_candidates_do_not_accumulate_cache_entries() {
    let md = MarkdownIt::new();
    for source in ["[a".repeat(256), "[label] ".repeat(256)] {
        let mut document = Document::new("", Root::new(""));
        let mut state = InlineState::new(
            &mut document,
            &source,
            0,
            source.len(),
            &md,
            md.inline.rules(),
            0,
            0,
        );
        for (pos, _) in source.match_indices('[') {
            state.pos = pos;
            assert!(
                scan_candidate(&mut state, 0, false, InlineTargetFallback::Reference, None)
                    .is_none()
            );
        }
        let shared = state.scan_limit_cache.borrow();
        assert!(shared.link_scan.candidates.is_empty());
        assert!(shared.link_scan.labels.is_empty());
    }
}

#[test]
fn nested_failures_have_a_source_sized_cache() {
    let mut md = MarkdownIt::new();
    md.max_nesting = 20;
    for body in ["雪a", "a `]`"] {
        let n = 256;
        let source = format!("{}{body}{}(x)", "[".repeat(n), "]".repeat(n));
        let mut document = Document::new("", Root::new(""));
        let mut state = InlineState::new(
            &mut document,
            &source,
            0,
            source.len(),
            &md,
            md.inline.rules(),
            0,
            0,
        );
        for pos in 0..n {
            state.pos = pos;
            scan_candidate(&mut state, 0, false, InlineTargetFallback::Reference, None);
        }
        let shared = state.scan_limit_cache.borrow();
        let cache = &shared.link_scan;
        assert!(
            cache.candidates.len() + cache.labels.len() + cache.failed_labels.len()
                <= 4 * source.len(),
            "{body}: overlapping failures retained too many scan results: {} candidates, {} labels, {} failed labels",
            cache.candidates.len(),
            cache.labels.len(),
            cache.failed_labels.len(),
        );
    }
}

#[test]
fn reused_boundaries_respect_nesting_budgets_and_new_cutoffs() {
    let mut md = MarkdownIt::new();
    md.max_nesting = 3;
    let source = "[雪](/url)";
    let mut document = Document::new("", Root::new(""));
    let mut state = InlineState::new(
        &mut document,
        source,
        0,
        source.len(),
        &md,
        md.inline.rules(),
        2,
        0,
    );
    // A boundary established with the smaller remaining budget also fits
    // shallower probes; it must not authorize a deeper one.
    for depth in [2, 1, 0] {
        state.depth = depth;
        assert_eq!(
            scan_candidate(&mut state, 0, false, InlineTargetFallback::Reference, None)
                .unwrap()
                .end,
            source.len(),
        );
        assert_eq!(state.remaining(), source);
    }
    state.depth = 3;
    assert!(scan_candidate(&mut state, 0, false, InlineTargetFallback::Reference, None).is_none());
    // A cutoff is source-wide for this window: the earlier successful
    // cache entry cannot revive the label at a shallower depth.
    state.depth = 0;
    assert!(scan_candidate(&mut state, 0, false, InlineTargetFallback::Reference, None).is_none());
}

#[test]
fn sequential_outer_candidates_do_not_retain_owned_results() {
    let md = MarkdownIt::new();
    let source = "[label](url) ".repeat(256);
    let mut document = Document::new("", Root::new(""));
    let mut state = InlineState::new(
        &mut document,
        &source,
        0,
        source.len(),
        &md,
        md.inline.rules(),
        0,
        0,
    );
    for (pos, _) in source.match_indices('[') {
        state.pos = pos;
        assert!(
            scan_candidate(&mut state, 0, false, InlineTargetFallback::Reference, None,).is_some()
        );
    }
    assert!(
        state
            .scan_limit_cache
            .borrow()
            .link_scan
            .candidates
            .is_empty()
    );
}

#[test]
fn outer_scan_can_reuse_a_retained_nested_candidate() {
    let md = MarkdownIt::new();
    let source = "[[x](inner)](outer)";
    let mut document = Document::new("", Root::new(""));
    let mut state = InlineState::new(
        &mut document,
        source,
        0,
        source.len(),
        &md,
        md.inline.rules(),
        0,
        0,
    );
    // A link inside the label prevents the outer link, but its boundary
    // remains useful when the dispatcher subsequently reaches that opener.
    assert!(scan_candidate(&mut state, 0, false, InlineTargetFallback::Reference, None,).is_none());
    assert!(
        !state
            .scan_limit_cache
            .borrow()
            .link_scan
            .candidates
            .is_empty()
    );
    state.pos = 1;
    let candidate =
        scan_candidate(&mut state, 0, false, InlineTargetFallback::Reference, None).unwrap();
    assert_eq!(candidate.href.as_deref(), Some("inner"));
    assert_eq!(candidate.end, "[x](inner)".len());
}
