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
    let node = md.parse(&source);

    // make sure we have sourcemaps for everything
    #[cfg(feature = "sourcemap")]
    for event in node.events(node.root()) {
        assert!(event.node().srcmap().is_some());
    }

    let result = md.render_html(&node);
    assert_eq!(result, output);

    let document = md.parse(&source);
    assert_eq!(md.render_html(&document), result);
    md.render_as(&document, "text");

    let mut direct_md = markdown_it::MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut direct_md);
    markdown_it::plugins::html::add(&mut direct_md);
    markdown_it::plugins::extra::tables::add(&mut direct_md);
    markdown_it::plugins::extra::typographer::add(&mut direct_md);
    let direct = direct_md.parse(&source);
    assert_eq!(direct_md.render_html(&direct), result);
    assert_eq!(
        direct_md.render_as(&direct, "text"),
        md.render_as(&document, "text"),
    );

    // make sure it doesn't crash without trailing \n
    let _ = md.parse(input.trim_end());
}

#[test]
fn table_headers_take_precedence_over_code_fences() {
    for marker in ["```", "~~~"] {
        run(
            &format!("{marker}|b|\n|---|---|\n|x|y|"),
            &format!(
                "<table>\n<thead>\n<tr>\n<th>{marker}</th>\n<th>b</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td>x</td>\n<td>y</td>\n</tr>\n</tbody>\n</table>"
            ),
        );
    }
}

#[test]
fn escaped_pipe_does_not_escape_the_next_cell_separator() {
    let source = "| a | b |\n| :- | -: |\n| `x\\|| `x\\|y]aac😀!z** |";
    run(
        source,
        "<table>\n<thead>\n<tr>\n<th style=\"text-align:left\">a</th>\n<th style=\"text-align:right\">b</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td style=\"text-align:left\">`x|</td>\n<td style=\"text-align:right\">`x|y]aac😀!z**</td>\n</tr>\n</tbody>\n</table>",
    );
    let md = markdown_it::MarkdownIt::with_preset(markdown_it::Preset::MarkdownItDefault);
    #[cfg(feature = "sourcemap")]
    let document = md.parse(source);
    #[cfg(feature = "sourcemap")]
    {
        let spans: Vec<_> = document
            .events(document.root())
            .filter_map(|event| {
                let node = event.node();
                let text = node.cast::<markdown_it::Text>()?;
                text.content.starts_with("`x").then(|| {
                    (
                        text.content.as_str(),
                        node.srcmap().unwrap().get_byte_offsets(),
                    )
                })
            })
            .collect();
        let row_start = source.rfind('\n').unwrap() + 1;
        assert_eq!(
            spans,
            vec![
                ("`x|", (row_start + 2, row_start + 6)),
                (
                    "`x|y]aac😀!z**",
                    (row_start + 8, source.rfind(" |").unwrap())
                ),
            ]
        );
    }
    for (row, left, right) in [
        (r"| a\|| b |", "a|", "b"),
        (r"| a\|\||b|", "a||", "b"),
        (r"| a\\|| b |", "a|", "b"),
    ] {
        let html = md.render(&format!("| a | b |\n| -- | -- |\n{row}"));
        assert!(
            html.contains(&format!("<td>{left}</td>\n<td>{right}</td>")),
            "{row:?}: {html}"
        );
    }
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
