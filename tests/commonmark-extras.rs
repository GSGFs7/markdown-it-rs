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
fn nul_link_destination_keeps_original_source_offsets() {
    let md = markdown_it::MarkdownIt::new();
    let source = "雪 [x](/a\0b) 尾";
    let document = md.parse_document(source);
    assert_eq!(document.source(), source);
    let link = document
        .events(document.root())
        .find_map(|event| {
            let node = event.node();
            node.is::<markdown_it::plugins::cmark::inline::link::Link>()
                .then_some(node)
        })
        .unwrap();
    assert_eq!(
        link.srcmap().unwrap().get_byte_offsets(),
        (source.find('[').unwrap(), source.find(')').unwrap() + 1)
    );
    assert_eq!(
        link.cast::<markdown_it::plugins::cmark::inline::link::Link>()
            .unwrap()
            .url,
        "/a%EF%BF%BDb"
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
fn emphasis_next_to_nul_keeps_original_source_offsets() {
    let md = markdown_it::MarkdownIt::new();
    let source = "雪 \0_a_";
    let document = md.parse_document(source);
    assert_eq!(document.source(), source);
    let start = source.find('_').unwrap();
    let emphasis = document
        .events(document.root())
        .find_map(|event| {
            let node = event.node();
            node.is::<markdown_it::plugins::cmark::inline::emphasis::Em>()
                .then_some(node)
        })
        .unwrap();
    assert_eq!(
        emphasis.srcmap().unwrap().get_byte_offsets(),
        (start, source.len())
    );
}

#[test]
fn emphasis_across_normalized_lines_preserves_source_offsets() {
    use markdown_it::plugins::cmark::inline::emphasis::{Em, Strong};
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
                let document = md.parse_document(&source);
                assert_eq!(
                    md.render_document(&document),
                    format!("<p>&lt;div&gt;</p>\n<p>{open}te<br>\n!90xt{close} 尾</p>\n"),
                    "{source:?}"
                );
                let node = document
                    .events(document.root())
                    .find_map(|event| {
                        let node = event.node();
                        (node.is::<Em>() || node.is::<Strong>() || node.is::<Strikethrough>())
                            .then_some(node)
                    })
                    .unwrap();
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
