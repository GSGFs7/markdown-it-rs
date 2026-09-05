use markdown_it::parser::core::Root;
use markdown_it::{DocumentParseError, MarkdownIt, NodeDraft, StructuralEvent};

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
fn direct_code_spans_preserve_output_structure_and_source_maps() {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    markdown_it::plugins::cmark::inline::backticks::add(&mut md);

    for source in [
        "`foo`",
        "before `` foo ` bar `` after",
        "` `` ` and `  ``  `",
        "` a` `b ` `   `",
        "`\u{a0}雪\u{a0}`",
        "``\nfoo\nbar  \nbaz\n``",
        "``\nfoo \n``",
        "`foo   bar \nbaz`",
        "`foo\\`bar`",
        "```foo``",
        "雪 `代码` 雨",
    ] {
        assert_direct_matches_bridge(&md, source);
    }

    let document = md.parse_document_direct("foo ```bar``` baz").unwrap();
    let mut spans = document
        .events(document.root())
        .unwrap()
        .filter_map(|event| {
            if matches!(event, StructuralEvent::Exit(_)) {
                return None;
            }
            let node = event.node();
            if node.is::<markdown_it::plugins::cmark::inline::backticks::CodeInline>()
                || node.is::<markdown_it::parser::inline::Text>()
            {
                Some((node.name(), node.srcmap().unwrap().get_byte_offsets()))
            } else {
                None
            }
        });
    assert_eq!(spans.next().unwrap().1, (0, 4));
    assert_eq!(spans.next().unwrap().1, (4, 13));
    assert_eq!(spans.next().unwrap().1, (7, 10));
    assert_eq!(spans.next().unwrap().1, (13, 17));
    assert!(spans.next().is_none());
}

#[test]
fn code_pair_factory_uses_drafts_and_supports_unicode_markers() {
    use markdown_it::{Node, NodeValue, Renderer};

    #[derive(Debug)]
    struct CustomCode(usize);

    impl NodeValue for CustomCode {
        fn render(&self, node: &Node, renderer: &mut dyn Renderer) {
            renderer.text(&format!("{}:", self.0));
            renderer.contents(&node.children);
        }
    }

    let mut md = MarkdownIt::empty();
    markdown_it::generics::inline::code_pair::add_with::<'🦀'>(&mut md, |len| {
        NodeDraft::new(CustomCode(len))
    });
    let source = "a 🦀 雪 🦀 b 🦀🦀x🦀🦀";
    let direct = md.parse_document_direct(source).unwrap();
    assert_eq!(direct.into_legacy().render(), "a 1:雪 b 2:x\n");
    assert_eq!(md.parse(source).render(), "a 1:雪 b 2:x\n");
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

#[test]
fn direct_html_inline_and_autolinks_match_the_legacy_bridge() {
    let mut md = MarkdownIt::empty();

    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    markdown_it::plugins::cmark::inline::autolink::add(&mut md);
    markdown_it::plugins::html::html_inline::add(&mut md);

    for source in [
        "<https://example.com>",
        "<foo@example.com>",
        "<a href='target'>text</a>",
        "<br>",
        "<br />",
        "<!-- comment -->",
        "<!-- internal -- hyphens -->",
        "<!--> short",
        "<!---> short",
        "<?processing instruction?>",
        "<!DOCTYPE html>",
        "<![CDATA[<tag>]]>",
        "<3",
        "<invalid attribute=>",
        "<!-- unclosed",
        "before <em>雪</em> after",
        "before <!-- multi\nline --> after",
        "<a>inside <https://example.com></a>",
    ] {
        assert_direct_matches_bridge(&md, source);
    }
}

#[test]
fn direct_html_inline_caches_unclosed_comments() {
    let mut md = MarkdownIt::empty();

    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    markdown_it::plugins::cmark::inline::autolink::add(&mut md);
    markdown_it::plugins::html::html_inline::add(&mut md);

    let source = format!("{} tail", "<!--".repeat(8192));
    assert_direct_matches_bridge(&md, &source);
}
