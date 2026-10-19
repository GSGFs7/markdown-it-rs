#[test]
fn smartquotes_use_markdown_whitespace_boundaries() {
    for ch in ['\u{85}', '\u{2028}', '\u{2029}', '\u{feff}'] {
        run(&format!("a{ch}'x'"), &format!("<p>a{ch}’x’</p>"));
    }
    for ch in [
        '\u{a0}', '\u{1680}', '\u{2000}', '\u{200a}', '\u{202f}', '\u{205f}', '\u{3000}',
    ] {
        run(&format!("a{ch}'x'"), &format!("<p>a{ch}‘x’</p>"));
    }
}

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
    markdown_it::plugins::extra::smartquotes::add(md);
    let source = input.to_owned() + "\n";
    let node = md.parse(&source);

    // make sure we have sourcemaps for everything
    #[cfg(feature = "sourcemap")]
    for event in node.events(node.root()) {
        assert!(event.node().srcmap().is_some());
    }

    let result = md.render_html(&node);
    assert_eq!(result, output);

    let registry_md = &mut markdown_it::MarkdownIt::empty();
    markdown_it::plugins::cmark::add(registry_md);
    markdown_it::plugins::html::add(registry_md);
    markdown_it::plugins::extra::typographer::add(registry_md);
    markdown_it::plugins::extra::smartquotes::add(registry_md);
    let registry_document = registry_md.parse(&source);
    assert_eq!(registry_md.render_html(&registry_document), output);

    // make sure it doesn't crash without trailing \n
    let _ = md.parse(input.trim_end());
}

#[test]
fn smartquotes_transforms_autolink_labels() {
    for (source, expected) in [
        (
            "<a9:]ef=\"`\">`",
            "<p><a href=\"a9:%5Def=%22%60%22\">a9:]ef=“`”</a>`</p>",
        ),
        (
            "<https://example.org/\"x\">",
            "<p><a href=\"https://example.org/%22x%22\">https://example.org/“x”</a></p>",
        ),
        (
            "<foo'bar@example.org>",
            "<p><a href=\"mailto:foo'bar@example.org\">foo’bar@example.org</a></p>",
        ),
        (
            "<https://example.org/\"...\">",
            "<p><a href=\"https://example.org/%22...%22\">https://example.org/“...”</a></p>",
        ),
        (
            "`<https://example.org/\"x\">`",
            "<p><code>&lt;https://example.org/&quot;x&quot;&gt;</code></p>",
        ),
    ] {
        run(source, expected);
    }
}

#[test]
fn quotes_do_not_pair_between_sibling_emphasis_nodes() {
    run(
        "__<a [_ \r*_(b_f=\"__(b_f=\"__\">\n",
        "<p><em><em>&lt;a [</em>\n*_(b_f=&quot;</em><em>(b_f=&quot;</em>_&quot;&gt;</p>",
    );
    // Leaving an emphasis node invalidates its openers even for a sibling's
    // quote at the same depth.
    run(
        "*a \"b* *c\" d*",
        "<p><em>a &quot;b</em> <em>c&quot; d</em></p>",
    );
}

#[cfg(feature = "linkify")]
#[test]
fn smartquotes_transforms_linkified_labels() {
    let mut md = markdown_it::MarkdownIt::new();
    markdown_it::plugins::extra::linkify::add(&mut md);
    markdown_it::plugins::extra::typographer::add(&mut md);
    markdown_it::plugins::extra::smartquotes::add(&mut md);
    assert_eq!(
        md.render("https://example.org/a'b"),
        "<p><a href=\"https://example.org/a'b\">https://example.org/a’b</a></p>\n",
    );
}

#[test]
fn smartquotes_uses_escaped_and_entity_text_as_read_only_context() {
    for (source, expected) in [
        (
            "\\:[)é;0[url \"title \\\"&quot;\")\n",
            "<p>:[)é;0[url “title &quot;&quot;”)</p>",
        ),
        ("\"title \\\"&quot;\")", "<p>“title &quot;&quot;”)</p>"),
        ("\"foo &quot;\"", "<p>“foo &quot;”</p>"),
        ("\"foo \\\"\"", "<p>“foo &quot;”</p>"),
        ("&quot;foo&quot;", "<p>&quot;foo&quot;</p>"),
        ("\\\"foo\\\"", "<p>&quot;foo&quot;</p>"),
        ("foo&quot;\"bar\"", "<p>foo&quot;“bar”</p>"),
        ("\"foo\"&quot;", "<p>“foo”&quot;</p>"),
        ("\"foo\"&#32;bar", "<p>“foo” bar</p>"),
        ("&#32;\"foo\"", "<p> “foo”</p>"),
    ] {
        run(source, expected);
    }
}

#[test]
fn smartquotes_preserves_code_and_image_alt_text() {
    for (source, expected) in [
        (
            "`<a href=\"bar\">` \"outside\"",
            "<p><code>&lt;a href=&quot;bar&quot;&gt;</code> “outside”</p>",
        ),
        (
            "`\"code\" can't users'`",
            "<p><code>&quot;code&quot; can't users'</code></p>",
        ),
        (
            "\"one `\"inner\"` two\"",
            "<p>“one <code>&quot;inner&quot;</code> two”</p>",
        ),
        ("`word`'tail'", "<p><code>word</code>’tail’</p>"),
        (
            "![\"alt\" can't](/img) \"outside\"",
            "<p><img src=\"/img\" alt=\"&quot;alt&quot; can't\"> “outside”</p>",
        ),
        (
            "\"one ![alt](/img) two\"",
            "<p>“one <img src=\"/img\" alt=\"alt\"> two”</p>",
        ),
    ] {
        run(source, expected);
    }
}

#[test]
fn quotes_do_not_pair_across_inline_blocks() {
    for (source, expected) in [
        ("\"one\n\ntwo\"", "<p>&quot;one</p>\n<p>two&quot;</p>"),
        ("# \"one\n\ntwo\"", "<h1>&quot;one</h1>\n<p>two&quot;</p>"),
        (
            "\"one\n---\n\ntwo\"",
            "<h2>&quot;one</h2>\n<p>two&quot;</p>",
        ),
        ("\"one\n\n# two\"", "<p>&quot;one</p>\n<h1>two&quot;</h1>"),
        (
            "- \"one\n- two\"",
            "<ul>\n<li>&quot;one</li>\n<li>two&quot;</li>\n</ul>",
        ),
        (
            "> \"one\n\ntwo\"",
            "<blockquote>\n<p>&quot;one</p>\n</blockquote>\n<p>two&quot;</p>",
        ),
        (
            "`Foo\n----\n`\n\n<a title=\"a lot\n---\nof dashes\"/>\n",
            "<h2>`Foo</h2>\n<p>`</p>\n<h2>&lt;a title=&quot;a lot</h2>\n<p>of dashes&quot;/&gt;</p>",
        ),
    ] {
        run(source, expected);
    }
}

#[test]
fn quotes_do_not_pair_across_table_cells() {
    let mut md = markdown_it::MarkdownIt::new();
    markdown_it::plugins::extra::tables::add(&mut md);
    markdown_it::plugins::extra::smartquotes::add(&mut md);
    assert_eq!(
        md.render("| \"one | two\" |\n| --- | --- |"),
        "<table>\n<thead>\n<tr>\n<th>&quot;one</th>\n<th>two&quot;</th>\n</tr>\n</thead>\n</table>\n"
    );
}

#[test]
fn quotes_still_pair_across_line_breaks_and_inline_markup() {
    for (source, expected) in [
        ("\"one\ntwo\"", "<p>“one\ntwo”</p>"),
        ("\"one  \ntwo\"", "<p>“one<br>\ntwo”</p>"),
        ("\"one *two* three\"", "<p>“one <em>two</em> three”</p>"),
    ] {
        run(source, expected);
    }
}

#[test]
fn smartquotes_matches_utf16_boundaries_for_supplementary_characters() {
    for (source, expected) in [
        (
            "'\"double in single\"'😀b\n 0_\n\"'single in d文*ouuble'\"\n",
            "<p>'“double in single”'😀b\n0_\n“‘single in d文*ouuble’”</p>",
        ),
        ("'\"x\"'😀b", "<p>'“x”'😀b</p>"),
        ("'\"x\"'☀b", "<p>‘“x”’☀b</p>"),
        ("😀'x'😀", "<p>😀’x’😀</p>"),
        ("☀'x'☀", "<p>☀‘x’☀</p>"),
        ("😀\"x\"😀", "<p>😀&quot;x&quot;😀</p>"),
        ("☀\"x\"☀", "<p>☀“x”☀</p>"),
        ("'\"x\"'𐄀b", "<p>'“x”'𐄀b</p>"),
        ("𐄀'x'𐄀", "<p>𐄀’x’𐄀</p>"),
        ("😀*x*'tail'", "<p>😀<em>x</em>’tail’</p>"),
        ("'\"x\"'*😀*", "<p>'“x”'<em>😀</em></p>"),
    ] {
        run(source, expected);
    }
}

#[test]
fn smartquotes_classifies_nul_as_the_replacement_character() {
    for (source, expected) in [
        (
            "\"this\"\\\nand \0\"that\".\n\n\"this\" and\\\n\"t*\ta*0! \0",
            "<p>“this”<br>\nand �“that”.</p>\n<p>“this” and<br>\n&quot;t*\ta*0! �</p>",
        ),
        ("\0\"that\".", "<p>�“that”.</p>"),
        ("\"that\"\0", "<p>“that”�</p>"),
        ("\0'that'.", "<p>�‘that’.</p>"),
        ("'that'\0", "<p>‘that’�</p>"),
        ("\"a\0b\"", "<p>“a�b”</p>"),
        ("\u{0001}\"that\".", "<p>\u{0001}&quot;that&quot;.</p>"),
        ("�\"that\".", "<p>�“that”.</p>"),
    ] {
        run(source, expected);
    }
}

#[test]
fn smartquotes_uses_raw_image_labels_as_opaque_context() {
    for (source, expected) in [
        (
            "\"![&amp;&#10;](/img)\"",
            "<p>“<img src=\"/img\" alt=\"&amp;\n\">”</p>",
        ),
        (
            "\"![&#32;x&#32;](/img)\"",
            "<p>“<img src=\"/img\" alt=\" x \">”</p>",
        ),
        ("\"![*x*](/img)\"", "<p>“<img src=\"/img\" alt=\"x\">”</p>"),
        (
            "\"![a\nb](/img)\"",
            "<p>“<img src=\"/img\" alt=\"a\nb\">”</p>",
        ),
        (
            "\"![![x](/inner)](/outer)\"",
            "<p>“<img src=\"/outer\" alt=\"x\">”</p>",
        ),
        ("\"![](/img)\"", "<p>“<img src=\"/img\" alt=\"\">”</p>"),
        (
            "\"![&amp;&#10;][img]\"\n\n[img]: /img",
            "<p>“<img src=\"/img\" alt=\"&amp;\n\">”</p>",
        ),
    ] {
        run(source, expected);
    }
}
