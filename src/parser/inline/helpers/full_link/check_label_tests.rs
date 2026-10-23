use super::*;
use crate::document::Text;
use crate::parser::inline::InlineRule;
use crate::plugins::cmark::inline::link::Link;
use crate::{InlineState, MarkdownIt, NodeId};

// Register both brackets so the built-in text classifier stops at them.
struct Bracket<const C: char>;
impl<const C: char> InlineRule for Bracket<C> {
    const MARKER: char = C;
    fn check(_: &mut InlineState<'_>) -> Option<usize> {
        None
    }
    fn run(_: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        panic!("the consumer must consume the complete test input")
    }
}

struct Consumer<const NESTED: bool>;
impl<const NESTED: bool> InlineRule for Consumer<NESTED> {
    const MARKER: char = '@';
    fn run(state: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        if !state.remaining().starts_with("@[") {
            return None;
        }
        let len = state.remaining().len();
        let expected_source = state.remaining()[1..].to_owned();
        let mut parent = state
            .child_state(1..len, state.depth.saturating_add(1), state.link_level)
            .unwrap();
        let result = parent
            .child_state(
                1..parent.remaining().len(),
                parent.depth.saturating_add(1),
                parent.link_level,
            )
            .and_then(|child| scan_link_label(child, NESTED));
        assert_eq!(parent.remaining(), expected_source);
        assert_eq!(parent.trailing_text(), "");
        assert_eq!(parent.link_level, 0);
        let content = match result {
            Some(end) => format!("end={end}"),
            None => "none".to_owned(),
        };
        Some((Some(state.document.create_node(Text { content })), len))
    }
}

fn parser<const NESTED: bool>() -> MarkdownIt {
    let mut md = MarkdownIt::empty();
    crate::plugins::cmark::block::paragraph::add(&mut md);
    md.inline
        .add_rule::<crate::parser::inline::TextScanner>()
        .before_all();
    md.inline.add_rule::<Bracket<'['>>();
    md.inline.add_rule::<Bracket<']'>>();
    md.inline.add_rule::<Consumer<NESTED>>();
    crate::plugins::cmark::inline::escape::add(&mut md);
    crate::plugins::cmark::inline::backticks::add(&mut md);
    crate::plugins::cmark::inline::entity::add(&mut md);
    crate::plugins::html::html_inline::add(&mut md);
    md
}

fn render(md: &MarkdownIt, source: &str) -> String {
    md.render(source)
}

/// Label bodies shared by the raw-label and combined-candidate tests.
///
/// A `]` inside a code span, escape, entity, or raw HTML must not close
/// the label; raw nested `[` bytes still increase the bracket level.
const LABELS: &[&str] = &[
    "",
    "abc",
    "雪[a]雨",
    "`]`",
    r"\]",
    "&#93;",
    "<i x=']'>",
    "<a>x",
];

#[test]
fn labels_respect_raw_brackets_and_opaque_spans() {
    let md = parser::<false>();
    for label in LABELS {
        let source = format!("@[{label}]");
        assert_eq!(
            render(&md, &source),
            format!("<p>end={}</p>\n", label.len()),
            "{source}"
        );
    }
    assert_eq!(render(&md, "@[a[b]"), "<p>none</p>\n");
    assert_eq!(render(&md, "@[abc"), "<p>none</p>\n");
}

struct DefaultCheck;
impl InlineRule for DefaultCheck {
    const MARKER: char = '?';
    fn run(_: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        None
    }
}

struct Closing;
impl InlineRule for Closing {
    const MARKER: char = ']';
    fn check(_: &mut InlineState<'_>) -> Option<usize> {
        panic!("terminal bracket must be checked before dispatch")
    }
    fn run(_: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        panic!("consumer owns input")
    }
}

#[test]
fn terminal_close_precedes_dispatch_and_does_not_scan_suffix() {
    let mut md = parser::<false>();
    md.inline.add_rule::<Closing>();
    md.inline.add_rule::<DefaultCheck>();
    assert_eq!(render(&md, "@[x]?"), "<p>end=1</p>\n");
    assert_eq!(render(&md, "@[?]"), "<p>end=1</p>\n");
}

struct BracketToken;
impl InlineRule for BracketToken {
    const MARKER: char = '[';
    fn check(context: &mut InlineState<'_>) -> Option<usize> {
        if context.remaining().starts_with("[x]") {
            // Legacy uses consumed length, even if a custom rule calls it Text.
            Some(3)
        } else {
            None
        }
    }
    fn run(_: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        panic!("consumer owns input")
    }
}

#[test]
fn nested_flag_preserves_consumption_rule() {
    let mut no = parser::<false>();
    no.inline.add_rule::<BracketToken>();
    assert_eq!(render(&no, "@[[x]]"), "<p>none</p>\n");
    let mut yes = parser::<true>();
    yes.inline.add_rule::<BracketToken>();
    assert_eq!(render(&yes, "@[[x]]"), "<p>end=3</p>\n");
}

#[test]
fn recursive_entry_at_depth_limit_yields_no_label() {
    let mut md = parser::<false>();
    md.max_nesting = 1;
    // An immediately closing bracket does not call `skipToken`, so it is
    // still accepted at the nesting limit.
    assert_eq!(render(&md, "@[]"), "<p>end=0</p>\n");
    // A non-empty label must be scanned with `skipToken`, which records
    // the failure once the nesting limit is reached.
    assert_eq!(render(&md, "@[x]"), "<p>none</p>\n");
}

#[test]
fn registered_link_and_image_check_respect_remaining_depth() {
    let mut document = crate::Document::new("", crate::Root::new(""));
    let mut md = MarkdownIt::empty();
    crate::plugins::cmark::inline::link::add(&mut md);
    crate::plugins::cmark::inline::image::add(&mut md);
    md.max_nesting = 3;
    let rules = md.inline.rules();

    for source in ["[雪](/url)", "[](/url)", "![雪](/img)", "![](/img)"] {
        for depth in [1, 2, 3] {
            let mut context =
                InlineState::new(&mut document, source, 0, source.len(), &md, rules, depth, 0);
            let result = if source.starts_with('!') {
                <LinkPrefixScanner<'!', true> as InlineRule>::check(&mut context)
            } else {
                <LinkScanner<false> as InlineRule>::check(&mut context)
            };
            // `depth` is the rule level of the check; a non-empty label
            // is rejected once that level reaches `maxNesting`. An empty
            // label closes before `skipToken` and is still accepted.
            let empty_label = source.starts_with("[]") || source.starts_with("![]");
            let expected = if depth < 3 || empty_label {
                Some(source.len())
            } else {
                None
            };
            assert_eq!(result, expected, "depth={depth}, source={source:?}");
            assert_eq!(context.remaining(), source);
            assert_eq!(context.depth, depth);
            assert_eq!(context.link_level, 0);
        }
    }
}

fn utf8_parser<const PREFIX: char>() -> MarkdownIt {
    let mut md = MarkdownIt::empty();
    crate::plugins::cmark::add(&mut md);
    super::add_prefix::<PREFIX, true>(&mut md, |document, href, title| {
        document.create_node(Link {
            url: href.unwrap_or_default(),
            title,
        })
    });
    md
}

#[test]
fn custom_prefix_policies_are_independent_of_marker_and_label_context() {
    fn verify<const PREFIX: char>() {
        for fallback in [
            InlineTargetFallback::Reference,
            InlineTargetFallback::Reject,
        ] {
            for label_context in [LabelContext::NestedLink, LabelContext::Independent] {
                let mut md = utf8_parser::<PREFIX>();
                add_prefix_with_options::<PREFIX, true>(
                    &mut md,
                    LinkOptions {
                        inline_target_fallback: fallback,
                        label_context,
                    },
                    |document, href, title| {
                        document.create_node(Link {
                            url: href.unwrap_or_default(),
                            title,
                        })
                    },
                );
                let source = format!("{PREFIX}[foo](/unfinished");
                let mut references = ReferenceMap::default();
                references.insert("foo".into(), "/url".into(), None);
                let mut root_ext = crate::common::extset::RootExtSet::default();
                root_ext.insert(references);
                let mut document = crate::Document::new("", crate::Root::new(""));
                let rules = md.inline.rules();
                let mut context =
                    InlineState::new(&mut document, &source, 0, source.len(), &md, rules, 0, 0);
                context.root_ext = Some(&root_ext);
                let expected_end = match fallback {
                    InlineTargetFallback::Reference => Some(PREFIX.len_utf8() + "[foo]".len()),
                    InlineTargetFallback::Reject => None,
                };
                assert_eq!(
                    LinkPrefixScanner::<PREFIX, true>::check(&mut context),
                    expected_end
                );
                let retained_prefix = if fallback == InlineTargetFallback::Reject {
                    PREFIX.to_string()
                } else {
                    String::new()
                };
                assert_eq!(
                    md.render(&format!("{source}\n\n[foo]: /url\n")),
                    format!("<p>{retained_prefix}<a href=\"/url\">foo</a>(/unfinished</p>\n"),
                );
                assert_eq!(
                    md.render(&format!("{PREFIX}[foo](/inline)")),
                    "<p><a href=\"/inline\">foo</a></p>\n",
                );
                assert_eq!(
                    md.render(&format!("{PREFIX}[foo][ref]\n\n[ref]: /url\n")),
                    "<p><a href=\"/url\">foo</a></p>\n",
                );
            }
        }
    }
    verify::<'~'>();
    verify::<'雪'>();
    verify::<'!'>();
}

#[test]
fn custom_prefix_uses_utf8_byte_length() {
    for (source, html) in [
        ("雪[文字](/url)", "<p><a href=\"/url\">文字</a></p>\n"),
        (
            "雪[文字][ref]\n\n[ref]: /url",
            "<p><a href=\"/url\">文字</a></p>\n",
        ),
        ("雪[文字]", "<p>雪[文字]</p>\n"),
    ] {
        assert_eq!(utf8_parser::<'雪'>().render(source), html, "{source}");
    }

    assert_eq!(
        utf8_parser::<'😀'>().render("😀[x](/url)"),
        "<p><a href=\"/url\">x</a></p>\n",
    );
    assert_eq!(
        utf8_parser::<'~'>().render("~[x](/url)"),
        "<p><a href=\"/url\">x</a></p>\n",
    );
}

#[test]
fn label_scan_checks_unicode_prefixed_rule() {
    // Image label scanning calls the custom rule's check before its run.
    // The nested custom link contributes its text to the image alt value.
    assert_eq!(
        utf8_parser::<'雪'>().render("![雪[x](/inner)](/image)"),
        "<p><img src=\"/image\" alt=\"x\"></p>\n",
    );
}

#[test]
fn candidate_combines_label_target_and_reference() {
    let mut document = crate::Document::new("", crate::Root::new(""));
    let md = parser::<false>();
    let ruleset = md.inline.rules();
    let mut references = ReferenceMap::default();
    references.insert("x".into(), "/shortcut".into(), None);
    references.insert("ref".into(), "/ref".into(), Some("t".into()));

    for (source, href, title, end) in [
        ("[x](/inline)", "/inline", None, 12),
        ("[x][ref]", "/ref", Some("t"), 8),
        ("[x][]", "/shortcut", None, 5),
        ("[x]", "/shortcut", None, 3),
        ("[x][", "/shortcut", None, 3),
        ("[x](/unfinished", "/shortcut", None, 3),
    ] {
        let mut context =
            InlineState::new(&mut document, source, 0, source.len(), &md, ruleset, 0, 0);
        let result = scan_link_candidate(
            &mut context,
            0,
            false,
            Some(&references),
            InlineTargetFallback::Reference,
        )
        .expect(source);
        assert_eq!((result.label_start, result.label_end), (1, 2));
        assert_eq!(result.href.as_deref(), Some(href), "{source}");
        assert_eq!(result.title.as_deref(), title, "{source}");
        assert_eq!(result.end, end, "{source}");
        assert_eq!(context.remaining(), source);
        assert_eq!(context.trailing_text(), "");
        assert_eq!(context.link_level, 0);
    }

    for source in [
        "[x][missing]",
        "[missing]",
        "[unclosed",
        "[x](",
        "[x]( \t\n",
    ] {
        let mut context =
            InlineState::new(&mut document, source, 0, source.len(), &md, ruleset, 0, 0);
        assert!(
            scan_link_candidate(
                &mut context,
                0,
                false,
                Some(&references),
                InlineTargetFallback::Reference
            )
            .is_none()
        );
    }
}

#[test]
fn candidate_offsets_are_relative_to_current_window() {
    let mut document = crate::Document::new("", crate::Root::new(""));
    let md = parser::<false>();
    let ruleset = md.inline.rules();
    let source = "前雪[x](/url)";
    let mut context = InlineState::new(
        &mut document,
        source,
        "前".len(),
        source.len(),
        &md,
        ruleset,
        0,
        0,
    );
    let result = scan_link_candidate(
        &mut context,
        "雪".len(),
        false,
        None,
        InlineTargetFallback::Reference,
    )
    .unwrap();
    assert_eq!(
        (result.label_start, result.label_end, result.end),
        (4, 5, 12)
    );
    assert_eq!(result.href.as_deref(), Some("/url"));
    assert_eq!(context.remaining(), "雪[x](/url)");
    assert!(
        scan_link_candidate(
            &mut context,
            1,
            false,
            None,
            InlineTargetFallback::Reference
        )
        .is_none()
    );
}

#[test]
fn candidate_combines_opaque_labels_with_target() {
    let mut document = crate::Document::new("", crate::Root::new(""));
    let md = parser::<false>();
    let ruleset = md.inline.rules();
    for label in LABELS {
        let source = format!("[{label}](/url)");
        let mut context =
            InlineState::new(&mut document, &source, 0, source.len(), &md, ruleset, 0, 0);
        let result = scan_link_candidate(
            &mut context,
            0,
            false,
            None,
            InlineTargetFallback::Reference,
        )
        .unwrap_or_else(|| panic!("{source}"));
        // `]` bytes inside opaque spans must not shorten the label.
        assert_eq!(
            (result.label_start, result.label_end),
            (1, label.len() + 1),
            "{source}"
        );
        assert_eq!(result.href.as_deref(), Some("/url"), "{source}");
        assert_eq!(result.title, None, "{source}");
        assert_eq!(result.end, source.len(), "{source}");
    }
}

#[test]
fn candidate_requires_reference_map_and_label_window() {
    let mut document = crate::Document::new("", crate::Root::new(""));
    let md = parser::<false>();
    let ruleset = md.inline.rules();

    // Inline targets parse without references, shortcut references do not.
    let source = "[x](/url)";
    let mut context = InlineState::new(&mut document, source, 0, source.len(), &md, ruleset, 0, 0);
    let result = scan_link_candidate(
        &mut context,
        0,
        false,
        None,
        InlineTargetFallback::Reference,
    )
    .expect(source);
    assert_eq!(result.href.as_deref(), Some("/url"));
    assert_eq!(result.end, source.len());

    for source in ["[x]", "[x][ref]"] {
        let mut context =
            InlineState::new(&mut document, source, 0, source.len(), &md, ruleset, 0, 0);
        assert!(
            scan_link_candidate(
                &mut context,
                0,
                false,
                None,
                InlineTargetFallback::Reference
            )
            .is_none(),
            "{source}"
        );
    }

    // Label scanning consumes one nesting level; when the budget is spent
    // the whole candidate is rejected even though the syntax is valid.
    let mut md = parser::<false>();
    md.max_nesting = 2;
    let ruleset = md.inline.rules();
    let source = "[x](/url)";
    let mut root = InlineState::new(&mut document, source, 0, source.len(), &md, ruleset, 0, 0);
    assert!(
        scan_link_candidate(&mut root, 0, false, None, InlineTargetFallback::Reference).is_some()
    );

    let mut nested = root
        .child_state(
            0..source.len(),
            root.depth.saturating_add(2),
            root.link_level,
        )
        .unwrap();
    assert_eq!(nested.depth, 2);
    assert!(
        scan_link_candidate(&mut nested, 0, false, None, InlineTargetFallback::Reference).is_none()
    );
}

#[test]
fn candidate_respects_window_edges_and_parent_cursor() {
    let mut document = crate::Document::new("", crate::Root::new(""));
    let md = parser::<false>();
    let ruleset = md.inline.rules();

    // The full source parses, but the window ends before the closing
    // parenthesis; bytes after `remaining()` must stay invisible.
    let source = "[x](/url)tail";
    let close = source.find(')').unwrap();
    let mut truncated = InlineState::new(&mut document, source, 0, close, &md, ruleset, 0, 0);
    assert_eq!(truncated.remaining(), "[x](/url");
    assert!(
        scan_link_candidate(
            &mut truncated,
            0,
            false,
            None,
            InlineTargetFallback::Reference
        )
        .is_none()
    );

    // Including the closing parenthesis makes the same window parse.
    let mut complete = InlineState::new(&mut document, source, 0, close + 1, &md, ruleset, 0, 0);
    let result = scan_link_candidate(
        &mut complete,
        0,
        false,
        None,
        InlineTargetFallback::Reference,
    )
    .unwrap();
    assert_eq!(result.end, close + 1);

    // After the parent cursor advances, results stay relative to
    // `remaining()`, not to the start of the session.
    let source = "前雪[x](/url)";
    let mut context = InlineState::new(
        &mut document,
        source,
        "前".len(),
        source.len(),
        &md,
        ruleset,
        0,
        0,
    );
    context.skip_token().unwrap();
    assert_eq!(context.remaining(), "[x](/url)");
    let result = scan_link_candidate(
        &mut context,
        0,
        false,
        None,
        InlineTargetFallback::Reference,
    )
    .unwrap();
    assert_eq!(
        (result.label_start, result.label_end, result.end),
        (1, 2, 9)
    );
    assert_eq!(result.href.as_deref(), Some("/url"));
    assert_eq!(context.remaining(), "[x](/url)");
}

#[test]
fn registered_link_check_does_not_require_running_the_factory() {
    let mut document = crate::Document::new("", crate::Root::new(""));
    let mut md = MarkdownIt::empty();
    add::<false>(&mut md, |_, _, _| {
        panic!("check must not call arena node factory")
    });
    let ruleset = md.inline.rules();
    let source = "[x](/url)";
    let mut context = InlineState::new(&mut document, source, 0, source.len(), &md, ruleset, 0, 0);
    let token = context.skip_token().unwrap();
    assert_eq!(token, source.len());
    assert_eq!(context.link_level, 0);
    assert!(context.skip_token().is_none());
}

#[test]
fn registered_prefix_check_does_not_require_running_the_factory() {
    let mut document = crate::Document::new("", crate::Root::new(""));
    let mut md = MarkdownIt::empty();
    add_prefix::<'雪', true>(&mut md, |_, _, _| {
        panic!("check must not call arena node factory")
    });
    let ruleset = md.inline.rules();
    let source = "雪[x](/url)";
    let mut context = InlineState::new(&mut document, source, 0, source.len(), &md, ruleset, 0, 0);
    let token = context.skip_token().unwrap();
    assert_eq!(token, source.len());
    assert_eq!(context.link_level, 0);
    assert!(context.skip_token().is_none());
}

#[test]
fn direct_prefix_uses_utf8_byte_offsets() {
    fn check<const PREFIX: char>() {
        let mut md = MarkdownIt::empty();
        crate::plugins::cmark::block::paragraph::add(&mut md);
        crate::plugins::cmark::inline::link::add(&mut md);
        add_prefix::<PREFIX, true>(&mut md, |document, href, title| {
            document.create_node(Link {
                url: href.unwrap_or_default(),
                title,
            })
        });
        // Expected outputs captured from the pre-migration legacy parser.
        let cases = [
            (
                format!("{PREFIX}[雪](/url)"),
                "<p><a href=\"/url\">雪</a></p>\n".to_owned(),
                1,
            ),
            (
                format!("[a {PREFIX}[雪](/in)](/out)"),
                "<p><a href=\"/out\">a <a href=\"/in\">雪</a></a></p>\n".to_owned(),
                2,
            ),
            (
                format!("{PREFIX}[雪](/unfinished"),
                format!("<p>{PREFIX}[雪](/unfinished</p>\n"),
                0,
            ),
        ];
        for (source, expected, link_count) in cases {
            let document = md.parse(&source);
            assert_eq!(md.render_html(&document), expected, "{source}");
            let links = document
                .events(document.root())
                .filter(|event| {
                    matches!(event, crate::StructuralEvent::Enter(_)) && event.node().is::<Link>()
                })
                .count();
            assert_eq!(links, link_count, "{source}");
            #[cfg(feature = "sourcemap")]
            if link_count > 0 {
                let start = source.find("[雪]").unwrap() + 1;
                let text_ranges: Vec<_> = document
                    .events(document.root())
                    .filter_map(|event| {
                        let node = event.node();
                        node.cast::<crate::document::Text>()
                            .filter(|text| text.content == "雪")
                            .map(|_| node.srcmap().unwrap().get_byte_offsets())
                    })
                    .collect();
                assert_eq!(text_ranges, [(start, start + "雪".len())], "{source}");
            }
        }
    }
    check::<'~'>();
    check::<'雪'>();
    check::<'🦀'>();
}
