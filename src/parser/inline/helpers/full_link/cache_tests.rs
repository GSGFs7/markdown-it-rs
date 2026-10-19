use super::*;

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
