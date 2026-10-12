fn run(input: &str, output: &str) {
    let output = if output.is_empty() {
        "".to_owned()
    } else {
        output.to_owned() + "\n"
    };
    let md = &mut markdown_it::MarkdownIt::empty();
    markdown_it::plugins::cmark::add(md);
    markdown_it::plugins::html::add(md);
    markdown_it::plugins::extra::typographer::add(md);
    markdown_it::plugins::extra::tables::add(md);
    let source = input.to_owned() + "\n";
    let node = md.parse_document(&source);

    // make sure we have sourcemaps for everything
    for event in node.events(node.root()) {
        assert!(event.node().srcmap().is_some());
    }

    let result = md.render_document(&node);
    assert_eq!(result, output);

    let document = md.parse_document(&source);
    assert_eq!(md.render_document(&document), result);
    md.render_document_as(&document, "text");

    let mut direct_md = markdown_it::MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut direct_md);
    markdown_it::plugins::html::add(&mut direct_md);
    markdown_it::plugins::extra::tables::add(&mut direct_md);
    markdown_it::plugins::extra::typographer::add(&mut direct_md);
    let direct = direct_md.parse_document(&source);
    assert_eq!(direct_md.render_document(&direct), result);
    assert_eq!(
        direct_md.render_document_as(&direct, "text"),
        md.render_document_as(&document, "text"),
    );

    // make sure it doesn't crash without trailing \n
    let _ = md.parse_document(input.trim_end());
}

#[test]
fn table_headers_require_a_pipe() {
    for (source, expected) in [
        (
            "&é\n:------:\nbar | baz",
            "<p>&amp;é\n:------:\nbar | baz</p>",
        ),
        ("foo\n|---\nbar", "<p>foo\n|—\nbar</p>"),
    ] {
        run(source, expected);
    }
    run(
        "| abc  |\n:-: |\r\r&é\r:------:\nbar | baz\n",
        "<table>\n<thead>\n<tr>\n<th style=\"text-align:center\">abc</th>\n</tr>\n</thead>\n</table>\n<p>&amp;é\n:------:\nbar | baz</p>",
    );
}

#[test]
fn single_column_tables_allow_plain_dash_delimiters() {
    for delimiter in ["--", "---"] {
        run(
            &format!("before\n|a\n{delimiter}\n"),
            "<p>before</p>\n<table>\n<thead>\n<tr>\n<th>a</th>\n</tr>\n</thead>\n</table>",
        );
    }
    let md = markdown_it::MarkdownIt::with_preset(markdown_it::Preset::MarkdownItDefault);
    for delimiter in ["-", "- ", "-\t"] {
        assert!(!md.render(&format!("|a\n{delimiter}")).contains("<table>"));
    }
}
