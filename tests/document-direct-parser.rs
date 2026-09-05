use markdown_it::parser::core::Root;
use markdown_it::{DocumentParseError, MarkdownIt};

fn assert_direct_matches_bridge(md: &MarkdownIt, source: &str) {
    let bridged = md.parse_document(source);
    let direct = md.parse_document_direct(source).unwrap();

    assert_eq!(direct.source(), source);
    assert_eq!(direct.len(), bridged.len(), "node count for {source:?}");
    assert_eq!(
        direct
            .node(direct.root())
            .unwrap()
            .cast::<Root>()
            .unwrap()
            .content,
        source
    );
    assert_eq!(
        md.render_document(&direct).unwrap(),
        md.render_document(&bridged).unwrap(),
        "HTML for {source:?}"
    );
    assert_eq!(
        md.render_document_as(&direct, "text").unwrap(),
        md.render_document_as(&bridged, "text").unwrap(),
        "text for {source:?}"
    );
    assert_eq!(
        md.render_document_as(&direct, "debug").unwrap(),
        md.render_document_as(&bridged, "debug").unwrap(),
        "debug tree for {source:?}"
    );
    assert_eq!(
        direct.into_legacy().render(),
        md.parse(source).render(),
        "legacy conversion for {source:?}"
    );
}

#[test]
fn direct_text_fallback_matches_the_legacy_bridge() {
    let md = MarkdownIt::empty();
    let sources = [
        "",
        "hello",
        "  leading\n\nsecond  ",
        "punctuation *<&> and 雪\n",
        "first\r\n\tsecond\rthird",
        "nul \0 byte",
    ];

    for source in sources {
        assert_direct_matches_bridge(&md, source);
    }
}

#[test]
fn direct_paragraph_and_text_rules_match_the_legacy_bridge() {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    let sources = [
        "",
        "one paragraph",
        "first line\nsecond line",
        "first\n\nsecond",
        "  leading\n    lazy continuation\n",
        "first\r\nsecond\r\n\r\n雪 <&>\rthird  ",
    ];

    for source in sources {
        assert_direct_matches_bridge(&md, source);
    }
}

#[test]
fn direct_parser_rejects_unmigrated_syntax_rules() {
    let md = MarkdownIt::new();
    assert!(matches!(
        md.parse_document_direct("# heading"),
        Err(DocumentParseError::UnsupportedConfiguration)
    ));

    let mut partial = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut partial);
    markdown_it::plugins::cmark::block::hr::add(&mut partial);
    assert!(matches!(
        partial.parse_document_direct("---"),
        Err(DocumentParseError::UnsupportedConfiguration)
    ));

    let mut partial_inline = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut partial_inline);
    markdown_it::plugins::cmark::inline::emphasis::add(&mut partial_inline);
    assert!(matches!(
        partial_inline.parse_document_direct("soft\nbreak"),
        Err(DocumentParseError::UnsupportedConfiguration)
    ));
}

#[test]
fn direct_entities_preserve_output_and_source_maps() {
    for paragraph in [false, true] {
        let mut md = MarkdownIt::empty();
        if paragraph {
            markdown_it::plugins::cmark::block::paragraph::add(&mut md);
        }
        markdown_it::plugins::cmark::inline::entity::add(&mut md);
        markdown_it::plugins::cmark::inline::escape::add(&mut md);

        for source in [
            "",
            "&amp;",
            "before &copy; after",
            "&AElig;&NotEqualTilde;",
            "&#0; &#32; &#9;",
            "&#x41; &#X1F雪; &#x1F4A9;",
            "&#xD800; &#x110000; &#9999999; &#xFFFFFFFF;",
            "&unknown; &amp &; &#; &#x;",
            "&amp;&amp;",
            "\\&amp; &amp;",
            "雪&amp;雨\n&#x96EA;",
        ] {
            assert_direct_matches_bridge(&md, source);
        }
    }
}

#[test]
fn direct_breaks_and_escapes_preserve_output_and_source_maps() {
    for paragraph in [false, true] {
        for breaks in [false, true] {
            for xhtml in [false, true] {
                let mut md = MarkdownIt::empty();
                if paragraph {
                    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
                }
                markdown_it::plugins::cmark::inline::newline::add(&mut md);
                markdown_it::plugins::cmark::inline::escape::add(&mut md);
                md.render_options.breaks = breaks;
                md.render_options.xhtml_out = xhtml;
                for source in [
                    "",
                    "a\nb",
                    "a \nb",
                    "a  \nb",
                    "a    \n  b",
                    "a\\\nb",
                    "a\\  \nb",
                    "a\\\\\nb",
                    "a\\*b\\雪",
                    "雪\r\n次  \r\n行",
                    "  雪  \n\t次",
                    "a\nb  \nc",
                    "first\n\nsecond  \nthird",
                    "\\\nnext",
                    "\\",
                    "end  ",
                ] {
                    assert_direct_matches_bridge(&md, source);
                }
            }
        }
    }
}

#[test]
fn direct_zero_nesting_matches_legacy() {
    let mut md = MarkdownIt::empty();
    md.max_nesting = 0;
    assert_direct_matches_bridge(&md, "text\n");
}

#[test]
fn trailing_space_removal_maps_inline_offsets_once() {
    use markdown_it::parser::inline::Text;
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    markdown_it::plugins::cmark::inline::newline::add(&mut md);
    let source = "雪\r\n次  \r\n行";
    for document in [
        md.parse_document(source),
        md.parse_document_direct(source).unwrap(),
    ] {
        let spans: Vec<_> = document
            .events(document.root())
            .unwrap()
            .filter_map(|event| {
                let node = event.node();
                node.cast::<Text>().map(|text| {
                    (
                        text.content.clone(),
                        node.srcmap().unwrap().get_byte_offsets(),
                    )
                })
            })
            .collect();
        assert_eq!(
            spans,
            vec![
                ("雪".into(), (0, 3)),
                ("次".into(), (5, 8)),
                ("行".into(), (12, 15))
            ]
        );
    }
}
