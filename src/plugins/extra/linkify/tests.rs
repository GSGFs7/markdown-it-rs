use crate as markdown_it;

#[test]
fn session_cleanup_order_is_independent_of_plugin_registration_order() {
    let mut md = crate::MarkdownIt::empty();
    super::add(&mut md);
    crate::plugins::cmark::add(&mut md);
    crate::plugins::html::add(&mut md);
    assert_eq!(
        md.render("*a@b.co* **//example.org**"),
        "<p><em><a href=\"mailto:a@b.co\">a@b.co</a></em> <strong><a href=\"//example.org\">//example.org</a></strong></p>\n"
    );
}

#[test]
fn warmed_parser_refreshes_linkify_postpass_configuration() {
    use crate::parser::inline::InlineParserRule;

    let mut md = crate::MarkdownIt::new();
    super::add(&mut md);
    let source = "*a@b.co* https://example.org";
    for enabled in [false, true, false, true] {
        if enabled {
            md.add_rule::<super::LinkifyRule>()
                .after::<InlineParserRule>()
                .before_all();
        } else {
            md.remove_rule::<super::LinkifyRule>();
        }
        let email = if enabled {
            "<a href=\"mailto:a@b.co\">a@b.co</a>"
        } else {
            "a@b.co"
        };
        assert_eq!(
            md.render(source),
            format!(
                "<p><em>{email}</em> <a href=\"https://example.org\">https://example.org</a></p>\n"
            )
        );
    }
}

#[test]
#[cfg(feature = "sourcemap")]
fn inline_and_postpass_links_preserve_source_ranges() {
    let mut md = crate::MarkdownIt::new();
    super::add(&mut md);
    for source in [
        "雪 https://example.org *a@b.co* //example.net",
        "> 雪 https://example.org\n> *a@b.co* //example.net",
    ] {
        let document = md.parse(source);
        let mut ranges = Vec::new();
        for event in document.events(document.root()) {
            let node = event.node();
            if !matches!(event, crate::StructuralEvent::Enter(_)) || !node.is::<super::Linkified>()
            {
                continue;
            }
            let map = node.srcmap().unwrap();
            let (start, end) = map.get_byte_offsets();
            ranges.push(&source[start..end]);
            let children = document.children(node.id());
            assert_eq!(children.len(), 1);
            let inner = document.node(children[0]);
            assert_eq!(inner.srcmap().unwrap().get_byte_offsets(), (start, end));
            assert_eq!(
                inner.cast::<crate::document::TextSpecial>().unwrap().info,
                "autolink"
            );
        }
        // Explicit schemes are built inline; emails and // URLs are replaced
        // after emphasis. Container prefixes must not shift either path's map.
        assert_eq!(
            ranges,
            ["https://example.org", "a@b.co", "//example.net"],
            "{source}"
        );
    }
}

#[test]
fn retry_batches_siblings_and_nested_emphasis() {
    let input = format!(
        "<a>{}",
        "https://example.org *https://example.com* &amp; plain ".repeat(128)
    );
    let output = format!(
            "<p><a>{}</p>",
            "<a href=\"https://example.org\">https://example.org</a> <em><a href=\"https://example.com\">https://example.com</a></em> &amp; plain ".repeat(128),
        );
    // The parser trims trailing spaces at the end of the inline range.
    run(input.trim_end(), &output.replace("plain </p>", "plain</p>"));
}

#[test]
fn unmatched_html_anchor_retries_linkification() {
    let mut md = crate::MarkdownIt::new();
    crate::plugins::html::add(&mut md);
    super::add(&mut md);
    crate::plugins::extra::typographer::add(&mut md);
    crate::plugins::extra::smartquotes::add(&mut md);
    let run = |input: &str, output: &str| {
        assert_eq!(md.render(input), format!("{output}\n"));
    };
    run(
        r#"<a href>a!~|\中="/">https://example.org</a"#,
        r#"<p><a href>a!~|\中=“/”&gt;<a href="https://example.org">https://example.org</a>&lt;/a</p>"#,
    );
    run(
        "<a>https://example.org",
        "<p><a><a href=\"https://example.org\">https://example.org</a></p>",
    );
    run(
        "<a>a@b.co //example.org",
        "<p><a><a href=\"mailto:a@b.co\">a@b.co</a> <a href=\"//example.org\">//example.org</a></p>",
    );
    run(
        "<a>*https://example.org*</a",
        "<p><a><em><a href=\"https://example.org\">https://example.org</a></em>&lt;/a</p>",
    );
    run(
        "<a>https://example.org</a> <a>https://example.org",
        "<p><a>https://example.org</a> <a><a href=\"https://example.org\">https://example.org</a></p>",
    );
    run(
        "[<a>https://example.org](https://example.org)",
        "<p><a href=\"https://example.org\"><a>https://example.org</a></p>",
    );
}

#[test]
fn direct_prescan_marks_linkify_enabled_without_inline_scanners() {
    use super::{
        LinkifyEmailScanner,
        LinkifyEnabled,
        LinkifyFuzzyScanner,
        LinkifyPrescan,
        LinkifyScanner,
    };
    use crate::document::Root;
    use crate::parser::block::BlockParserRule;
    use crate::parser::inline::InlineParserRule;
    for before_blocks in [false, true] {
        let mut md = crate::MarkdownIt::empty();
        super::add(&mut md);
        md.inline.remove_rule::<LinkifyScanner>();
        md.inline.remove_rule::<LinkifyFuzzyScanner>();
        md.inline.remove_rule::<LinkifyEmailScanner>();
        md.remove_rule::<LinkifyPrescan>();
        if before_blocks {
            md.add_rule::<LinkifyPrescan>().before::<BlockParserRule>();
        } else {
            md.add_rule::<LinkifyPrescan>()
                .after::<BlockParserRule>()
                .before::<InlineParserRule>();
        }
        for limit in [0, 100] {
            md.max_nesting = limit;
            for source in ["", "雪 https://example.com\r\na@b.co"] {
                let document = md.parse(source);
                let root = document.node(document.root()).cast::<Root>().unwrap();
                assert!(
                    root.ext.contains::<LinkifyEnabled>(),
                    "{source:?}, before_blocks={before_blocks}, limit={limit}"
                );
            }
        }
    }
}

#[test]
fn prescan_does_not_run_inline_postprocessors_too_early() {
    use crate::plugins::cmark;
    use crate::plugins::extra::*;

    let md = &mut MarkdownIt::empty();
    cmark::add(md);
    typographer::add(md);
    smartquotes::add(md);
    linkify::add(md);

    assert_eq!(md.render(r#"a~~"foo"~~"#), "<p>a~~“foo”~~</p>\n");
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
    markdown_it::plugins::extra::linkify::add(md);
    let node = md.parse(&(input.to_owned() + "\n"));

    // make sure we have sourcemaps for everything
    #[cfg(feature = "sourcemap")]
    for event in node.events(node.root()) {
        assert!(event.node().srcmap().is_some());
    }

    let result = md.render_html(&node);
    assert_eq!(result, output);
    // make sure it doesn't crash without trailing \n
    let _ = md.parse(input.trim_end());
}

#[test]
fn linkify() {
    let input = r#"url http://www.youtube.com/watch?v=5Jt5GEr4AYg."#;
    let output = r#"<p>url <a href="http://www.youtube.com/watch?v=5Jt5GEr4AYg">http://www.youtube.com/watch?v=5Jt5GEr4AYg</a>.</p>"#;
    run(input, output);
}

#[test]
fn don_t_touch_text_in_links() {
    let input = r#"[https://example.com](https://example.com)"#;
    let output = r#"<p><a href="https://example.com">https://example.com</a></p>"#;
    run(input, output);
}

#[test]
fn don_t_touch_text_in_autolinks() {
    let input = r#"<https://example.com>"#;
    let output = r#"<p><a href="https://example.com">https://example.com</a></p>"#;
    run(input, output);
}

#[test]
fn don_t_touch_text_in_html_a_tags() {
    let input = r#"<a href="https://example.com">https://example.com</a>"#;
    let output = r#"<p><a href="https://example.com">https://example.com</a></p>"#;
    run(input, output);
}

#[test]
fn entities_inside_raw_links() {
    let input = r#"https://example.com/foo&amp;bar"#;
    let output = r#"<p><a href="https://example.com/foo&amp;amp;bar">https://example.com/foo&amp;amp;bar</a></p>"#;
    run(input, output);
}

#[test]
fn emphasis_inside_raw_links_asterisk_can_happen_in_links_with_params() {
    let input = r#"https://example.com/foo*bar*baz"#;
    let output =
        r#"<p><a href="https://example.com/foo*bar*baz">https://example.com/foo*bar*baz</a></p>"#;
    run(input, output);
}

#[test]
fn emphasis_inside_raw_links_underscore() {
    let input = r#"http://example.org/foo._bar_-_baz"#;
    let output = r#"<p><a href="http://example.org/foo._bar_-_baz">http://example.org/foo._bar_-_baz</a></p>"#;
    run(input, output);
}

#[test]
fn backticks_inside_raw_links() {
    let input = r#"https://example.com/foo`bar`baz"#;
    let output = r#"<p><a href="https://example.com/foo%60bar%60baz">https://example.com/foo`bar`baz</a></p>"#;
    run(input, output);
}

#[test]
fn unicode_tlds_are_linkified_and_normalized() {
    run(
        "https://例子.测试/a_(b)?x=1&y=2",
        "<p><a href=\"https://xn--fsqu00a.xn--0zwm56d/a_(b)?x=1&amp;y=2\">https://例子.测试/a_(b)?x=1&amp;y=2</a></p>",
    );
}

#[test]
fn pipes_in_url_paths_are_linkified_and_encoded() {
    run(
        "https://例子.测试/a_(b)?x=1|é~&y=2é[aé(",
        "<p><a href=\"https://xn--fsqu00a.xn--0zwm56d/a_(b)?x=1%7C%C3%A9~&amp;y=2%C3%A9\">https://例子.测试/a_(b)?x=1|é~&amp;y=2é</a>[aé(</p>",
    );
    run(
        "//example.com/a|b",
        "<p><a href=\"//example.com/a%7Cb\">//example.com/a|b</a></p>",
    );
}

#[test]
fn links_inside_raw_links() {
    let input = r#"https://example.com/foo[123](456)bar"#;
    let output = r#"<p><a href="https://example.com/foo%5B123%5D(456)bar">https://example.com/foo[123](456)bar</a></p>"#;
    run(input, output);
}

#[test]
fn escapes_not_allowed_at_the_start() {
    let input = r#"\https://example.com"#;
    let output = r#"<p>\https://example.com</p>"#;
    run(input, output);
}

#[test]
fn escapes_not_allowed_at_comma() {
    let input = r#"https\://example.com"#;
    let output = r#"<p>https://example.com</p>"#;
    run(input, output);
}

#[test]
fn escapes_not_allowed_at_slashes() {
    let input = r#"https:\//aa.org https://bb.org"#;
    let output = r#"<p>https://aa.org <a href="https://bb.org">https://bb.org</a></p>"#;
    run(input, output);
}

#[test]
fn fuzzy_link_shouldn_t_match_cc_org() {
    let input = r#"https:/\/cc.org"#;
    let output = r#"<p>https://cc.org</p>"#;
    run(input, output);
}

#[test]
fn bold_links_exclude_markup_of_pairs_from_link_tail() {
    let input = r#"**http://example.com/foobar**"#;
    let output = r#"<p><strong><a href="http://example.com/foobar">http://example.com/foobar</a></strong></p>"#;
    run(input, output);
}

#[test]
fn match_links_without_protocol() {
    let md = &mut markdown_it::MarkdownIt::empty();
    markdown_it::plugins::cmark::add(md);
    markdown_it::plugins::extra::linkify::add_with_options(
        md,
        markdown_it::plugins::extra::linkify::LinkifyOptions { fuzzy_links: true },
    );

    assert_eq!(
        md.render("www.example.org"),
        "<p><a href=\"http://www.example.org\">www.example.org</a></p>\n"
    );
}

#[test]
fn links_without_protocol_are_disabled_by_default() {
    let md = &mut markdown_it::MarkdownIt::empty();
    markdown_it::plugins::cmark::add(md);
    markdown_it::plugins::extra::linkify::add(md);

    assert_eq!(md.render("www.example.org"), "<p>www.example.org</p>\n");
}

#[test]
fn explicit_urls_after_failed_protocol_relative_prefixes() {
    for (input, output) in [
        (
            "<//https://foo.[a",
            "<p>&lt;//<a href=\"https://foo\">https://foo</a>.[a</p>",
        ),
        (
            "//https://foo",
            "<p>//<a href=\"https://foo\">https://foo</a></p>",
        ),
        (
            "//HTTPS://foo",
            "<p>//<a href=\"HTTPS://foo\">HTTPS://foo</a></p>",
        ),
        (
            "<//ftp://foo.[a",
            "<p>&lt;//<a href=\"ftp://foo\">ftp://foo</a>.[a</p>",
        ),
        (
            "<//https://foo*",
            "<p>&lt;//<a href=\"https://foo\">https://foo</a>*</p>",
        ),
        ("//\\https://foo", "<p>//\\https://foo</p>"),
        (
            "[//https://foo](/url)",
            "<p><a href=\"/url\">//https://foo</a></p>",
        ),
        ("<a>//https://foo</a>", "<p><a>//https://foo</a></p>"),
        ("//ahttps://foo", "<p>//ahttps://foo</p>"),
        ("//mailto:a@b.co", "<p>//mailto:a@b.co</p>"),
    ] {
        run(input, output);
    }
}

#[test]
fn protocol_relative_boundaries_match_linkify_it() {
    for input in [
        "x//example.com",
        "http:////example.com",
        "////example.com",
        "组//example.com",
    ] {
        run(input, &format!("<p>{input}</p>"));
    }

    run(
        "。//example.com",
        "<p>。<a href=\"//example.com\">//example.com</a></p>",
    );
    run(
        "//example.com//other.org",
        "<p><a href=\"//example.com//other.org\">//example.com//other.org</a></p>",
    );
}

#[test]
fn protocol_relative_links_are_matched_after_emphasis() {
    for (input, output) in [
        (
            "__a<https(:0:&//foo.bar/?q=__>",
            "<p><strong>a&lt;https(:0:&amp;<a href=\"//foo.bar/?q=\">//foo.bar/?q=</a></strong>&gt;</p>",
        ),
        (
            "*//example.org/a*",
            "<p><em><a href=\"//example.org/a\">//example.org/a</a></em></p>",
        ),
        (
            "__//example.org/a__",
            "<p><strong>//example.org/a</strong></p>",
        ),
        (
            "//example.org/a__",
            "<p><a href=\"//example.org/a__\">//example.org/a__</a></p>",
        ),
        (
            "//example.org/a_b",
            "<p><a href=\"//example.org/a_b\">//example.org/a_b</a></p>",
        ),
        (
            "https://example.org/a__",
            "<p><a href=\"https://example.org/a__\">https://example.org/a__</a></p>",
        ),
        (
            "[//example.org/a](/url)",
            "<p><a href=\"/url\">//example.org/a</a></p>",
        ),
        ("<a>//example.org/a</a>", "<p><a>//example.org/a</a></p>"),
        ("`//example.org/a`", "<p><code>//example.org/a</code></p>"),
        ("&amp;//example.org/a", "<p>&amp;//example.org/a</p>"),
    ] {
        run(input, output);
    }
}

#[test]
fn linkify_postpass_preserves_inline_code() {
    for (input, output) in [
        (
            "`<https:a__\0(:\t//foo.bar.`baz>`\n",
            "<p><code>&lt;https:a__�(:\t//foo.bar.</code>baz&gt;`</p>",
        ),
        (
            "`<https:\t//foo.bar`baz>`",
            "<p><code>&lt;https:\t//foo.bar</code>baz&gt;`</p>",
        ),
        (
            "`//example.org/a` //example.com/b",
            "<p><code>//example.org/a</code> <a href=\"//example.com/b\">//example.com/b</a></p>",
        ),
        (
            "<a>`https://example.org a@b.co //example.com` https://example.net",
            "<p><a><code>https://example.org a@b.co //example.com</code> <a href=\"https://example.net\">https://example.net</a></p>",
        ),
    ] {
        run(input, output);
    }
}

#[test]
fn protocol_relative_links_without_emphasis_plugin() {
    let mut md = crate::MarkdownIt::empty();
    crate::plugins::cmark::block::paragraph::add(&mut md);
    super::add(&mut md);
    for (input, output) in [
        (
            "//example.org/a__",
            "<p><a href=\"//example.org/a__\">//example.org/a__</a></p>\n",
        ),
        (
            "雪 //example.org/a_b tail //example.com/b",
            "<p>雪 <a href=\"//example.org/a_b\">//example.org/a_b</a> tail <a href=\"//example.com/b\">//example.com/b</a></p>\n",
        ),
    ] {
        let document = md.parse(input);
        assert_eq!(md.render_html(&document), output);
        #[cfg(feature = "sourcemap")]
        for event in document.events(document.root()) {
            let node = event.node();
            if let Some(link) = node.cast::<super::Linkified>() {
                let (start, end) = node.srcmap().unwrap().get_byte_offsets();
                assert_eq!(&input[start..end], link.url);
            }
        }
    }
}

#[test]
fn short_email_with_beautifier_does_not_panic() {
    let md = &mut markdown_it::MarkdownIt::empty();
    markdown_it::plugins::cmark::add(md);
    markdown_it::plugins::extra::beautify_links::add(md);
    markdown_it::plugins::extra::linkify::add(md);

    assert_eq!(
        md.render("ping a@b.co ok"),
        "<p>ping <a href=\"mailto:a@b.co\">a@b.co</a> ok</p>\n"
    );
}

#[test]
fn emails() {
    let input = r#"test@example.com

mailto:test@example.com"#;
    let output = r#"<p><a href="mailto:test@example.com">test@example.com</a></p>
<p><a href="mailto:test@example.com">mailto:test@example.com</a></p>"#;
    run(input, output);
}

#[test]
fn emails_respect_reverse_html_anchor_boundaries() {
    for (input, output) in [
        ("b@example.com</a>", "<p>b@example.com</a></p>"),
        (
            "mailto:b@example.com</a>",
            "<p>mailto:b@example.com</a></p>",
        ),
        ("b@example.com</a></a>", "<p>b@example.com</a></a></p>"),
        ("*b@example.com*</A >", "<p><em>b@example.com</em></A ></p>"),
        (
            "`a`+b@example.oé</a>",
            "<p><code>a</code>+b@example.oé</a></p>",
        ),
        (
            "b@example.com</a> c@example.com",
            "<p>b@example.com</a> <a href=\"mailto:c@example.com\">c@example.com</a></p>",
        ),
        (
            "<a>b@example.com</a> c@example.com",
            "<p><a>b@example.com</a> <a href=\"mailto:c@example.com\">c@example.com</a></p>",
        ),
        (
            "</a>b@example.com",
            "<p></a><a href=\"mailto:b@example.com\">b@example.com</a></p>",
        ),
        (
            "*a*+b@example.com",
            "<p><em>a</em><a href=\"mailto:+b@example.com\">+b@example.com</a></p>",
        ),
        (
            "[</a>b@example.com](/url)",
            "<p><a href=\"/url\"></a>b@example.com</a></p>",
        ),
    ] {
        run(input, output);
    }
}

#[test]
fn emails_after_inline_code_are_matched_in_remaining_text() {
    for (input, output) in [
        (
            "`a<`+b@example.oé! é[#\t&",
            "<p><code>a&lt;</code><a href=\"mailto:+b@example.xn--o-bga\">+b@example.oé</a>! é[#\t&amp;</p>",
        ),
        (
            "`a`b@example.com",
            "<p><code>a</code><a href=\"mailto:b@example.com\">b@example.com</a></p>",
        ),
        (
            "`a<b@example.com`+c@example.com",
            "<p><code>a&lt;b@example.com</code><a href=\"mailto:+c@example.com\">+c@example.com</a></p>",
        ),
        (
            "`a`+b@example.com `c`+d@example.com",
            "<p><code>a</code><a href=\"mailto:+b@example.com\">+b@example.com</a> <code>c</code><a href=\"mailto:+d@example.com\">+d@example.com</a></p>",
        ),
        (
            "`a`+b@example.com <a>+c@example.com</a>",
            "<p><code>a</code><a href=\"mailto:+b@example.com\">+b@example.com</a> <a>+c@example.com</a></p>",
        ),
        (
            "[`a`+b@example.com](/url)",
            "<p><a href=\"/url\"><code>a</code>+b@example.com</a></p>",
        ),
        ("\\+b@example.com", "<p>+b@example.com</p>"),
        ("&amp;+b@example.com", "<p>&amp;+b@example.com</p>"),
    ] {
        run(input, output);
    }
}

#[test]
fn email_tlds_can_contain_digits() {
    run(
        "<foo+special@Bar.b9\n[bz-barar0.com>\n",
        "<p>&lt;<a href=\"mailto:foo+special@Bar.b9\">foo+special@Bar.b9</a>\n[bz-barar0.com&gt;</p>",
    );
}

#[test]
fn typorgapher_should_not_break_href() {
    let input = r#"http://example.com/(c)"#;
    let output = r#"<p><a href="http://example.com/(c)">http://example.com/(c)</a></p>"#;
    run(input, output);
}

#[test]
fn coverage_prefix_not_valid() {
    let input = r#"http:/example.com/"#;
    let output = r#"<p>http:/example.com/</p>"#;
    run(input, output);
}

#[test]
fn unregistered_schemes_are_not_linkified() {
    let input = r#"a://a://"#;
    let output = r#"<p>a://a://</p>"#;
    run(input, output);
}

#[test]
fn coverage_negative_link_level() {
    let input = r#"</a>[https://example.com](https://example.com)"#;
    let output = r#"<p></a><a href="https://example.com"><a href="https://example.com">https://example.com</a></a></p>"#;
    run(input, output);
}

#[test]
fn emphasis_with_real_link() {
    let input = r#"http://cdecl.ridiculousfish.com/?q=int+%28*f%29+%28float+*%29%3B"#;
    let output = r#"<p><a href="http://cdecl.ridiculousfish.com/?q=int+%28*f%29+%28float+*%29%3B">http://cdecl.ridiculousfish.com/?q=int+(*f)+(float+*)%3B</a></p>"#;
    run(input, output);
}

#[test]
fn emphasis_with_real_link_1() {
    let input = r#"https://www.sell.fi/sites/default/files/elainlaakarilehti/tieteelliset_artikkelit/kahkonen_t._et_al.canine_pancreatitis-_review.pdf"#;
    let output = r#"<p><a href="https://www.sell.fi/sites/default/files/elainlaakarilehti/tieteelliset_artikkelit/kahkonen_t._et_al.canine_pancreatitis-_review.pdf">https://www.sell.fi/sites/default/files/elainlaakarilehti/tieteelliset_artikkelit/kahkonen_t._et_al.canine_pancreatitis-_review.pdf</a></p>"#;
    run(input, output);
}
