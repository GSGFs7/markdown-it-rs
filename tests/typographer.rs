fn run_with(input: &str, output: &str, configure: impl Fn(&mut markdown_it::MarkdownIt)) {
    let output = if output.is_empty() {
        "".to_owned()
    } else {
        output.to_owned() + "\n"
    };
    let md = &mut markdown_it::MarkdownIt::empty();
    markdown_it::plugins::cmark::add(md);
    markdown_it::plugins::html::add(md);
    configure(md);
    markdown_it::plugins::extra::typographer::add(md);
    let node = md.parse(&(input.to_owned() + "\n"));

    // make sure we have sourcemaps for everything
    #[cfg(feature = "sourcemap")]
    for event in node.events(node.root()) {
        assert!(event.node().srcmap().is_some());
    }

    let result = md.render_html(&node);
    assert_eq!(result, output);

    let document_md = &mut markdown_it::MarkdownIt::empty();
    markdown_it::plugins::cmark::add(document_md);
    markdown_it::plugins::html::add(document_md);
    configure(document_md);
    markdown_it::plugins::extra::typographer::add(document_md);
    let document = document_md.parse(&(input.to_owned() + "\n"));
    assert_eq!(document_md.render_html(&document), output);

    // make sure it doesn't crash without trailing \n
    let _ = md.parse(input.trim_end());
}

fn run(input: &str, output: &str) {
    run_with(input, output, |_| {});
}

#[test]
fn typographer_dash_boundaries_follow_javascript_whitespace() {
    for (source, expected) in [
        ("a--\u{feff}b", "<p>a--\u{feff}b</p>"),
        ("a\u{feff}--b", "<p>a\u{feff}--b</p>"),
        ("a\u{feff}--\u{feff}b", "<p>a\u{feff}–\u{feff}b</p>"),
        ("a--\u{85}b", "<p>a–\u{85}b</p>"),
        ("a\u{85}--b", "<p>a\u{85}–b</p>"),
        ("a\u{85}--\u{85}b", "<p>a\u{85}–\u{85}b</p>"),
        ("a --\u{85}b", "<p>a --\u{85}b</p>"),
        ("a\u{85}-- b", "<p>a\u{85}-- b</p>"),
    ] {
        run(source, expected);
    }
}

#[test]
fn typographer_preserves_code_spans_and_image_alt_text() {
    for (source, expected) in [
        (
            "before... `<strong>...</strong>` after...",
            "<p>before… <code>&lt;strong&gt;...&lt;/strong&gt;</code> after…</p>",
        ),
        (
            "`(c) (r) (tm) +- -- --- ???? !!!! ,,,,` (c) (r) (tm)",
            "<p><code>(c) (r) (tm) +- -- --- ???? !!!! ,,,,</code> © ® ™</p>",
        ),
        (
            "``a ` ...`` and `b...` ...",
            "<p><code>a ` ...</code> and <code>b...</code> …</p>",
        ),
        (
            "[*label...* `code...`](/url)",
            "<p><a href=\"/url\"><em>label…</em> <code>code...</code></a></p>",
        ),
        (
            "![text... `code...`](/img) after...",
            "<p><img src=\"/img\" alt=\"text... code...\"> after…</p>",
        ),
    ] {
        run(source, expected);
    }
}

#[cfg(feature = "linkify")]
#[test]
fn typographer_does_not_touch_text_in_linkified_urls() {
    let input = r#"URL with (C) (c) (R) (r) (TM) (tm): https://example.com/(c)(r)(tm)/(C)(R)(TM) what do you think?"#;
    let output = r#"<p>URL with © © ® ® ™ ™: <a href="https://example.com/(c)(r)(tm)/(C)(R)(TM)">https://example.com/(c)(r)(tm)/(C)(R)(TM)</a> what do you think?</p>"#;

    run_with(input, output, |md| {
        markdown_it::plugins::extra::linkify::add(md);
    });
}
