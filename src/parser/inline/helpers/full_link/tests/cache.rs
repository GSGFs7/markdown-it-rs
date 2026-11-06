use std::any::TypeId;

use super::*;

#[test]
fn rejected_link_windows_leave_pairing_index_uninitialized() {
    use super::super::cache::has_possible_link_label_close;

    let md = MarkdownIt::new();
    let mut document = crate::document::Document::new("", crate::document::Root::new(""));
    for source in [
        "[unclosed",
        "[unclosed(target",
        "[closed]",
        "(target) [unclosed",
    ] {
        let start = source.find('[').unwrap();
        let mut state = InlineState::new(
            &mut document,
            source,
            start,
            source.len(),
            &md,
            md.inline.rules(),
            0,
            0,
        );
        assert!(
            !has_possible_link_label_close(&mut state, 0, None),
            "{source}"
        );
        let cache = state.scan_limit_cache.borrow();
        let bounds = cache.link_bounds().unwrap();
        assert!(bounds.plain_closes.is_none(), "{source}");
        assert!(bounds.can_memoize_labels(start, source.len()));
    }

    // A rejected child window must not prevent a later parent candidate from
    // initializing the source-wide index shared by both sessions.
    let source = "[no] [yes](url)";
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
    {
        let mut child = state.child_state(0..4).unwrap();
        assert!(!has_possible_link_label_close(&mut child, 0, None));
    }
    assert!(
        state
            .scan_limit_cache
            .borrow()
            .link_bounds()
            .unwrap()
            .plain_closes
            .is_none()
    );
    state.pos = 5;
    assert!(has_possible_link_label_close(&mut state, 0, None));
    let cache = state.scan_limit_cache.borrow();
    assert_eq!(
        cache
            .link_bounds()
            .unwrap()
            .plain_closes
            .as_ref()
            .unwrap()
            .get(&5),
        Some(&9)
    );
}

#[test]
fn unsupported_pairing_source_caches_the_failed_index_attempt() {
    use super::super::cache::has_possible_link_label_close;

    let md = MarkdownIt::new();
    let mut document = crate::document::Document::new("", crate::document::Root::new(""));
    let source = "[*a*](url) [b](url)";
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
    for start in [0, 11] {
        state.pos = start;
        assert!(has_possible_link_label_close(&mut state, 0, None));
        assert!(
            state
                .scan_limit_cache
                .borrow()
                .link_bounds()
                .unwrap()
                .plain_closes
                .as_ref()
                .is_some_and(std::collections::HashMap::is_empty)
        );
    }
}

#[test]
fn close_outside_child_window_does_not_trigger_pairing_index() {
    use super::super::cache::has_possible_link_label_close;

    let md = MarkdownIt::new();
    let mut document = crate::document::Document::new("", crate::document::Root::new(""));
    let source = "[no(url) ] [yes](url)";
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
    {
        let mut child = state.child_state(0..8).unwrap();
        assert!(!has_possible_link_label_close(&mut child, 0, None));
        assert!(
            child
                .scan_limit_cache
                .borrow()
                .link_bounds()
                .unwrap()
                .plain_closes
                .is_none()
        );
    }
    state.pos = source.find("[yes]").unwrap();
    assert!(has_possible_link_label_close(&mut state, 0, None));
}

#[test]
fn forced_label_scans_keep_memoization_without_building_pairing_index() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::super::cache::has_possible_link_label_close;

    #[derive(Debug, Default)]
    struct Calls(AtomicUsize);
    fn force_scan(state: &InlineState<'_>, _: usize, _: usize) -> bool {
        state
            .markdown_it()
            .ext
            .get::<Calls>()
            .unwrap()
            .0
            .fetch_add(1, Ordering::Relaxed);
        true
    }

    let mut md = MarkdownIt::new();
    md.ext.insert(Calls::default());
    crate::parser::inline::add_scan_hook(&mut md, force_scan);
    let mut document = crate::document::Document::new("", crate::document::Root::new(""));
    let source = "[unclosed";
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
    for _ in 0..2 {
        assert!(has_possible_link_label_close(&mut state, 0, None));
        assert!(scan_link_label(state.child_state(1..source.len()).unwrap(), false).is_none());
    }
    assert_eq!(md.ext.get::<Calls>().unwrap().0.load(Ordering::Relaxed), 2);
    let cache = state.scan_limit_cache.borrow();
    let bounds = cache.link_bounds().unwrap();
    assert!(bounds.plain_closes.is_none());
    assert!(!bounds.failed_labels.is_empty());
}

#[test]
fn special_html_link_scans_have_bounded_work() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct ScanBudget {
        calls: AtomicUsize,
        limit: usize,
    }

    fn count_scan(state: &InlineState<'_>, _: usize, _: usize) -> bool {
        let budget = state.markdown_it().ext.get::<ScanBudget>().unwrap();
        assert!(
            budget.calls.fetch_add(1, Ordering::Relaxed) < budget.limit,
            "special HTML caused repeated overlapping link scans"
        );
        false
    }

    for n in [32, 128, 512] {
        for opener in ["[a", "![a"] {
            for html in ["<!-- ok -->", "<?ok?>", "<![CDATA[ok]]>", "<!a>"] {
                let source = format!("{} {html}", opener.repeat(n));
                let mut md = MarkdownIt::new();
                md.ext.insert(ScanBudget {
                    calls: AtomicUsize::new(0),
                    limit: source.len() * 256,
                });
                // Count every probe, including those the HTML hook accepts.
                crate::parser::inline::add_scan_hook(&mut md, count_scan);
                crate::plugins::html::add(&mut md);
                assert_eq!(md.render(&source), format!("<p>{source}</p>\n"));
            }
        }
    }
}

/// Built-in check that is deliberately left uncached: its `check` mutates the
/// shared inline-footnote scan budget, so the same probe can return different
/// results under budget pressure.
fn is_known_uncached_check(type_id: TypeId) -> bool {
    type_id == TypeId::of::<crate::plugins::extra::footnote::FootnoteInlineScanner>()
}

#[test]
fn plain_brackets_have_a_linear_close_index() {
    let mut md = MarkdownIt::new();
    crate::plugins::html::add(&mut md);
    crate::plugins::extra::add(&mut md);
    let rules = md.inline.rules();
    let closes =
        plain_label_closes("![[]()", 6, rules).expect("plain cmark brackets support a close index");
    assert!(!closes.contains_key(&1));
    assert_eq!(closes.get(&2), Some(&3));
    assert!(plain_label_closes("[[a]](x)", 8, rules).is_some());
    for source in ["[a](x[y])", "[a](x!y)", "[a](x\\)y)"] {
        assert!(plain_label_closes(source, source.len(), rules).is_none());
    }
}

#[test]
fn indexed_labels_preserve_nested_link_and_image_rendering() {
    let md = MarkdownIt::new();
    for (source, expected) in [
        ("[[[a]]](x)", "<p><a href=\"x\">[[a]]</a></p>\n"),
        ("[[[a](x)]]", "<p>[[<a href=\"x\">a</a>]]</p>\n"),
        (
            "[![a](x)](y)",
            "<p><a href=\"y\"><img src=\"x\" alt=\"a\"></a></p>\n",
        ),
        (
            "[r]: x\n\n[[[a]]] [r]",
            "<p>[[[a]]] <a href=\"x\">r</a></p>\n",
        ),
        ("[r]: x\n\n[[a]][r]", "<p><a href=\"x\">[a]</a></p>\n"),
    ] {
        assert_eq!(md.render(source), expected, "{source}");
    }
}

#[test]
fn memoizable_checks_cover_shipped_presets() {
    fn assert_all_memoizable(md: &MarkdownIt, preset: &str) {
        for rule in &md.inline.rules().checks {
            assert!(
                is_memoizable_check(rule.type_id) || is_known_uncached_check(rule.type_id),
                "{preset}: check rule for marker {:?} is neither memoizable nor a known \
                 uncached rule; update `is_memoizable_check`",
                rule.marker,
            );
        }
    }

    assert_all_memoizable(&MarkdownIt::new(), "MarkdownItDefault");

    let mut extras = MarkdownIt::empty();
    crate::plugins::cmark::add(&mut extras);
    crate::plugins::html::add(&mut extras);
    crate::plugins::extra::add(&mut extras);
    assert_all_memoizable(&extras, "cmark+html+extra");
}

#[test]
fn memoized_labels_match_uncached_checks_across_nesting_limits() {
    struct Uncached;
    impl InlineRule for Uncached {
        const MARKER: char = '\0';
        fn run(_: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
            None
        }
    }

    let mut cached = MarkdownIt::new();
    crate::plugins::html::add(&mut cached);
    crate::plugins::extra::add(&mut cached);
    let mut uncached = MarkdownIt::new();
    crate::plugins::html::add(&mut uncached);
    crate::plugins::extra::add(&mut uncached);
    // A custom check disables both optimizations, without changing syntax.
    uncached.inline.add_rule::<Uncached>().before_all();

    for depth in [0, 1, 2, 3, 8, 20] {
        cached.max_nesting = depth;
        uncached.max_nesting = depth;
        for body in [
            "a",
            "雪",
            "*a*",
            "`[`",
            "`]`",
            "\\]",
            "<b>]</b>",
            "<a href=x>a</a>",
            "![a](x)",
            "[a](x)",
            "[r]",
            "&amp;",
            "^[note]",
            "^[^[nested]]",
        ] {
            for suffix in ["(x)", "[r]", "(/unfinished", ""] {
                for prefix in ["雪", "^"] {
                    let source = format!(
                        "[r]: /u\n\n{prefix}[[[{body}]]]{suffix} *[[[{body}]]]{suffix} ^[tail]"
                    );
                    assert_eq!(
                        cached.render(&source),
                        uncached.render(&source),
                        "depth={depth}, source={source:?}"
                    );
                }
            }
        }
    }
}
