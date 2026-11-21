use super::{FootnoteEnv, FootnoteReferenceEnds, FootnoteScans};
use crate as markdown_it;
use crate::MarkdownIt;
use crate::common::utils::normalize_reference;

#[test]
fn footnote_scan_cache_keeps_success_and_failure_and_context() {
    let source_len = "^[note]".len();
    let scans = FootnoteScans::new(source_len);
    let window = (2, source_len, 0, 0);
    assert_eq!(scans.memoized_end(window, 100, || Some(4)), Some(4));
    assert_eq!(
        scans.memoized_end(window, 100, || panic!("expected a cache hit")),
        Some(4)
    );
    let different_window = (2, source_len - 1, 0, 0);
    assert_eq!(scans.memoized_end(different_window, 100, || None), None);
    assert_eq!(
        scans.memoized_end(different_window, 100, || panic!("expected a cache hit")),
        None
    );
    for changed in [(2, source_len, 1, 0), (2, source_len, 0, 1)] {
        assert_eq!(scans.memoized_end(changed, 100, || Some(4)), Some(4));
    }
    assert_eq!(
        scans.0.lock().unwrap().budget.remaining_steps,
        source_len * FootnoteScans::STEPS_PER_BYTE - 4
    );
}

#[test]
fn malformed_footnote_scans_have_bounded_work() {
    for limit in [2, 100] {
        let mut md = MarkdownIt::new();
        markdown_it::plugins::extra::footnote::add(&mut md);
        md.max_nesting = limit;
        for n in [32, 128, 512] {
            let source = format!("{}x", "^[".repeat(n));
            let direct = md.parse(&source);
            assert_eq!(
                md.render_document(&direct, "html"),
                format!("<p>{source}</p>\n")
            );
        }
    }
}

#[test]
fn exhausted_footnote_scan_preserves_following_markdown() {
    let openers = "^[".repeat(128);
    let html = render(&format!("{openers}x **bold** [link](/url)"));
    assert_eq!(
        html,
        format!("<p>{openers}x <strong>bold</strong> <a href=\"/url\">link</a></p>\n")
    );
}

#[test]
fn nested_footnotes_preserve_the_parent_scan_budget() {
    let source = format!(
        "{}{}{} ^[tail] and **bold**",
        "^[".repeat(40),
        "*x* ".repeat(128),
        "]".repeat(40)
    );
    let html = render(&source);
    assert_eq!(html.matches("class=\"footnote-ref\"").count(), 41);
    assert_eq!(html.matches("<em>x</em>").count(), 128);
    assert!(html.contains("and <strong>bold</strong>"), "{html}");
    assert!(html.contains("<p>tail "), "{html}");
}

#[test]
fn overlapping_reference_labels_match_independent_normalization() {
    let mut env = FootnoteEnv::default();
    for label in ["a", "Straße", "ΟΣ", "Outer [^ x", "x", "[^", "Outer [^"] {
        env.define(normalize_reference(label));
    }
    for source in [
        "[^A] [^\u{feff}STRASSE  ] [^Outer [^\u{feff}x ]",
        "[^Outer [^ ΟΣ] [^Outer [^ Ος] [^Outer [^ ]",
        "[^x\n] [^Missing [^x] [^x",
        "[^[^] [^] [^ \t ]",
    ] {
        let actual = FootnoteReferenceEnds::new(source, &env);
        let expected = source
            .match_indices("[^")
            .filter_map(|(start, _)| {
                let tail = &source[start + 2..];
                let end = tail.find(['\n', ']'])?;
                (tail.as_bytes()[end] == b']'
                    && env.named.contains_key(&normalize_reference(&tail[..end])))
                .then_some((start, start + 3 + end))
            })
            .collect();
        assert_eq!(actual.0, expected, "{source:?}");
    }
}

#[test]
fn malformed_footnote_scan_budget_and_depth_are_bounded() {
    let source_len = 8;
    let scans = FootnoteScans::new(source_len);
    let window = (0, source_len, 0, 0);
    assert_eq!(
        scans.memoized_end(window, 0, || panic!("depth limit should reject the scan")),
        None
    );
    scans.memoized_end(window, 100, || {
        let mut steps = 2; // The rejected scan and this cache miss each spent a step.
        while scans.step().is_some() {
            steps += 1;
        }
        assert_eq!(steps, source_len * FootnoteScans::STEPS_PER_BYTE);
        assert_eq!(scans.0.lock().unwrap().budget.remaining_steps, 0);
        None
    });
    let state = scans.0.lock().unwrap();
    assert_eq!((state.scan_depth, state.content_depth), (0, 0));
}

#[test]
fn deep_closed_footnotes_respect_the_content_depth_limit() {
    let scans = FootnoteScans::new(1);
    let first = scans.enter_content(1, 2).unwrap();
    let second = scans.enter_content(1, 2).unwrap();
    assert!(scans.enter_content(1, 2).is_none());
    drop(second);
    assert!(scans.enter_content(1, 2).is_some());
    drop(first);
    for limit in [2, 100, 10_000] {
        let mut md = MarkdownIt::new();
        markdown_it::plugins::extra::footnote::add(&mut md);
        md.max_nesting = limit;
        let source = format!("{}x{}", "^[".repeat(256), "]".repeat(256));
        let direct = md.parse(&source);
        assert!(md.render_document(&direct, "html").contains('x'));
    }
}

#[test]
fn identical_paragraphs_do_not_share_an_exhausted_scan_budget() {
    let source = "text^[note]\n\n".repeat(100);
    let html = render(&source);
    assert_eq!(html.matches("class=\"footnote-ref\"").count(), 100);
    assert!(html.contains("id=\"fn100\""));
}

#[test]
fn duplicate_definition_keeps_first_body_and_collects_nested_footnotes() {
    let html = render("Text[^a].\n\n[^a]: first body\n\n[^a]: duplicate body^[nested note]");
    for (id, body) in [("fn1", "first body"), ("fn2", "nested note")] {
        let item = format!("<li id=\"{id}\" class=\"footnote-item\">\n<p>{body}");
        assert!(html.contains(&item), "{html}");
    }
    assert!(!html.contains("duplicate body"), "{html}");
}

#[test]
fn earlier_definition_wins_over_later_definition_in_nested_block() {
    let html = render("Text[^a].\n\n[^a]: first body\n\n> [^a]: duplicate body");
    assert!(html.contains("first body"), "{html}");
    assert!(!html.contains("duplicate body"), "{html}");
}

fn render(input: &str) -> String {
    let mut md = markdown_it::MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::extra::footnote::add(&mut md);

    md.render(input)
}

#[test]
fn basic_footnote() {
    let html = render("Here is a footnote.[^a]\n\n[^a]: Footnote text.");

    assert!(html.contains(r#"<sup class="footnote-ref">"#));
    assert!(html.contains(r##"href="#fn1""##));
    assert!(html.contains(r#"id="fn1""#));
    assert!(html.contains("Footnote text."));
    assert!(!html.contains("[^a]:"));
}

#[test]
fn backreference_text_and_aria_label_are_independently_configurable() {
    use super::{FootnoteOptions, add_with_options};

    let source = "First[^a] and second[^a].\n\n[^a]: Note";
    for (text, label, expected) in [
        (
            Some("返回 <正文> &"),
            None,
            r#" class="footnote-backref">返回 &lt;正文&gt; &amp;</a>"#,
        ),
        (
            None,
            Some("返回 \"正文\""),
            r#" class="footnote-backref" aria-label="返回 &quot;正文&quot;">&#8617;</a>"#,
        ),
    ] {
        let mut md = MarkdownIt::empty();
        markdown_it::plugins::cmark::add(&mut md);
        add_with_options(
            &mut md,
            FootnoteOptions {
                backref_text: text.map(str::to_owned),
                backref_aria_label: label.map(str::to_owned),
            },
        );
        let document = md.parse(source);
        let html = md.render_document(&document, "html");
        for id in ["fnref1", "fnref1:2"] {
            assert!(
                html.contains(&format!("<a href=\"#{id}\"{expected}")),
                "{html}"
            );
        }
        assert_eq!(
            md.render_document(&document, "text"),
            "First and second.\nNote\n"
        );
    }
}

#[test]
fn definition_can_appear_before_reference() {
    let html = render("[^a]: Footnote text.\n\nHere is a footnote.[^a]");

    assert!(html.contains(r##"href="#fn1""##));
    assert!(html.contains("Footnote text."));
}

#[test]
fn undefined_reference_stays_text() {
    let html = render("Here is missing[^x].");

    assert_eq!(html, "<p>Here is missing[^x].</p>\n");
}

#[test]
fn link_reference_still_works() {
    let html = render("[link]: /url\n\n[link]");

    assert_eq!(html, r#"<p><a href="/url">link</a></p>"#.to_owned() + "\n");
}

#[test]
fn footnote_definition_is_not_link_reference() {
    let html = render("Text[^a]\n\n[^a]: Footnote text.");

    assert!(html.contains("Footnote text."));
    assert!(!html.contains(r#"href="Footnote""#));
}

#[test]
fn numbers_follow_reference_order() {
    let html = render("A[^b] B[^a]\n\n[^a]: first\n[^b]: second");

    let ref_b = html.find(r##"href="#fn1""##).unwrap();
    let ref_a = html.find(r##"href="#fn2""##).unwrap();
    let second_defined = html.find("second").unwrap();
    let first_defined = html.find("first").unwrap();

    assert!(ref_b < ref_a);
    assert!(second_defined < first_defined);
}

#[test]
fn repeated_reference_creates_one_footnote_item() {
    let html = render("A[^a] B[^a]\n\n[^a]: Footnote text.");

    assert_eq!(html.matches(r#"id="fn1""#).count(), 1);
    assert_eq!(html.matches(r##"href="#fn1""##).count(), 2);
    assert!(html.contains("Footnote text."));
}

#[test]
fn footnote_definition_allows_indented_continuation_lines() {
    let html = render("Text[^a]\n\n[^a]: first line\n    second line\n\nAfter.");

    assert!(html.contains("first line\nsecond line"));
    assert!(html.contains("<p>After.</p>"));

    let paragraph = html.find("<p>After.</p>").unwrap();
    let footnotes = html.find(r#"<section class="footnotes">"#).unwrap();
    assert!(paragraph < footnotes);
}

#[test]
fn footnote_definition_allows_tab_indented_continuation_lines() {
    let html = render("Text[^a]\n\n[^a]: first line\n\tsecond line\n\nAfter.");

    assert!(html.contains("first line\nsecond line"));
    assert!(html.contains("<p>After.</p>"));

    let paragraph = html.find("<p>After.</p>").unwrap();
    let footnotes = html.find(r#"<section class="footnotes">"#).unwrap();
    assert!(paragraph < footnotes);
}

#[test]
fn footnote_definition_allows_tab_indented_multiple_paragraphs() {
    let html = render("Text[^a]\n\n[^a]: first paragraph\n\n\tsecond paragraph");

    assert!(html.contains("<p>first paragraph</p>"));
    assert!(html.contains(
        r##"<p>second paragraph <a href="#fnref1" class="footnote-backref">&#8617;</a></p>"##
    ));
    assert!(!html.contains("[^a]:"));
}

#[test]
fn footnote_definition_allows_multiple_paragraphs() {
    let html = render("Text[^a]\n\n[^a]: first paragraph\n\n    second paragraph");

    assert!(html.contains("<p>first paragraph</p>"));
    assert!(html.contains(
        r##"<p>second paragraph <a href="#fnref1" class="footnote-backref">&#8617;</a></p>"##
    ));
    assert!(!html.contains("[^a]:"));
}

#[test]
fn inline_footnote_creates_footnote_item() {
    let html = render("Text^[inline footnote].");

    assert!(html.contains(r##"href="#fn1""##));
    assert!(html.contains(r#"id="fn1""#));
    assert!(html.contains(
        r##"<p>inline footnote <a href="#fnref1" class="footnote-backref">&#8617;</a></p>"##
    ));
    assert!(!html.contains("^[inline footnote]"));
}

#[test]
fn inline_footnote_parses_inline_markdown() {
    let html = render("Text^[inline **strong** note].");

    assert!(html.contains(r##"<p>inline <strong>strong</strong> note <a href="#fnref1" class="footnote-backref">&#8617;</a></p>"##));
}

#[test]
fn inline_and_reference_footnotes_share_reference_order() {
    let html = render("A[^a] B^[inline]\n\n[^a]: named");

    let named_ref = html.find(r##"href="#fn1""##).unwrap();
    let inline_ref = html.find(r##"href="#fn2""##).unwrap();
    let named_definition = html.find("<p>named ").unwrap();
    let inline_definition = html.find("<p>inline ").unwrap();

    assert!(named_ref < inline_ref);
    assert!(named_definition < inline_definition);
}

#[test]
fn inline_footnote_before_reference_gets_first_number() {
    let html = render("A^[inline] B[^a]\n\n[^a]: named");

    let inline_ref = html.find(r##"href="#fn1""##).unwrap();
    let named_ref = html.find(r##"href="#fn2""##).unwrap();
    let inline_definition = html.find("<p>inline ").unwrap();
    let named_definition = html.find("<p>named ").unwrap();

    assert!(inline_ref < named_ref);
    assert!(inline_definition < named_definition);
}

#[test]
fn inline_footnote_allows_nested_brackets() {
    let html = render("Text^[literal [nested] brackets].");

    assert!(html.contains(r##"<p>literal [nested] brackets <a href="#fnref1" class="footnote-backref">&#8617;</a></p>"##));
    assert!(!html.contains("^[literal"));
}

#[test]
fn inline_footnote_allows_escaped_closing_bracket() {
    let html = render(r"Text^[escaped \] bracket].");

    assert!(html.contains(
        r##"<p>escaped ] bracket <a href="#fnref1" class="footnote-backref">&#8617;</a></p>"##
    ));
    assert!(!html.contains(r"^[escaped \] bracket]"));
}

#[test]
fn inline_footnote_allows_links_in_content() {
    let html = render("Text^[see [Rust](https://www.rust-lang.org/)].");

    assert!(html.contains(r##"<p>see <a href="https://www.rust-lang.org/">Rust</a> <a href="#fnref1" class="footnote-backref">&#8617;</a></p>"##));
}

#[test]
fn footnote_definition_can_contain_inline_footnote() {
    let html = render("Text[^outer]\n\n[^outer]: outer^[inner]");

    assert!(html.contains(r##"href="#fn1""##));
    assert!(html.contains(r##"href="#fn2""##));

    let outer_definition = html.find("<p>outer").unwrap();
    let inner_definition = html.find("<p>inner ").unwrap();
    assert!(outer_definition < inner_definition);
}

#[test]
fn single_paragraph_backref_renders_inside_paragraph() {
    let html = render("Text[^a]\n\n[^a]: see [Rust](https://www.rust-lang.org/)");

    assert!(html.contains(r##"<p>see <a href="https://www.rust-lang.org/">Rust</a> <a href="#fnref1" class="footnote-backref">&#8617;</a></p>"##));
    assert!(!html.contains(
        r##"</p>
 <a href="#fnref1" class="footnote-backref">"##
    ));
}

#[test]
fn empty_inline_footnote_stays_text() {
    let html = render("Text^[]");

    assert_eq!(html, "<p>Text^[]</p>\n");
}

#[test]
fn unclosed_inline_footnote_stays_text() {
    let html = render("Text^[missing");

    assert_eq!(html, "<p>Text^[missing</p>\n");
}

#[test]
fn escaped_inline_footnote_marker_stays_text() {
    let html = render(r"Text\^[not footnote]");

    assert_eq!(html, "<p>Text^[not footnote]</p>\n");
}
