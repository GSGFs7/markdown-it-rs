fn render(input: &str) -> String {
    let mut md = markdown_it::MarkdownIt::new();
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::extra::footnote::add(&mut md);

    md.parse(input).render()
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
fn definition_can_appear_before_reference() {
    let html = render("[^a]: Footnote text.\n\nHere is a footnote.[^a]");

    assert!(html.contains(r##"href="#fn1""##));
    assert!(html.contains("Footnote text."));
}

#[test]
fn undefined_reference_stays_text() {
    let html = render("Here is missing[^x].");

    assert_eq!(html, "<p>Here is missing[^x].</p>\n")
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
    assert!(!html.contains(r#"href="Footnote""#))
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
    assert!(html.contains("<p>second paragraph</p>"));
    assert!(!html.contains("[^a]:"));
}

#[test]
fn footnote_definition_allows_multiple_paragraphs() {
    let html = render("Text[^a]\n\n[^a]: first paragraph\n\n    second paragraph");

    assert!(html.contains("<p>first paragraph</p>"));
    assert!(html.contains("<p>second paragraph</p>"));
    assert!(!html.contains("[^a]:"));
}

#[test]
fn inline_footnote_creates_footnote_item() {
    let html = render("Text^[inline footnote].");

    assert!(html.contains(r##"href="#fn1""##));
    assert!(html.contains(r#"id="fn1""#));
    assert!(html.contains("<p>inline footnote</p>"));
    assert!(!html.contains("^[inline footnote]"));
}

#[test]
fn inline_footnote_parses_inline_markdown() {
    let html = render("Text^[inline **strong** note].");

    assert!(html.contains("<p>inline <strong>strong</strong> note</p>"));
}

#[test]
fn inline_and_reference_footnotes_share_reference_order() {
    let html = render("A[^a] B^[inline]\n\n[^a]: named");

    let named_ref = html.find(r##"href="#fn1""##).unwrap();
    let inline_ref = html.find(r##"href="#fn2""##).unwrap();
    let named_definition = html.find("<p>named</p>").unwrap();
    let inline_definition = html.find("<p>inline</p>").unwrap();

    assert!(named_ref < inline_ref);
    assert!(named_definition < inline_definition);
}

#[test]
fn inline_footnote_before_reference_gets_first_number() {
    let html = render("A^[inline] B[^a]\n\n[^a]: named");

    let inline_ref = html.find(r##"href="#fn1""##).unwrap();
    let named_ref = html.find(r##"href="#fn2""##).unwrap();
    let inline_definition = html.find("<p>inline</p>").unwrap();
    let named_definition = html.find("<p>named</p>").unwrap();

    assert!(inline_ref < named_ref);
    assert!(inline_definition < named_definition);
}

#[test]
fn inline_footnote_allows_nested_brackets() {
    let html = render("Text^[literal [nested] brackets].");

    assert!(html.contains("<p>literal [nested] brackets</p>"));
    assert!(!html.contains("^[literal"));
}

#[test]
fn inline_footnote_allows_escaped_closing_bracket() {
    let html = render(r"Text^[escaped \] bracket].");

    assert!(html.contains("<p>escaped ] bracket</p>"));
    assert!(!html.contains(r"^[escaped \] bracket]"));
}

#[test]
fn inline_footnote_allows_links_in_content() {
    let html = render("Text^[see [Rust](https://www.rust-lang.org/)].");

    assert!(html.contains(r#"<p>see <a href="https://www.rust-lang.org/">Rust</a></p>"#));
}

#[test]
fn footnote_definition_can_contain_inline_footnote() {
    let html = render("Text[^outer]\n\n[^outer]: outer^[inner]");

    assert!(html.contains(r##"href="#fn1""##));
    assert!(html.contains(r##"href="#fn2""##));

    let outer_definition = html.find("<p>outer").unwrap();
    let inner_definition = html.find("<p>inner</p>").unwrap();
    assert!(outer_definition < inner_definition);
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
