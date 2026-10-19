mod common;

fn run(input: &str, output: &str) {
    let expected = if output.is_empty() {
        String::new()
    } else {
        output.to_owned() + "\n"
    };

    let md = common::markdown_it_fixture_parser();
    let actual = md.render(&(input.to_owned() + "\n"));
    assert_eq!(actual, expected);
}

#[test]
fn empty_link_and_image_titles_match_markdown_it() {
    for (open, close) in [('"', '"'), ('\'', '\''), ('(', ')')] {
        for (title, attr) in [
            ("", ""),
            (" ", " title=\" \""),
            ("title", " title=\"title\""),
        ] {
            let expected = format!(
                "<p><a href=\"/url\"{attr}>link</a>\n<img src=\"/url\" alt=\"image\"{attr}></p>"
            );
            run(
                &format!("[link](/url {open}{title}{close})\n![image](/url {open}{title}{close})"),
                &expected,
            );
            run(
                &format!("[link][ref]\n![image][ref]\n\n[ref]: /url {open}{title}{close}"),
                &expected,
            );
        }
    }
}

// markdown-it compatibility: it reasonable???
#[test]
fn unknown_autolink_protocol_preserves_punycode_display_text() {
    run(
        "<httttp://xn--n3h.net/>",
        "<p><a href=\"httttp://xn--n3h.net/\">httttp://xn--n3h.net/</a></p>",
    );
}

#[test]
fn autolink_relocates_invalid_host_suffix_after_numeric_port() {
    let md = markdown_it::MarkdownIt::new();
    assert_eq!(
        md.render("<irc://foo.bar:2233r:2:33/baaz>\n"),
        "<p><a href=\"irc://foo.bar:33:2233r:2/baaz\">irc://foo.bar:33:2233r:2/baaz</a></p>\n"
    );
}

#[test]
fn autolink_normalizes_host_before_semicolon_suffix() {
    let md = markdown_it::MarkdownIt::new();
    assert_eq!(
        md.render("[foo<https://examp\0;.com/?searc?search=](uri)>\n"),
        "<p>[foo<a href=\"https://xn--examp-pm43a;.com/?searc?search=%5D(uri)\">https://examp�;.com/?searc?search=](uri)</a></p>\n"
    );
}

#[test]
fn link_destinations_treat_nul_as_the_replacement_character() {
    for (source, expected) in [
        (
            "[foo #bar](/uri\0>])",
            "<p><a href=\"/uri%EF%BF%BD%3E%5D\">foo #bar</a></p>",
        ),
        ("[x](</a\0b>)", "<p><a href=\"/a%EF%BF%BDb\">x</a></p>"),
        ("[x](/a\\\0b)", "<p><a href=\"/a%5C%EF%BF%BDb\">x</a></p>"),
        ("![x](/a\0b)", "<p><img src=\"/a%EF%BF%BDb\" alt=\"x\"></p>"),
        (
            "[x][r]\n\n[r]: /a\0b",
            "<p><a href=\"/a%EF%BF%BDb\">x</a></p>",
        ),
        ("[x](/a\x01b)", "<p>[x](/a\x01b)</p>"),
    ] {
        run(source, expected);
    }
}

#[test]
fn nul_link_destination_uses_normalized_source_offsets() {
    let md = markdown_it::MarkdownIt::new();
    let source = "雪 [x](/a\0b) 尾";
    let normalized = "雪 [x](/a\u{fffd}b) 尾";
    let document = md.parse(source);
    assert_eq!(document.source(), normalized);
    let link = document
        .events(document.root())
        .find_map(|event| {
            let node = event.node();
            node.is::<markdown_it::plugins::cmark::inline::link::Link>()
                .then_some(node)
        })
        .unwrap();
    #[cfg(feature = "sourcemap")]
    assert_eq!(
        link.srcmap().unwrap().get_byte_offsets(),
        (
            normalized.find('[').unwrap(),
            normalized.find(')').unwrap() + 1
        )
    );
    assert_eq!(
        link.cast::<markdown_it::plugins::cmark::inline::link::Link>()
            .unwrap()
            .url,
        "/a%EF%BF%BDb"
    );
}

#[test]
fn emphasis_uses_markdown_whitespace_boundaries() {
    for ch in ['\u{85}', '\u{2028}', '\u{2029}', '\u{feff}'] {
        for (source, expected) in [
            (format!("x{ch}_a_"), format!("<p>x{ch}_a_</p>")),
            (format!("*{ch}a*"), format!("<p><em>{ch}a</em></p>")),
            (format!("*a{ch}*"), format!("<p><em>a{ch}</em></p>")),
            (format!("_{ch}a_"), format!("<p><em>{ch}a</em></p>")),
            (
                format!("**{ch}a**"),
                format!("<p><strong>{ch}a</strong></p>"),
            ),
            (format!("~~{ch}a~~"), format!("<p><s>{ch}a</s></p>")),
        ] {
            run(&source, &expected);
        }
    }
    for ch in [
        '\u{b}', '\u{c}', '\u{a0}', '\u{1680}', '\u{2000}', '\u{200a}', '\u{202f}', '\u{205f}',
        '\u{3000}',
    ] {
        run(&format!("x{ch}_a_"), &format!("<p>x{ch}<em>a</em></p>"));
        run(&format!("*{ch}a*"), &format!("<p>*{ch}a*</p>"));
        run(&format!("~~{ch}a~~"), &format!("<p>~~{ch}a~~</p>"));
    }
    run(
        "<b\u{85}_b____ c____](/url)",
        "<p>&lt;b\u{85}_b____ c____](/url)</p>",
    );
}

#[test]
fn emphasis_classifies_nul_as_the_replacement_character() {
    for (source, expected) in [
        (
            "_foo _bar_ <_\0\\\\😀",
            "<p><em>foo <em>bar</em> &lt;</em>�\\😀</p>",
        ),
        ("_a_\0", "<p><em>a</em>�</p>"),
        ("\0_a_", "<p>�<em>a</em></p>"),
        ("_a\0_", "<p><em>a�</em></p>"),
        ("*a*\0", "<p><em>a</em>�</p>"),
        ("a_\0b_", "<p>a_�b_</p>"),
        ("a_\0_b", "<p>a_�_b</p>"),
        ("_a_\u{0001}", "<p>_a_\u{0001}</p>"),
    ] {
        run(source, expected);
    }
}

#[test]
fn emphasis_next_to_nul_uses_normalized_source_offsets() {
    let md = markdown_it::MarkdownIt::new();
    let source = "雪 \0_a_";
    let normalized = "雪 \u{fffd}_a_";
    let document = md.parse(source);
    assert_eq!(document.source(), normalized);
    #[cfg(feature = "sourcemap")]
    let start = normalized.find('_').unwrap();
    #[cfg(feature = "sourcemap")]
    let emphasis = document
        .events(document.root())
        .find_map(|event| {
            let node = event.node();
            node.is::<markdown_it::plugins::cmark::inline::emphasis::Em>()
                .then_some(node)
        })
        .unwrap();
    #[cfg(feature = "sourcemap")]
    assert_eq!(
        emphasis.srcmap().unwrap().get_byte_offsets(),
        (start, normalized.len())
    );
}

#[test]
fn emphasis_across_normalized_lines_preserves_source_offsets() {
    #[cfg(feature = "sourcemap")]
    use markdown_it::plugins::cmark::inline::emphasis::{Em, Strong};
    #[cfg(feature = "sourcemap")]
    use markdown_it::plugins::extra::strikethrough::Strikethrough;

    let mut md = markdown_it::MarkdownIt::new();
    markdown_it::plugins::extra::strikethrough::add(&mut md);
    md.render_options.breaks = true;
    md.render_options.xhtml_out = false;
    for newline in ["\n", "\r\n", "\r"] {
        for indent in ["", "\t", "   "] {
            for (marker, open, close) in [
                ("*", "<em>", "</em>"),
                ("_", "<em>", "</em>"),
                ("**", "<strong>", "</strong>"),
                ("***", "<em><strong>", "</strong></em>"),
                ("~~", "<s>", "</s>"),
            ] {
                let source = format!("<div>\n\n{marker}te{newline}{indent}!90xt{marker} 尾\n");
                let document = md.parse(&source);
                assert_eq!(
                    md.render_html(&document),
                    format!("<p>&lt;div&gt;</p>\n<p>{open}te<br>\n!90xt{close} 尾</p>\n"),
                    "{source:?}"
                );
                #[cfg(feature = "sourcemap")]
                let node = document
                    .events(document.root())
                    .find_map(|event| {
                        let node = event.node();
                        (node.is::<Em>() || node.is::<Strong>() || node.is::<Strikethrough>())
                            .then_some(node)
                    })
                    .unwrap();
                #[cfg(feature = "sourcemap")]
                assert_eq!(
                    node.srcmap().unwrap().get_byte_offsets(),
                    (source.find(marker).unwrap(), source.find(" 尾").unwrap()),
                    "{source:?}"
                );
            }
        }
    }
}

#[test]
fn link_destinations_trim_trailing_line_breaks_before_encoding() {
    let mut md = markdown_it::MarkdownIt::new();
    md.render_options.breaks = true;
    md.render_options.xhtml_out = false;
    assert_eq!(
        md.render("[ﬀﬁﬂ]\n\n[ff\n\n[fffifl]:|9\\\r0 /url\nurl"),
        "<p><a href=\"%7C9%5C\">ﬀﬁﬂ</a></p>\n<p>[ff</p>\n<p>0 /url<br>\nurl</p>\n"
    );
    for newline in ["\n", "\r\n", "\r"] {
        assert_eq!(
            md.render(&format!("[x]\n\n[x]: /url\\{newline}next")),
            "<p><a href=\"/url%5C\">x</a></p>\n<p>next</p>\n"
        );
        assert_eq!(
            md.render(&format!("[x](/url\\{newline})")),
            "<p><a href=\"/url%5C\">x</a></p>\n"
        );
    }
    assert_eq!(
        md.render("[x](< /url >)"),
        "<p><a href=\"/url\">x</a></p>\n"
    );
}

#[test]
fn malformed_bracketed_link_hosts_encode_outer_brackets() {
    let mut md = markdown_it::MarkdownIt::new();
    md.max_nesting = 3;
    assert_eq!(
        md.render("[foo](//[::f|ff:192.0.2.1]/)\n"),
        "<p><a href=\"//%5B::f%7Cff:192.0.2.1%5D/\">foo</a></p>\n"
    );
    assert_eq!(
        md.render("[foo](//[::ffff:192.0.2.1]/)\n"),
        "<p><a href=\"//[::ffff:192.0.2.1]/\">foo</a></p>\n"
    );
}

#[test]
fn bracketed_link_host_with_inner_closing_bracket_preserves_outer_brackets() {
    let md = markdown_it::MarkdownIt::new();
    assert_eq!(
        md.render("[foo](http://[2001:db8::1]:1]:1896/a[b]?x=[y])\n"),
        "<p><a href=\"http://[2001:db8::1%5D:1]:1896/a%5Bb%5D?x=%5By%5D\">foo</a></p>\n"
    );
}

#[test]
fn unclosed_bracketed_link_host_extracts_port_before_encoding() {
    let md = markdown_it::MarkdownIt::new();
    assert_eq!(
        md.render("[foo](http://[2001:db:1896/a[b]?x=[y])\n"),
        "<p><a href=\"http://:1896%5B2001:db/a%5Bb%5D?x=%5By%5D\">foo</a></p>\n"
    );
}

#[test]
fn image_url_with_unicode_and_forbidden_host_punctuation_is_not_punycoded() {
    let md = markdown_it::MarkdownIt::new();
    assert_eq!(
        md.render("There is a newline in this image ![here\nit is](https://中!)0*c😀&github/executablebooks/)\n"),
        "<p>There is a newline in this image <img src=\"https://%E4%B8%AD!\" alt=\"here\nit is\">0*c😀&amp;github/executablebooks/)</p>\n"
    );
}

#[test]
fn autolink_display_replaces_invalid_percent_encoded_utf8() {
    let md = markdown_it::MarkdownIt::new();
    assert_eq!(
        md.render("<http://example.com/α%Ccom/α%CE%B2γ%CE%B4>\n"),
        "<p><a href=\"http://example.com/%CE%B1%Ccom/%CE%B1%CE%B2%CE%B3%CE%B4\">http://example.com/α�om/αβγδ</a></p>\n"
    );
}
