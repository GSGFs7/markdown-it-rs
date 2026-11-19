use std::ops::Range;

use markdown_it::MarkdownIt;
use markdown_it::parser::inline::helpers::paired_span::{EscapePolicy, NestingPolicy, PairOptions};
use markdown_it::plugin::{InlineContainerPart, PluginSpec};

#[derive(Debug)]
struct Annotation;

fn annotation() -> PluginSpec {
    let label = PairOptions::new("[", "]")
        .escape(EscapePolicy::Backslash)
        .nesting(NestingPolicy::SamePair);
    let note = PairOptions::new("{", "}")
        .escape(EscapePolicy::Backslash)
        .nesting(NestingPolicy::SamePair);
    PluginSpec::new("annotation")
        .inline_container::<Annotation>("annotation")
        .marker('[')
        .parse(move |cx| {
            cx.consume_part(&label)?;
            cx.consume_part(&note)?;
            Some(Annotation)
        })
        .render_html(|_, html| {
            let label = html.render_part(0);
            let note = html.render_part(1);
            assert_eq!(html.render_part(2), "");
            html.element("ruby").raw(label).element("rt").raw(note);
        })
        .render_text(|_, text| {
            text.render_part(0);
            text.write_str("(");
            text.render_part(1);
            text.write_str(")");
            text.render_part(2);
        })
        .finish()
}

#[test]
fn composed_pairs_parse_markdown_in_both_parts() {
    let md = MarkdownIt::builder().plugin(annotation()).build();
    let doc = md.parse("前[**文字**]{*说明* & <}尾");
    assert_eq!(
        md.render_document(&doc, "html"),
        "<p>前<ruby><strong>文字</strong><rt><em>说明</em> &amp; &lt;</rt></ruby>尾</p>\n"
    );
    assert_eq!(md.render_document(&doc, "text"), "前文字(说明 & <)尾\n");
    let paragraph = doc.children(doc.root())[0];
    let node = doc.children(paragraph)[1];
    assert!(doc.value::<Annotation>(node).is_some());
    let parts = doc.children(node);
    assert_eq!(parts.len(), 2);
    for &part in parts {
        assert!(doc.value::<InlineContainerPart>(part).is_some());
        assert_eq!(doc.parent(part), Some(node));
        for &child in doc.children(part) {
            assert_eq!(doc.parent(child), Some(part));
        }
    }
    #[cfg(feature = "sourcemap")]
    {
        let source = doc.source();
        for (&part, body) in parts.iter().zip(["**文字**", "*说明* & <"]) {
            let (start, end) = doc.node(part).srcmap().unwrap().get_byte_offsets();
            assert_eq!(&source[start..end], body);
        }
    }
}

fn single_part_plugin() -> PluginSpec {
    PluginSpec::new("single-part")
        .inline_container::<Annotation>("single-part")
        .marker('{')
        .parse(|cx| {
            let source = cx.source();
            let end = source.find('}')?;
            cx.part(1..end);
            cx.advance_bytes(end + 1);
            Some(Annotation)
        })
        .render_html(|_, html| {
            let part = html.render_part(0);
            let missing = html.render_part(1);
            html.element("mark").raw(part).raw(missing);
        })
        .finish()
}

#[test]
fn single_range_containers_also_expose_one_part() {
    let md = MarkdownIt::builder().plugin(single_part_plugin()).build();
    for (source, expected) in [
        ("{*x*}", "<p><mark><em>x</em></mark></p>\n"),
        ("{}", "<p><mark></mark></p>\n"),
    ] {
        let doc = md.parse(source);
        assert_eq!(md.render_document(&doc, "html"), expected);
        let paragraph = doc.children(doc.root())[0];
        let node = doc.children(paragraph)[0];
        let parts = doc.children(node);
        assert_eq!(parts.len(), 1);
        assert!(doc.value::<InlineContainerPart>(parts[0]).is_some());
    }
}

#[test]
fn empty_parts_render_in_formats_without_a_part_renderer() {
    let plugin = PluginSpec::new("html-only")
        .inline_container::<Annotation>("html-only")
        .marker('{')
        .parse(|cx| {
            let source = cx.source();
            let end = source.find('}')?;
            cx.part(1..end);
            cx.part(1..1);
            cx.advance_bytes(end + 1);
            Some(Annotation)
        })
        .html_tag("span")
        .finish();
    let md = MarkdownIt::builder().plugin(plugin).build();
    let doc = md.parse("{x}");
    assert_eq!(md.render_document(&doc, "html"), "<p><span>x</span></p>\n");
    assert_eq!(md.render_document(&doc, "text"), "x\n");
}

#[test]
fn empty_parts_keep_their_indices() {
    let md = MarkdownIt::builder().plugin(annotation()).build();
    for (source, html, text) in [
        ("[]{说明}", "<ruby><rt>说明</rt></ruby>", "(说明)"),
        ("[文字]{}", "<ruby>文字<rt></rt></ruby>", "文字()"),
        ("[]{}", "<ruby><rt></rt></ruby>", "()"),
    ] {
        let doc = md.parse(source);
        assert_eq!(md.render_document(&doc, "html"), format!("<p>{html}</p>\n"));
        assert_eq!(md.render_document(&doc, "text"), format!("{text}\n"));
    }
}

#[test]
fn failed_suffix_rolls_back_and_keeps_normal_links() {
    let md = MarkdownIt::builder().plugin(annotation()).build();
    assert_eq!(md.render("[文字]{未结束"), "<p>[文字]{未结束</p>\n");
    assert_eq!(
        md.render("[文字](/url)"),
        "<p><a href=\"/url\">文字</a></p>\n"
    );
    assert_eq!(
        md.render("[文字]{[链接](/url)}"),
        "<p><ruby>文字<rt><a href=\"/url\">链接</a></rt></ruby></p>\n"
    );
}

#[test]
fn nesting_escapes_and_child_sessions_are_preserved() {
    let md = MarkdownIt::builder().plugin(annotation()).build();
    assert_eq!(
        md.render("[外[内]{注}]{说明}"),
        "<p><ruby>外<ruby>内<rt>注</rt></ruby><rt>说明</rt></ruby></p>\n"
    );
    assert_eq!(
        md.render(r"[文\]字]{说\}明}"),
        "<p><ruby>文]字<rt>说}明</rt></ruby></p>\n"
    );
    assert_eq!(
        md.render("[*文字]{说明*}"),
        "<p><ruby>*文字<rt>说明*</rt></ruby></p>\n"
    );
    let mut limited = MarkdownIt::builder().plugin(annotation()).build();
    limited.max_nesting = 1;
    assert_eq!(
        limited.render("[*文字*]{*说明*}"),
        "<p><ruby>*文字*<rt>*说明*</rt></ruby></p>\n"
    );
}

fn parts_plugin(ranges: Vec<Range<usize>>, consumed: usize) -> PluginSpec {
    PluginSpec::new("parts")
        .inline_container::<Annotation>("parts")
        // Renderers may be registered before the parser.
        .html_tag("span")
        .render_text(|_, text| text.render_children())
        .parse(move |cx| {
            if !cx.advance_bytes(consumed) {
                return None;
            }
            for range in &ranges {
                cx.part_at(range.clone());
            }
            Some(Annotation)
        })
        .finish()
}

#[test]
fn render_children_transparently_renders_all_parts_in_returned_order() {
    let md = MarkdownIt::builder()
        .plugin(parts_plugin(vec![3..4, 2..2, 1..2], 5))
        .build();
    let doc = md.parse("[a|b]");
    assert_eq!(md.render_document(&doc, "html"), "<p><span>ba</span></p>\n");
    assert_eq!(md.render_document(&doc, "text"), "ba\n");
    let md = MarkdownIt::builder()
        .plugin(parts_plugin(vec![], 1))
        .build();
    let doc = md.parse("x");
    assert_eq!(md.render_document(&doc, "html"), "<p><span></span></p>\n");
    let paragraph = doc.children(doc.root())[0];
    let node = doc.children(paragraph)[0];
    assert!(doc.children(node).is_empty());
}

#[test]
#[allow(clippy::reversed_empty_ranges, clippy::single_range_in_vec_init)] // the invalid ranges are the test input
fn invalid_parts_are_rejected() {
    for ranges in [vec![1..2], vec![0..99], vec![2..1], vec![0..3, 4..5]] {
        let md = MarkdownIt::builder()
            .plugin(parts_plugin(ranges, 3))
            .build();
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| md.parse("雪tail"))).is_err()
        );
    }
    let md = MarkdownIt::builder()
        .plugin(parts_plugin(vec![], 0))
        .build();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| md.parse("x"))).is_err());
}

#[test]
fn paired_containers_skip_code_spans_in_both_parts() {
    let md = MarkdownIt::builder().plugin(annotation()).build();
    for (source, body) in [
        (
            "[包含 `]` 的文字]{说明}",
            "包含 <code>]</code> 的文字<rt>说明</rt>",
        ),
        (
            "[包含 ``a` ] b`` 的文字]{说明}",
            "包含 <code>a` ] b</code> 的文字<rt>说明</rt>",
        ),
        (
            "[文字]{包含 `}` 的说明}",
            "文字<rt>包含 <code>}</code> 的说明</rt>",
        ),
        ("[文字 ` 未闭合]{说明}", "文字 ` 未闭合<rt>说明</rt>"),
        ("[文字 `[`]{说明}", "文字 <code>[</code><rt>说明</rt>"),
    ] {
        let doc = md.parse(source);
        assert_eq!(
            md.render_document(&doc, "html"),
            format!("<p><ruby>{body}</ruby></p>\n"),
            "{source}"
        );
        #[cfg(feature = "sourcemap")]
        {
            let paragraph = doc.children(doc.root())[0];
            let annotation = doc.children(paragraph)[0];
            let label = doc.children(annotation)[0];
            let (start, end) = doc.node(label).srcmap().unwrap().get_byte_offsets();
            assert_eq!(&source[start..end], &source[1..source.rfind("]{").unwrap()]);
        }
    }
    assert_eq!(
        md.render_document(&md.parse("[包含 `]` 的文字]{说明}"), "text"),
        "包含 ] 的文字(说明)\n"
    );
    assert_eq!(
        md.render("[[包含 `]` 的文字]{说明}](/url)"),
        "<p><a href=\"/url\"><ruby>包含 <code>]</code> 的文字<rt>说明</rt></ruby></a></p>\n"
    );
}

#[test]
fn unclosed_nested_openers_do_not_reenter_container_scanning() {
    let md = MarkdownIt::builder().plugin(annotation()).build();
    // Every '[' starts a failed pair scan. Probing inline rules here must not
    // re-enter this container, which previously branched exponentially.
    let source = "[".repeat(1_000);
    assert_eq!(md.render(&source), format!("<p>{source}</p>\n"));
}

fn configured_pair(options: PairOptions<'static>) -> PluginSpec {
    PluginSpec::new("configured-pair")
        .inline_container::<Annotation>("configured-pair")
        .marker('@')
        .parse(move |cx| {
            if !cx.consume("@") {
                return None;
            }
            let before = cx.consumed_bytes();
            let matched = cx.scan_pair(&options)?;
            assert_eq!(cx.scan_pair(&options), Some(matched.clone()));
            assert_eq!(cx.consumed_bytes(), before);
            let consumed = cx.consume_pair(&options)?;
            assert_eq!(matched, consumed);
            cx.part_at(before + matched.body.start..before + matched.body.end);
            Some(Annotation)
        })
        .html_tag("mark")
        .finish()
}

#[test]
fn contextual_pair_scan_keeps_offsets_and_stops_text_at_custom_markers() {
    let md = MarkdownIt::builder()
        .plugin(configured_pair(PairOptions::new("«", "雪*")))
        .build();
    assert_eq!(
        md.render("前 @«文字 `雪*` 文字雪* 尾"),
        "<p>前 <mark>文字 <code>雪*</code> 文字</mark> 尾</p>\n"
    );
    // Without commonmark's text/rule markers the closing string is still found.
    let md = markdown_it::plugin::MarkdownItBuilder::empty()
        .plugin(configured_pair(PairOptions::new("«", "»")))
        .build();
    assert_eq!(md.render("@«文字»尾"), "<mark>文字</mark>尾\n");
}

#[test]
fn contextual_pair_scan_keeps_escape_empty_and_line_break_policies() {
    for (options, source, expected) in [
        (
            PairOptions::new("[", "]"),
            r"@[a\]b]",
            "<p><mark>a\\</mark>b]</p>\n",
        ),
        (
            PairOptions::new("[", "]").escape(EscapePolicy::Backslash),
            r"@[a\]b]",
            "<p><mark>a]b</mark></p>\n",
        ),
        (
            PairOptions::new("[", "]").allow_empty(false),
            "@[]",
            "<p>@[]</p>\n",
        ),
        (
            PairOptions::new("[", "]"),
            "@[a`x\ny`b]",
            "<p>@[a<code>x y</code>b]</p>\n",
        ),
        (
            PairOptions::new("[", "]").allow_line_breaks(true),
            "@[a`x\ny`b]",
            "<p><mark>a<code>x y</code>b</mark></p>\n",
        ),
    ] {
        let md = MarkdownIt::builder()
            .plugin(configured_pair(options))
            .build();
        assert_eq!(md.render(source), expected, "{source}");
    }
}

#[test]
fn contextual_pairs_respect_markdown_escapes_before_code_spans() {
    for escape in [EscapePolicy::None, EscapePolicy::Backslash] {
        let md = MarkdownIt::builder()
            .plugin(configured_pair(PairOptions::new("«", "»").escape(escape)))
            .build();
        assert_eq!(
            md.render(r"@«a \` » x ` b»"),
            "<p><mark>a ` </mark> x ` b»</p>\n",
            "{escape:?}"
        );
        assert_eq!(
            md.render(r"@«a \\`»` b»"),
            "<p><mark>a \\<code>»</code> b</mark></p>\n",
            "{escape:?}"
        );
        // The escaped backtick ends an escape token; the next backtick can
        // begin a code span even though it follows the same character.
        assert_eq!(
            md.render(r"@«a \``»` b»"),
            "<p><mark>a `<code>»</code> b</mark></p>\n",
            "{escape:?}"
        );
    }
}

#[test]
fn failed_contextual_pair_scan_does_not_poison_link_boundaries() {
    #[derive(Debug)]
    struct Inner;

    let inner = || {
        PluginSpec::new("inner")
            .inline_container::<Inner>("inner")
            .marker('{')
            .parse(|cx| {
                cx.consume("{]}").then_some(())?;
                cx.part_at(1..2);
                Some(Inner)
            })
            .html_tag("span")
            .finish()
    };
    let source = "@«[a {]}](url)";
    let expected = "<p>@«<a href=\"url\">a <span>]</span></a></p>\n";
    assert_eq!(
        MarkdownIt::builder().plugin(inner()).build().render(source),
        expected
    );
    let md = MarkdownIt::builder()
        .plugin(inner())
        .plugin(configured_pair(PairOptions::new("«", "»")))
        .build();
    assert_eq!(md.render(source), expected);
}

#[test]
fn unclosed_pair_scans_do_not_repeatedly_probe_the_same_suffix() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct Probe;

    for count in [64, 256, 1_024] {
        for suffix in ["", "]", "\nend", "\n]"] {
            let checks = Arc::new(AtomicUsize::new(0));
            let probe_checks = Arc::clone(&checks);
            let probe = PluginSpec::new("probe")
                .inline_leaf::<Probe>("probe")
                .marker('^')
                .parse(move |_| {
                    probe_checks.fetch_add(1, Ordering::Relaxed);
                    None
                })
                .render_html(|_, _| {})
                .finish();
            let md = MarkdownIt::builder()
                .plugin(probe)
                .plugin(configured_pair(
                    PairOptions::new("[", "]").nesting(NestingPolicy::SamePair),
                ))
                .build();
            let source = format!("{}{suffix}", "@[x^".repeat(count));
            let doc = md.parse(&source);
            // Count rule checks rather than elapsed time: a scan of every
            // suffix calls this rule quadratically. Include normal parsing.
            assert!(
                checks.load(Ordering::Relaxed) <= 8 * count,
                "{count} openers, suffix {suffix:?}: {} checks",
                checks.load(Ordering::Relaxed)
            );
            let output = md.render_document(&doc, "html");
            if suffix != "]" {
                assert_eq!(output, format!("<p>{source}</p>\n"));
            } else {
                // A failed outer pair must not discard the valid inner pair.
                assert_eq!(
                    output,
                    format!("<p>{}<mark>x^</mark></p>\n", "@[x^".repeat(count - 1))
                );
            }
        }
    }
}

#[test]
fn opaque_and_escaped_closers_do_not_cause_repeated_suffix_scans() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct Probe;

    for (escape, unit, rendered) in [
        (EscapePolicy::None, "@«x^`»` ", "@«x^<code>»</code> "),
        (EscapePolicy::Backslash, r"@«x^\» ", r"@«x^\» "),
    ] {
        let checks = Arc::new(AtomicUsize::new(0));
        let probe_checks = Arc::clone(&checks);
        let probe = PluginSpec::new("probe")
            .inline_leaf::<Probe>("probe")
            .marker('^')
            .parse(move |_| {
                probe_checks.fetch_add(1, Ordering::Relaxed);
                None
            })
            .render_html(|_, _| {})
            .finish();
        let md = MarkdownIt::builder()
            .plugin(probe)
            .plugin(configured_pair(PairOptions::new("«", "»").escape(escape)))
            .build();

        for count in [64, 256, 1_024] {
            checks.store(0, Ordering::Relaxed);
            let source = unit.repeat(count);
            let html = md.render(&source);
            let checked = checks.load(Ordering::Relaxed);
            assert!(
                checked <= 8 * count,
                "{unit:?}, {count} openers: {checked} checks"
            );
            assert_eq!(
                html,
                format!("<p>{}</p>\n", rendered.repeat(count).trim_end())
            );
        }
    }
}

#[test]
fn matched_links_win_over_nesting_when_the_marker_is_shared() {
    let md = MarkdownIt::builder().plugin(annotation()).build();
    // The container opener `[` is also the link opener. The link span is
    // opaque, so the `]` inside its destination cannot close the label.
    assert_eq!(
        md.render("[[x](/a]b)]{note}"),
        "<p><ruby><a href=\"/a%5Db\">x</a><rt>note</rt></ruby></p>\n"
    );
}
