use markdown_it::parser::core::Root;
use markdown_it::{MarkdownIt, NodeDraft, StructuralEvent};

fn assert_direct_matches_bridge(md: &MarkdownIt, source: &str) {
    let bridged = md.parse_document(source);
    let direct = md.parse_document_direct(source);

    assert_eq!(direct.source(), source);
    assert_eq!(direct.len(), bridged.len(), "node count for {source:?}");
    assert_eq!(
        direct.node(direct.root()).cast::<Root>().unwrap().content,
        source
    );
    assert_eq!(
        md.render_document(&direct),
        md.render_document(&bridged),
        "HTML for {source:?}"
    );
    assert_eq!(
        md.render_document_as(&direct, "text"),
        md.render_document_as(&bridged, "text"),
        "text for {source:?}"
    );
    assert_eq!(
        md.render_document_as(&direct, "debug"),
        md.render_document_as(&bridged, "debug"),
        "debug tree for {source:?}"
    );
    assert_eq!(
        direct.into_legacy().render(),
        md.parse(source).render(),
        "legacy conversion for {source:?}"
    );
}

fn assert_direct_configuration_panics(md: &MarkdownIt, source: &str, expected: &str) {
    let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        md.parse_document_direct(source);
    }))
    .expect_err("unsupported direct configuration must panic");
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .expect("configuration panic must carry a string message");
    assert!(message.contains(expected), "{message}");
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
    for source in ["# heading", ""] {
        assert_direct_configuration_panics(&md, source, "unsupported direct block rules");
    }

    let mut partial = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut partial);
    markdown_it::plugins::extra::front_matter::add(&mut partial);
    for source in ["---\ntitle: test\n---", ""] {
        assert_direct_configuration_panics(&partial, source, "unsupported direct block rules");
    }
    partial.max_nesting = 0;
    assert_direct_configuration_panics(&partial, "---", "unsupported direct block rules");

    let mut partial_inline = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut partial_inline);
    markdown_it::generics::inline::full_link::add_prefix::<'~', true>(
        &mut partial_inline,
        |_, _| {
            markdown_it::Node::new(markdown_it::parser::inline::Text {
                content: "custom".into(),
            })
        },
    );
    for source in ["~[x](/url)", ""] {
        assert_direct_configuration_panics(
            &partial_inline,
            source,
            "unsupported direct inline rules or factories",
        );
    }
    partial_inline.max_nesting = 0;
    assert_direct_configuration_panics(
        &partial_inline,
        "~[x](/url)",
        "unsupported direct inline rules or factories",
    );
}

#[test]
fn direct_parser_checks_core_configuration_before_parsing() {
    let mut md = MarkdownIt::empty();
    md.remove_rule::<markdown_it::parser::block::builtin::BlockParserRule>();
    md.max_nesting = 0;
    assert_direct_configuration_panics(
        &md,
        "",
        "direct parsing requires the built-in block and inline core rules only",
    );
}

#[test]
fn direct_parser_accepts_migrated_emphasis_rules() {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    markdown_it::plugins::cmark::inline::emphasis::add(&mut md);

    let direct = md.parse_document_direct("*em* and **strong**");

    assert_eq!(
        md.render_document(&direct),
        "<p><em>em</em> and <strong>strong</strong></p>\n",
    );
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

    let document = md.parse_document_direct("foo ```bar``` baz");
    let mut spans = document.events(document.root()).filter_map(|event| {
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
    let direct = md.parse_document_direct(source);
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
    for document in [md.parse_document(source), md.parse_document_direct(source)] {
        let spans: Vec<_> = document
            .events(document.root())
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
fn direct_html_blocks_match_the_legacy_bridge() {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::html::add(&mut md);

    for max_indent in [4, i32::MAX] {
        md.max_indent = max_indent;
        for source in [
            "",
            "<script>\n雪\n\n雨\n</script>\nafter",
            "<PRE>雪</PRE>\nafter",
            "<style>\nbody {}\n</style>",
            "<textarea>\n*literal*\n</textarea>",
            "<!-- 雪\n\n雨 -->\nafter",
            "<?processing\n\ninstruction?>\nafter",
            "<!DOCTYPE\nhtml>\nafter",
            "<![CDATA[\n\n雪 <tag>\n]]>\nafter",
            "<div>\n*literal*\n</div>\n\nafter",
            "</DIV>\n雪\n\nafter",
            "<custom attr='雪'>\n*literal*\n\nafter",
            "<custom />\n\nafter",
            "<script>",
            "<!-- unclosed\n\n雪",
            "<?unclosed",
            "<!DOCTYPE",
            "<![CDATA[unclosed",
            "<div>",
            "<custom>",
            "before\n<script>雪</script>\nafter",
            "before\n<!-- 雪 -->\nafter",
            "before\n<?instruction?>\nafter",
            "before\n<!DOCTYPE html>\nafter",
            "before\n<![CDATA[雪]]>\nafter",
            "before\n<div>\n雪\n\nafter",
            "before\n<custom>\nafter",
            "before\n\n<custom>\n雪\n\nafter",
            "<custom> trailing text\nafter",
            "<invalid attribute=>\nafter",
            "<3\nafter",
            "   <div>\n   雪\n\nafter",
            "    <div>\n    雪\n\nafter",
            "\t<div>\n\t雪",
            "<div>\r\n雪\r\n\r\nafter",
            "<!-- 雪\r\n\r\n雨 -->\rafter",
            "<!-- nul \0 -->",
            "> <script>\n> 雪\n>\n> 雨\n> </script>\n\nafter",
            "> <!-- unclosed\n> 雪\n\noutside",
            "> <div>\n> 雪\n>\n> after",
            "- <script>\n  雪\n\n  雨\n  </script>\n\nafter",
            "- <!-- unclosed\n  雪\n\noutside",
            "- <!-- unclosed\n  雪\n- sibling",
            "- <div>\n  雪\n\n  after",
            "> - <!-- 雪\n>\n>     雨 -->\n\noutside",
        ] {
            assert_direct_matches_bridge(&md, source);

            let bridged = md.parse_document(source);
            let direct = md.parse_document_direct(source);
            let source_maps = |document: &markdown_it::Document| {
                document
                    .events(document.root())
                    .filter_map(|event| match event {
                        StructuralEvent::Enter(node) | StructuralEvent::Leaf(node) => {
                            Some(node.srcmap())
                        }
                        StructuralEvent::Exit(_) => None,
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(source_maps(&direct), source_maps(&bridged), "{source:?}");
        }
    }

    md.max_nesting = 0;
    assert_direct_matches_bridge(&md, "<div>");
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

#[test]
fn direct_emphasis_uses_cjk_delimiter_override() {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    markdown_it::plugins::cmark::inline::emphasis::add(&mut md);
    markdown_it::plugins::cjk_friendly::add(&mut md);

    assert_direct_matches_bridge(&md, "**这是重要内容。**后面继续写");
}

#[test]
fn direct_nested_emphasis_preserves_source_maps() {
    use markdown_it::parser::inline::Text;
    use markdown_it::plugins::cmark::inline::emphasis::{Em, Strong};

    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    markdown_it::plugins::cmark::inline::emphasis::add(&mut md);

    let document = md.parse_document_direct("***foo***");

    let spans: Vec<_> = document
        .events(document.root())
        .filter(|event| !matches!(event, StructuralEvent::Exit(_)))
        .filter_map(|event| {
            let node = event.node();

            if node.is::<Em>() {
                Some(("em", node.srcmap()?.get_byte_offsets()))
            } else if node.is::<Strong>() {
                Some(("strong", node.srcmap()?.get_byte_offsets()))
            } else if node.is::<Text>() {
                Some(("text", node.srcmap()?.get_byte_offsets()))
            } else {
                None
            }
        })
        .collect();

    assert_eq!(
        spans,
        vec![("em", (0, 9)), ("strong", (1, 8)), ("text", (3, 6)),],
    );
}

#[test]
fn direct_emphasis_handles_many_unmatched_delimiters() {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    markdown_it::plugins::cmark::inline::emphasis::add(&mut md);

    for source in ["a_ ".repeat(8_192), "_a ".repeat(8_192)] {
        assert_direct_matches_bridge(&md, &source);
    }
}

#[test]
fn direct_links_use_real_registration() {
    use markdown_it::plugins::cmark::{block, inline};

    let mut md = MarkdownIt::empty();
    block::paragraph::add(&mut md);
    inline::newline::add(&mut md);
    inline::escape::add(&mut md);
    inline::backticks::add(&mut md);
    inline::emphasis::add(&mut md);
    inline::entity::add(&mut md);
    inline::autolink::add(&mut md);
    inline::link::add(&mut md);

    for source in [
        "[x](/url)",
        "[雪](</url> \"title\")",
        "[x]()",
        "[ *x* ](/url)",
        "[`]`](/url)",
        r"[\]](/url)",
        "[&#93;](/url)",
        "[outer [inner](/in)](/out)",
        "[<https://example.com>](/out)",
        "[x](javascript:alert(1))",
        "[x](/unclosed",
        "[x][id] [x][] [x]",
    ] {
        assert_direct_matches_bridge(&md, source);
    }
    let document = md.parse_document_direct("[x](/url)");
    assert_eq!(
        md.render_document(&document),
        "<p><a href=\"/url\">x</a></p>\n",
    );

    // A current-position probe must not spend an extra child depth.
    md.max_nesting = 2;
    assert_direct_matches_bridge(&md, "[x](/url)");

    // Reference definitions are part of the direct pipeline.
    block::reference::add(&mut md);
    assert_direct_matches_bridge(&md, "[x](/url)");
}

#[test]
fn direct_references_match_the_legacy_bridge() {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut md);

    for source in [
        "[x]: /url",
        "[x]: /url\n\n[x]",
        "[x][id]\n\n[id]: /url \"title\"",
        "[id]:\n  /url\n  'title'\n\n[x][id]",
        "[id]: /url\n\n[id]\n\n[id]: /other",
        "[missing]",
        "[x]: <>\n\n[x]",
        "[x]: /url 'title' trailing garbage\n\n[x]",
        "[x]: /url\n\n[x][x]",
    ] {
        assert_direct_matches_bridge(&md, source);
    }

    let document = md.parse_document_direct("[id]: /url\n\n[id]\n\n[id]: /other");
    assert_eq!(
        md.render_document(&document),
        "<p><a href=\"/url\">id</a></p>\n",
    );
}

#[test]
fn direct_forward_references_match_the_legacy_bridge() {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut md);

    for source in [
        "[x]\n\n[x]: /url",
        "[x][id]\n\n[id]: /url \"title\"",
        "![alt][id]\n\n[id]: /image \"title\"",
        "[雪]\n\n[雪]: /url",
        "- [x][id]\n\n[id]: /url",
        "> [x][id]\n\n[id]: /url",
        "[outer [inner][id]](/out)\n\n[id]: /in",
        "[![alt][id]](/out)\n\n[id]: /image",
    ] {
        assert_direct_matches_bridge(&md, source);
    }
}

#[test]
fn deferred_inline_fallback_preserves_sibling_order() {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::reference::add(&mut md);
    markdown_it::plugins::cmark::inline::link::add(&mut md);
    markdown_it::plugins::cmark::inline::emphasis::add(&mut md);
    markdown_it::plugins::cmark::inline::newline::add(&mut md);
    let source = format!("{}\n[id]: /url", "before *em* [id] after\n".repeat(512));
    assert_direct_matches_bridge(&md, &source);
}

#[test]
fn direct_images_and_links_match_legacy_in_both_registration_orders() {
    use markdown_it::plugins::cmark::{block, inline};

    for image_first in [false, true] {
        let mut md = MarkdownIt::empty();
        block::paragraph::add(&mut md);
        inline::newline::add(&mut md);
        inline::escape::add(&mut md);
        inline::backticks::add(&mut md);
        inline::emphasis::add(&mut md);
        inline::entity::add(&mut md);
        inline::autolink::add(&mut md);
        markdown_it::plugins::html::html_inline::add(&mut md);
        if image_first {
            inline::image::add(&mut md);
            inline::link::add(&mut md);
        } else {
            inline::link::add(&mut md);
            inline::image::add(&mut md);
        }
        for source in [
            "![雪](/img \"title\")",
            "![]()",
            "![ *x* &amp; `y` ](/img)",
            "![a\nb](/img)",
            "![a  \nb](/img)",
            r"![\]](/img)",
            "![`]`](/img)",
            "![x [inner](/in)](/img)",
            "[![alt](/img)](/out)",
            "![![alt](/inner)](/outer)",
            "[outer ![x [inner](/in)](/img)](/out)",
            "![<i>x</i>](/img)",
            "<a href='/a'>[x](/b) ![y](/img)</a>",
            "![x](javascript:alert(1))",
            "![x](/unclosed",
            "![x][id] ![x][] ![x]",
        ] {
            assert_direct_matches_bridge(&md, source);
        }
        let document = md.parse_document_direct("![x](/img)");
        assert_eq!(
            md.render_document(&document),
            "<p><img src=\"/img\" alt=\"x\"></p>\n"
        );
        md.max_nesting = 2;
        assert_direct_matches_bridge(&md, "![x](/img)");
    }
}

#[test]
fn direct_image_only_registration_matches_legacy() {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    markdown_it::plugins::cmark::inline::image::add(&mut md);
    assert_direct_matches_bridge(&md, "![x](/img)");
}

#[test]
fn direct_link_and_image_nesting_thresholds_are_explicit() {
    use markdown_it::plugins::cmark::{block, inline};

    let mut md = MarkdownIt::empty();
    block::paragraph::add(&mut md);
    inline::link::add(&mut md);
    inline::image::add(&mut md);

    let cases = [
        (
            "[x](/url)",
            "<p>[x](/url)</p>\n",
            "<p><a href=\"/url\">x</a></p>\n",
        ),
        (
            "[](/url)",
            "<p>[](/url)</p>\n",
            "<p><a href=\"/url\"></a></p>\n",
        ),
        (
            "![雪](/img)",
            "<p>![雪](/img)</p>\n",
            "<p><img src=\"/img\" alt=\"雪\"></p>\n",
        ),
        (
            "![](/img)",
            "<p>![](/img)</p>\n",
            "<p><img src=\"/img\" alt=\"\"></p>\n",
        ),
    ];
    for limit in [0, 1, 2, 32] {
        md.max_nesting = limit;
        for (source, literal, parsed) in cases {
            let document = md.parse_document_direct(source);
            let expected = match limit {
                0 => "",
                1 => literal,
                _ => parsed,
            };
            assert_eq!(
                md.render_document(&document),
                expected,
                "limit={limit}, source={source:?}",
            );
        }
    }
}

#[test]
fn direct_unclosed_link_labels_remain_literal() {
    use markdown_it::plugins::cmark::{block, inline};

    let mut md = MarkdownIt::empty();
    block::paragraph::add(&mut md);
    inline::link::add(&mut md);
    inline::image::add(&mut md);

    for size in [64, 1024, 4096] {
        for part in ["[", "![", "雪["] {
            let source = part.repeat(size);
            let document = md.parse_document_direct(&source);
            assert_eq!(
                md.render_document(&document),
                format!("<p>{source}</p>\n"),
                "size={size}, part={part:?}",
            );
        }
    }

    // A closer anywhere in the remaining range must keep normal scanning.
    for source in [
        "[[x](/url)",
        "![[x](/url)",
        "![雪](/img)",
        "[](/url)",
        "[x](/unfinished",
        "![x](/unfinished",
    ] {
        assert_direct_matches_bridge(&md, source);
    }
}

#[test]
fn direct_thematic_breaks_match_the_legacy_bridge() {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    markdown_it::plugins::cmark::block::hr::add(&mut md);

    for source in [
        "",
        "---",
        "***",
        "___",
        " - - -",
        "****",
        "a\n\n---\n\nb",
        "a\n---\nb",
        "---\n---",
        "  ---  ",
        "    ---",
        "*-*",
        "---x",
        "a\n***\nb",
    ] {
        assert_direct_matches_bridge(&md, source);
    }

    let document = md.parse_document_direct("a\n---\nb");
    assert_eq!(md.render_document(&document), "<p>a</p>\n<hr>\n<p>b</p>\n",);
}

#[test]
fn direct_atx_headings_match_the_legacy_bridge() {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    markdown_it::plugins::cmark::block::heading::add(&mut md);
    markdown_it::plugins::cmark::inline::emphasis::add(&mut md);

    for source in [
        "",
        "# foo",
        "## foo ##",
        "### foo ###",
        "####### foo",
        "#5 bolt",
        "#hashtag",
        "#",
        "### ###",
        "  ## indented",
        "    # code",
        "# foo *bar*",
        "# foo\nbar",
        "a\n# b",
        "雪 # 标题 雪",
    ] {
        assert_direct_matches_bridge(&md, source);
    }

    let document = md.parse_document_direct("## *雪* 标题");
    assert_eq!(md.render_document(&document), "<h2><em>雪</em> 标题</h2>\n",);
}

#[test]
fn direct_setext_headings_match_the_legacy_bridge() {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    markdown_it::plugins::cmark::block::lheading::add(&mut md);
    markdown_it::plugins::cmark::inline::emphasis::add(&mut md);

    for source in [
        "",
        "foo\n===",
        "foo\n---",
        "foo\nbar\n===",
        "foo *bar*\n===",
        "  foo\n  ===",
        "foo\n   ===",
        "foo\n=",
        "foo\n- -",
        "foo\n===\nbar",
        "foo\n---\n---",
        "雪\n===",
    ] {
        assert_direct_matches_bridge(&md, source);
    }

    let document = md.parse_document_direct("foo *bar*\n===");
    assert_eq!(md.render_document(&document), "<h1>foo <em>bar</em></h1>\n");
}

#[test]
fn direct_indented_code_blocks_match_the_legacy_bridge() {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    markdown_it::plugins::cmark::block::code::add(&mut md);

    for source in [
        "",
        "    foo",
        "    foo\n    bar",
        "    foo\n\n    bar",
        "    foo\nbar",
        "foo\n    bar",
        "    雪",
        "    <html>",
        "    a\n      b",
        "  \tfoo",
    ] {
        assert_direct_matches_bridge(&md, source);
    }

    let document = md.parse_document_direct("    foo\n    bar");
    assert_eq!(
        md.render_document(&document),
        "<pre><code>foo\nbar\n</code></pre>\n",
    );
}

#[test]
fn direct_code_fences_match_the_legacy_bridge() {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    markdown_it::plugins::cmark::block::code::add(&mut md);
    markdown_it::plugins::cmark::block::fence::add(&mut md);

    for source in [
        "",
        "```",
        "```\nfoo\n```",
        "~~~\nfoo\n~~~",
        "```rust\nlet x = 1;\n```",
        "  ```\n  foo\n  ```",
        "```\nfoo",
        "````\n```\n````",
        "```\nfoo\n~~~~",
        "```foo`bar```",
        "~~~ foo\nbar\n~~~",
        "a\n```\nb\n```",
        "雪\n```\n雨\n```",
        "```\nfoo\n\nbar\n```",
    ] {
        assert_direct_matches_bridge(&md, source);
    }

    let document = md.parse_document_direct("```rust\nlet x = 1;\n```");
    assert_eq!(
        md.render_document(&document),
        "<pre><code class=\"language-rust\">let x = 1;\n</code></pre>\n",
    );
}

#[test]
fn direct_blockquotes_match_the_legacy_bridge() {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    markdown_it::plugins::cmark::block::heading::add(&mut md);
    markdown_it::plugins::cmark::block::blockquote::add(&mut md);
    markdown_it::plugins::cmark::inline::emphasis::add(&mut md);

    for source in [
        "",
        "> foo",
        "> foo\n> bar",
        "> foo\n\n> bar",
        "> foo\n\nbar",
        "> # heading",
        "> *em*",
        "> foo\n> > nested",
        ">",
        ">\n> foo",
        "> foo\nbar",
        "> foo\n- bar",
        "a\n> b",
        "雪\n> 雨",
    ] {
        assert_direct_matches_bridge(&md, source);
    }

    let document = md.parse_document_direct("> *雪*");
    assert_eq!(
        md.render_document(&document),
        "<blockquote>\n<p><em>雪</em></p>\n</blockquote>\n",
    );
}

#[test]
fn direct_lists_match_the_legacy_bridge() {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    markdown_it::plugins::cmark::block::heading::add(&mut md);
    markdown_it::plugins::cmark::block::fence::add(&mut md);
    markdown_it::plugins::cmark::block::hr::add(&mut md);
    markdown_it::plugins::cmark::block::list::add(&mut md);
    markdown_it::plugins::cmark::inline::emphasis::add(&mut md);

    for source in [
        "",
        "- foo",
        "- foo\n- bar",
        "* foo\n+ bar",
        "1. foo\n2. bar",
        "1. foo\n1. bar",
        "3. foo\n4. bar",
        "- foo\n\n- bar",
        "- foo\n  - bar\n    - baz",
        "- foo\n\n  bar",
        "1. foo\n\n   bar",
        "- a\n- b\n\n- c",
        "-\n- foo",
        "- foo\n1. bar",
        "para\n- foo",
        "- foo\n---",
        "- foo\n\n---",
        "1. foo\n   ```\n   code\n   ```",
        "> - nested",
        "雪\n- 雨",
    ] {
        assert_direct_matches_bridge(&md, source);
    }

    let document = md.parse_document_direct("- foo\n- bar");
    assert_eq!(
        md.render_document(&document),
        "<ul>\n<li>foo</li>\n<li>bar</li>\n</ul>\n",
    );

    let document = md.parse_document_direct("- foo\n\n- bar");
    assert_eq!(
        md.render_document(&document),
        "<ul>\n<li>\n<p>foo</p>\n</li>\n<li>\n<p>bar</p>\n</li>\n</ul>\n",
    );

    let document = md.parse_document_direct("3. foo");
    assert_eq!(
        md.render_document(&document),
        "<ol start=\"3\">\n<li>foo</li>\n</ol>\n",
    );
}

#[test]
fn direct_emph_pair_extras_match_the_legacy_bridge() {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::extra::mark::add(&mut md);
    markdown_it::plugins::extra::strikethrough::add(&mut md);

    for source in [
        "==highlighted==",
        "==**bold** highlight==",
        "==one== and ==two==",
        "==mark ~~strike~~==",
        "before ==雪== after",
        "==unclosed",
    ] {
        assert_direct_matches_bridge(&md, source);
    }

    let document = md.parse_document_direct("==highlighted==");
    assert_eq!(
        md.render_document(&document),
        "<p><mark>highlighted</mark></p>\n",
    );
}
