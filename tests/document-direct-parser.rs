use markdown_it::parser::core::Root;
use markdown_it::{MarkdownIt, StructuralEvent};

// KaTeX uses unordered attribute/style maps; normalize only their ordering.
fn normalize_math_html(html: &str) -> String {
    #[cfg(not(feature = "katex"))]
    {
        html.to_owned()
    }
    #[cfg(feature = "katex")]
    {
        let styles = regex::Regex::new(r#"style="([^"]+)""#).unwrap();
        let html = styles.replace_all(html, |caps: &regex::Captures| {
            let mut values: Vec<_> = caps[1]
                .split(';')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            values.sort();
            format!(r#"style="{};""#, values.join("; "))
        });
        let math = regex::Regex::new(r#"<math ([^>]+)>"#).unwrap();
        math.replace_all(&html, |caps: &regex::Captures| {
            let mut attrs: Vec<_> = caps[1].split_whitespace().collect();
            attrs.sort();
            format!("<math {}>", attrs.join(" "))
        })
        .into_owned()
    }
}

fn fingerprint(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

fn snapshot_config(md: &MarkdownIt, config: &str) -> String {
    format!(
        "{config};nesting={};indent={};options={:?}",
        md.max_nesting, md.max_indent, md.render_options
    )
}

fn snapshot_outputs(md: &MarkdownIt, document: &markdown_it::Document) -> [String; 4] {
    let metadata = document
        .node(document.root())
        .cast::<Root>()
        .unwrap()
        .ext
        .get::<markdown_it::plugins::extra::front_matter::FrontMatter>()
        .map(|value| (value.kind, &value.raw, value.start_line, value.end_line));
    [
        normalize_math_html(&md.render_document(document)),
        md.render_document_as(document, "text"),
        md.render_document_as(document, "debug"),
        format!("{metadata:?}"),
    ]
}
fn assert_document_structure(md: &MarkdownIt, source: &str) -> markdown_it::Document {
    let document = md.parse_document(source);
    assert_eq!(document.source(), source);
    assert_eq!(
        document
            .node(document.root())
            .cast::<Root>()
            .unwrap()
            .content
            .as_ref(),
        source
    );
    assert!(std::ptr::eq(
        document
            .node(document.root())
            .cast::<Root>()
            .unwrap()
            .content
            .as_ref(),
        document.source(),
    ));
    let mut stack = Vec::new();
    let mut visited = std::collections::HashSet::new();
    for event in document.events(document.root()) {
        let node = event.node();
        match event {
            StructuralEvent::Enter(_) => {
                assert_eq!(node.parent(), stack.last().copied());
                assert!(visited.insert(node.id()));
                stack.push(node.id());
            }
            StructuralEvent::Leaf(_) => {
                assert_eq!(node.parent(), stack.last().copied());
                assert!(visited.insert(node.id()));
            }
            StructuralEvent::Exit(_) => {
                assert_eq!(stack.pop(), Some(node.id()));
            }
        }
    }
    assert!(stack.is_empty());
    assert_eq!(visited.len(), document.len());
    document
}

type Snapshot = (&'static str, &'static str, u64, u8, (u64, u64, u64, u64));

fn assert_document_valid(md: &MarkdownIt, source: &str) -> markdown_it::Document {
    assert_document_valid_with_config(md, source, "default")
}

fn assert_document_valid_with_config(
    md: &MarkdownIt,
    source: &str,
    config: &str,
) -> markdown_it::Document {
    let document = assert_document_structure(md, source);
    let snapshots: &[Snapshot] = include!("fixtures/document-parser-snapshots.rs");
    let thread = std::thread::current();
    let test = thread.name().unwrap();
    let config = snapshot_config(md, config);
    let mut candidates = snapshots.iter().filter(|row| {
        row.0 == test
            && row.1 == config
            && row.2 == fingerprint(source)
            && (row.3 == 2 || row.3 == u8::from(cfg!(feature = "katex")))
    });
    let expected = candidates.next().unwrap_or_else(|| {
        panic!("missing pre-migration snapshot for {test}, config={config}, source={source:?}")
    });
    assert!(
        candidates.next().is_none(),
        "ambiguous snapshot for {test}, config={config}, source={source:?}"
    );
    let hashes = [expected.4.0, expected.4.1, expected.4.2, expected.4.3];
    for ((format, actual), expected_hash) in ["HTML", "text", "debug", "front matter"]
        .into_iter()
        .zip(snapshot_outputs(md, &document))
        .zip(hashes)
    {
        assert_eq!(
            fingerprint(&actual),
            expected_hash,
            "pre-migration {format} mismatch for {test}, config={config}, source={source:?}, actual={actual:?}"
        );
    }
    document
}

#[test]
fn snapshot_keys_identify_a_single_configuration() {
    let snapshots: &[Snapshot] = include!("fixtures/document-parser-snapshots.rs");
    let mut seen = std::collections::HashSet::new();
    for row in snapshots {
        assert!(row.3 <= 2, "invalid KaTeX mode: {}", row.3);
        for katex in [0, 1] {
            if row.3 == 2 || row.3 == katex {
                assert!(
                    seen.insert((row.0, row.1, row.2, katex)),
                    "ambiguous snapshot: test={}, config={}, source={:x}, katex={katex}",
                    row.0,
                    row.1,
                    row.2
                );
            }
        }
    }
}

fn assert_direct_configuration_panics(md: &MarkdownIt, source: &str, expected: &str) {
    let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        md.parse_document(source);
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
fn direct_text_fallback_matches_snapshots() {
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
        assert_document_valid(&md, source);
    }
}

#[test]
fn direct_paragraph_and_text_rules_match_snapshots() {
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
        assert_document_valid(&md, source);
    }
}

#[test]
fn direct_front_matter_matches_snapshots() {
    use markdown_it::plugins::extra::front_matter::{self, FrontMatter, FrontMatterKind};

    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut md);
    front_matter::add_with_max_lines(&mut md, 3);

    for max_lines in [0, 1, 2, 3, 4, front_matter::DEFAULT_MAX_LINES] {
        front_matter::set_max_lines(&mut md, max_lines);
        for source in [
            "",
            "---",
            "+++",
            "---\n---",
            "+++\n+++\nBody",
            "---\ntitle: 雪\n---\n# Post",
            "+++\ntitle = '雪'\n+++\nBody",
            "---\ntitle: 雪\ntags:\n  - rust\n---\n# Post\n",
            "---\n\n---\nBody",
            "--- \t\ntitle: 雪\n--- \t\nBody",
            "---\r\ntitle: 雪\r\n---\r\nBody",
            "+++\rtitle = '雪'\r+++\rBody",
            "---\ntitle: \0\n---\nBody",
            "---\nunclosed",
            "---\ntitle: 雪\n+++\nBody",
            "---\ntitle: 雪\n  ---\nBody",
            "  ---\ntitle: 雪\n---\nBody",
            "\n---\ntitle: 雪\n---\nBody",
            "before\n\n---\ntitle: 雪\n---\nBody",
            "> ---\n> title: 雪\n> ---\n\nBody",
            "- ---\n  title: 雪\n  ---\n\nBody",
            "---\ntitle: 雪\n---\n\n[ref]: /url\n\n[ref]",
        ] {
            assert_document_valid_with_config(&md, source, &format!("max_lines={max_lines}"));
        }
    }

    let source = "---\ntitle: 雪\n---\n# Post";
    let document = md.parse_document(source);
    let root = document.node(document.root());
    let value = root
        .cast::<Root>()
        .unwrap()
        .ext
        .get::<FrontMatter>()
        .unwrap();
    assert_eq!(value.kind, FrontMatterKind::Yaml);
    assert_eq!(value.raw, "title: 雪");
    assert_eq!((value.start_line, value.end_line), (0, 2));
    assert_eq!(
        value.parse_with(|_, raw| raw
            .strip_prefix("title: ")
            .map(str::to_owned)
            .ok_or("missing title")),
        Ok("雪".to_owned())
    );
    assert_eq!(md.render_document(&document), "<h1>Post</h1>\n");

    // The closing delimiter must be strictly inside the scan limit.
    for end_line in [255, 256] {
        let source = format!("---\n{}---\nBody", "key: value\n".repeat(end_line - 1));
        assert_document_valid(&md, &source);
        let document = md.parse_document(&source);
        assert_eq!(
            document
                .node(document.root())
                .cast::<Root>()
                .unwrap()
                .ext
                .get::<FrontMatter>()
                .is_some(),
            end_line < front_matter::DEFAULT_MAX_LINES,
        );
    }

    // Registering front matter first also keeps it ahead of the thematic break.
    let mut front_first = MarkdownIt::empty();
    front_matter::add(&mut front_first);
    markdown_it::plugins::cmark::add(&mut front_first);
    assert_document_valid_with_config(&front_first, source, "front_matter_first");
    assert_eq!(
        front_first.render_document(&front_first.parse_document(source)),
        "<h1>Post</h1>\n"
    );

    md.max_nesting = 0;
    assert_document_valid_with_config(&md, source, "max_lines=256");
}

#[test]
fn direct_tables_match_snapshots() {
    let mut md = MarkdownIt::new();
    markdown_it::plugins::html::add(&mut md);

    for max_indent in [4, i32::MAX] {
        md.max_indent = max_indent;
        for source in [
            "",
            "| a | b |\n| - | - |",
            "a | b\n:- | -:\nx | y",
            "| a | b | c | d |\n| :- | -: | :-: | - |\n| 雪 | 雨 | 风 | 云 |",
            "| a | b |\n| - | - |\nx\nx | y | ignored\n||",
            "foo\n---\nbar",
            "|\n| - |",
            "| a | b |\n| - |",
            "a | b\n- - | -\nx | y",
            "a | b\n: | -\nx | y",
            "a | b\ninvalid | -\nx | y",
            "before\n| a | b |\n| - | - |\nx | y",
            "| a | b |\n| - | - |\n\nafter",
            "| a | b |\n| - | - |\nx | y\n# heading",
            "| a | b |\n| - | - |\nx | y\n- item",
            "| a | b |\n| - | - |\nx | y\n> quote",
            "| a | b |\n| - | - |\nx | y\n---",
            "| a | b |\n| - | - |\nx | y\n```\ncode\n```",
            "| a | b |\n| - | - |\nx | y\n<div>\nraw",
            "| a | b |\n| - | - |\nx | y\nc | d\n- | -\nz | w",
            "| *雪* | ~~雨~~ |\n| - | - |\n| `x` | [link](/url) |",
            "| [forward] | ![雪](/image) |\n| - | - |\n\n[forward]: /url",
            "| a &amp; b | <em>雪</em> |\n| - | - |\n| &#x96EA; | <https://example.com> |",
            "| a\\|b | c |\n| - | - |\n| `x\\|y` | 雪\\|雨 |",
            "| a\\\\|b | c |\n| - | - |",
            "|  雪  |\t雨\t|\n| - | - |\n| x | |",
            "| a | b |\r\n| - | - |\r\nx | y\r\n",
            "| a | b |\r| - | - |\rx | y",
            "| 雪\0 | 雨 |\n| - | - |",
            "   a | b\n   - | -\n   x | y",
            "    a | b\n    - | -\n    x | y",
            "a | b\n    - | -\nx | y",
            "a | b\n- | -\n    x | y",
            "> a | b\n> - | -\n> 雪 | 雨\n\noutside",
            "- a | b\n  - | -\n  雪 | 雨\n- sibling",
            "> - a | b\n>   - | -\n>   雪 | 雨\n\noutside",
        ] {
            assert_document_valid(&md, source);
        }
    }

    md.max_nesting = 0;
    assert_document_valid(&md, "a | b\n- | -\nx | y");
}

#[test]
fn direct_parser_checks_core_configuration_before_parsing() {
    let mut md = MarkdownIt::empty();
    md.remove_rule::<markdown_it::parser::block::builtin::BlockParserRule>();
    md.max_nesting = 0;
    assert_direct_configuration_panics(
        &md,
        "",
        "direct parsing requires exactly one block stage followed by exactly one inline stage",
    );
}

#[test]
fn direct_parser_rejects_invalid_core_stages_before_callbacks() {
    use markdown_it::parser::block::builtin::BlockParserRule;
    use markdown_it::parser::core::{CoreRule, DocumentCoreRule};
    use markdown_it::parser::inline::builtin::InlineParserRule;

    struct ExtraStage<const INLINE: bool>;
    impl<const INLINE: bool> CoreRule for ExtraStage<INLINE> {
        fn document_rule() -> DocumentCoreRule {
            if INLINE {
                DocumentCoreRule::ParseInlines
            } else {
                DocumentCoreRule::ParseBlocks
            }
        }
    }

    struct EarlyFinalizer;
    impl CoreRule for EarlyFinalizer {
        fn document_rule() -> DocumentCoreRule {
            DocumentCoreRule::FinalizeDraft(|_, _| {
                panic!("invalid configuration must be rejected before finalizing");
            })
        }
    }

    let mut missing_block = MarkdownIt::new();
    missing_block.remove_rule::<BlockParserRule>();
    let mut missing_inline = MarkdownIt::new();
    missing_inline.remove_rule::<InlineParserRule>();
    let mut duplicate_block = MarkdownIt::new();
    duplicate_block
        .add_rule::<ExtraStage<false>>()
        .after::<BlockParserRule>()
        .before::<InlineParserRule>();
    let mut duplicate_inline = MarkdownIt::new();
    duplicate_inline
        .add_rule::<ExtraStage<true>>()
        .after::<InlineParserRule>();
    let mut reversed = MarkdownIt::new();
    reversed.remove_rule::<InlineParserRule>();
    reversed
        .add_rule::<InlineParserRule>()
        .before::<BlockParserRule>();
    let mut early_finalizer = MarkdownIt::new();
    early_finalizer
        .add_rule::<EarlyFinalizer>()
        .after::<BlockParserRule>()
        .before::<InlineParserRule>();

    for mut md in [
        missing_block,
        missing_inline,
        duplicate_block,
        duplicate_inline,
        reversed,
        early_finalizer,
    ] {
        for limit in [100, 0] {
            md.max_nesting = limit;
            for source in ["", "# 雪\n\ntext"] {
                assert_direct_configuration_panics(
                    &md,
                    source,
                    "direct parsing requires exactly one block stage followed by exactly one inline stage, supported core rules, preparations before inlines, and draft finalizers after inlines",
                );
            }
        }
    }
}

#[test]
fn direct_parser_accepts_migrated_emphasis_rules() {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    markdown_it::plugins::cmark::inline::emphasis::add(&mut md);

    let direct = md.parse_document("*em* and **strong**");

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
            assert_document_valid_with_config(&md, source, &format!("paragraph={paragraph}"));
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
                    assert_document_valid_with_config(
                        &md,
                        source,
                        &format!("paragraph={paragraph}"),
                    );
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
        assert_document_valid(&md, source);
    }

    let document = md.parse_document("foo ```bar``` baz");
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
fn direct_zero_nesting_matches_expected_output() {
    let mut md = MarkdownIt::empty();
    md.max_nesting = 0;
    assert_document_valid(&md, "text\n");
}

#[test]
fn trailing_space_removal_maps_inline_offsets_once() {
    use markdown_it::parser::inline::Text;
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    markdown_it::plugins::cmark::inline::newline::add(&mut md);
    let source = "雪\r\n次  \r\n行";
    for document in [md.parse_document(source), md.parse_document(source)] {
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
fn direct_html_inline_and_autolinks_match_snapshots() {
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
        assert_document_valid(&md, source);
    }
}

#[test]
fn direct_html_blocks_match_snapshots() {
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
            assert_document_valid(&md, source);
        }
    }

    md.max_nesting = 0;
    assert_document_valid(&md, "<div>");
}

#[test]
fn direct_html_inline_caches_unclosed_comments() {
    let mut md = MarkdownIt::empty();

    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    markdown_it::plugins::cmark::inline::autolink::add(&mut md);
    markdown_it::plugins::html::html_inline::add(&mut md);

    let source = format!("{} tail", "<!--".repeat(8192));
    assert_document_valid(&md, &source);
}

#[test]
fn direct_emphasis_uses_cjk_delimiter_override() {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    markdown_it::plugins::cmark::inline::emphasis::add(&mut md);
    markdown_it::plugins::cjk_friendly::add(&mut md);

    assert_document_valid(&md, "**这是重要内容。**后面继续写");
}

#[test]
fn direct_nested_emphasis_preserves_source_maps() {
    use markdown_it::parser::inline::Text;
    use markdown_it::plugins::cmark::inline::emphasis::{Em, Strong};

    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    markdown_it::plugins::cmark::inline::emphasis::add(&mut md);

    let document = md.parse_document("***foo***");

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
        assert_document_valid(&md, &source);
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
        assert_document_valid_with_config(&md, source, "references=absent");
    }
    let document = md.parse_document("[x](/url)");
    assert_eq!(
        md.render_document(&document),
        "<p><a href=\"/url\">x</a></p>\n",
    );

    // A current-position probe must not spend an extra child depth.
    md.max_nesting = 2;
    assert_document_valid(&md, "[x](/url)");

    // Reference definitions are part of the direct pipeline.
    block::reference::add(&mut md);
    assert_document_valid(&md, "[x](/url)");
}

#[test]
fn direct_references_match_snapshots() {
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
        assert_document_valid(&md, source);
    }

    let document = md.parse_document("[id]: /url\n\n[id]\n\n[id]: /other");
    assert_eq!(
        md.render_document(&document),
        "<p><a href=\"/url\">id</a></p>\n",
    );
}

#[test]
fn direct_forward_references_match_snapshots() {
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
        assert_document_valid(&md, source);
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
    assert_document_valid(&md, &source);
}

#[test]
fn direct_images_and_links_match_expected_output_in_both_registration_orders() {
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
            assert_document_valid_with_config(&md, source, &format!("image_first={image_first}"));
        }
        let document = md.parse_document("![x](/img)");
        assert_eq!(
            md.render_document(&document),
            "<p><img src=\"/img\" alt=\"x\"></p>\n"
        );
        md.max_nesting = 2;
        assert_document_valid(&md, "![x](/img)");
    }
}

#[test]
fn direct_image_only_registration_matches_expected_output() {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut md);
    markdown_it::plugins::cmark::inline::image::add(&mut md);
    assert_document_valid(&md, "![x](/img)");
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
            let document = md.parse_document(source);
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
            let document = md.parse_document(&source);
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
        assert_document_valid(&md, source);
    }
}

#[test]
fn direct_thematic_breaks_match_snapshots() {
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
        assert_document_valid(&md, source);
    }

    let document = md.parse_document("a\n---\nb");
    assert_eq!(md.render_document(&document), "<p>a</p>\n<hr>\n<p>b</p>\n",);
}

#[test]
fn direct_atx_headings_match_snapshots() {
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
        assert_document_valid(&md, source);
    }

    let document = md.parse_document("## *雪* 标题");
    assert_eq!(md.render_document(&document), "<h2><em>雪</em> 标题</h2>\n",);
}

#[test]
fn direct_setext_headings_match_snapshots() {
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
        assert_document_valid(&md, source);
    }

    let document = md.parse_document("foo *bar*\n===");
    assert_eq!(md.render_document(&document), "<h1>foo <em>bar</em></h1>\n");
}

#[test]
fn direct_indented_code_blocks_match_snapshots() {
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
        assert_document_valid(&md, source);
    }

    let document = md.parse_document("    foo\n    bar");
    assert_eq!(
        md.render_document(&document),
        "<pre><code>foo\nbar\n</code></pre>\n",
    );
}

#[test]
fn direct_code_fences_match_snapshots() {
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
        assert_document_valid(&md, source);
    }

    let document = md.parse_document("```rust\nlet x = 1;\n```");
    assert_eq!(
        md.render_document(&document),
        "<pre><code class=\"language-rust\">let x = 1;\n</code></pre>\n",
    );
}

#[test]
fn direct_blockquotes_match_snapshots() {
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
        assert_document_valid(&md, source);
    }

    let document = md.parse_document("> *雪*");
    assert_eq!(
        md.render_document(&document),
        "<blockquote>\n<p><em>雪</em></p>\n</blockquote>\n",
    );
}

#[test]
fn direct_lists_match_snapshots() {
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
        assert_document_valid(&md, source);
    }

    let document = md.parse_document("- foo\n- bar");
    assert_eq!(
        md.render_document(&document),
        "<ul>\n<li>foo</li>\n<li>bar</li>\n</ul>\n",
    );

    let document = md.parse_document("- foo\n\n- bar");
    assert_eq!(
        md.render_document(&document),
        "<ul>\n<li>\n<p>foo</p>\n</li>\n<li>\n<p>bar</p>\n</li>\n</ul>\n",
    );

    let document = md.parse_document("3. foo");
    assert_eq!(
        md.render_document(&document),
        "<ol start=\"3\">\n<li>foo</li>\n</ol>\n",
    );
}

#[test]
fn direct_emph_pair_extras_match_snapshots() {
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
        assert_document_valid(&md, source);
    }

    let document = md.parse_document("==highlighted==");
    assert_eq!(
        md.render_document(&document),
        "<p><mark>highlighted</mark></p>\n",
    );
}

#[test]
fn direct_math_matches_snapshots() {
    let mut md = MarkdownIt::new();
    markdown_it::plugins::extra::math::add(&mut md);
    for source in [
        "",
        "$",
        "$$",
        "$$\n$$",
        "$$\nx\n$$",
        "$$\n\nx\n\n$$",
        "$$\nx",
        "$$ trailing\nx\n$$",
        "   $$  \n  x\n  $$",
        "    $$\nx\n$$",
        "before\n$$\nx\n$$\nafter",
        "> $$\n> x\n> $$\nend",
        "- $$\n  x\n  $$\n- end",
        "- $$\n  x\noutside",
        "$x$",
        "a$x$b",
        "$ x$",
        "$x $",
        "$ x $",
        "$x$1",
        "$10 to $20",
        r"$x\$y$",
        r"$x\\$y$",
        r"\$x$",
        "$$x$",
        "$x$$y$",
        "$x\ny$",
        "$雪🙂$",
        "雪 $x$ 后",
        "$$\n雪🙂 <&>\n$$",
        "$$\r\nx\r\n$$\r\n$x$\rnext",
        "[$x]y$](/url)",
        "![$x]y$](/img)",
        "[$x[y$](/url)",
        "[$x$][ref]\n\n[ref]: /url",
        "`$x$` **$x$** <i>$x$</i>",
        r"$\invalidcommand{<&}$",
        "$$\n\\invalidcommand{<&}\n$$",
    ] {
        assert_document_valid(&md, source);
    }
    for limit in [0, 1, 2] {
        md.max_nesting = limit;
        assert_document_valid(&md, "> $$\n> 雪\n> $$\n\n$x]y$");
    }
}

#[test]
fn direct_math_renders_plain_text_content() {
    let mut md = MarkdownIt::new();
    markdown_it::plugins::extra::math::add(&mut md);
    let document = md.parse_document("$$\nx < y\n$$\n\n雪 $a&b$ 后");
    assert_eq!(
        md.render_document_as(&document, "text"),
        "x < y\n雪 a&b 后\n"
    );
}

#[cfg(feature = "linkify")]
#[test]
fn direct_linkify_matches_snapshots() {
    use markdown_it::plugins::extra::linkify::{self, LinkifyOptions};
    for fuzzy_links in [false, true] {
        for linkify_first in [false, true] {
            let mut md = MarkdownIt::empty();
            if linkify_first {
                linkify::add_with_options(&mut md, LinkifyOptions { fuzzy_links });
            }
            markdown_it::plugins::cmark::add(&mut md);
            markdown_it::plugins::html::add(&mut md);
            if !linkify_first {
                linkify::add_with_options(&mut md, LinkifyOptions { fuzzy_links });
            }
            for source in [
                "",
                "plain 雪",
                "https://example.com/path?q=1&x=2.",
                "www.example.org example.org //example.org",
                "a@b.co mailto:test@example.com MAILTO:test@example.com",
                "雪 https://例子.example/路径 后 user@example.com",
                "https://example.com/🙂",
                "a\nhttps://example.com\r\nb@c.org\rwww.example.org",
                "a\0 https://example.com",
                "https://example.com/foo*bar*baz",
                "https://example.com/foo`bar`baz",
                "https://example.com/foo[123](456)bar",
                "https://example.com/foo&amp;bar",
                "**https://example.com/path** *a@b.co*",
                "`https://example.com`",
                r"\https://example.com",
                r"https\://example.com",
                r"https:\//aa.org https://bb.org",
                r"https:/\/cc.org",
                "x//example.com 。//example.com 组//example.com",
                "[https://example.com](other) ![https://example.com](/img)",
                "[https://example.com/foo[bar]](/url)",
                "![https://example.com/a[b]](/img)",
                "[https://example.com][id]\n\n[id]: /url",
                "[outer ![a@b.co](/img)](/url)",
                "<https://example.com> <a href='/x'>https://example.com</a>",
                "</a>[https://example.com](other)",
                "ftp://example.com javascript://example.com http:/example.com",
                "> https://example.com\n> 雪 a@b.co\n\n- www.example.org\n  https://example.com/path",
                "# https://example.com\n\n    https://example.com\n\n```\nhttps://example.com\n```",
            ] {
                assert_document_valid_with_config(
                    &md,
                    source,
                    &format!("fuzzy_links={fuzzy_links};linkify_first={linkify_first}"),
                );
            }
            markdown_it::plugins::extra::beautify_links::add_with_char_limit(&mut md, 12);
            assert_document_valid_with_config(
                &md,
                "雪 https://example.com/a/very/long/path a@b.co",
                &format!(
                    "fuzzy_links={fuzzy_links};linkify_first={linkify_first};beautify_limit=12"
                ),
            );
        }
    }
}

#[cfg(feature = "linkify")]
#[test]
fn direct_linkify_configuration_and_source_maps() {
    use markdown_it::plugins::extra::linkify::{self, Linkified, LinkifyOptions, LinkifyPrescan};
    let mut md = MarkdownIt::new();
    linkify::add_with_options(&mut md, LinkifyOptions { fuzzy_links: true });
    let source = "雪 https://example.com 后 a@b.co www.example.org";
    assert_document_valid_with_config(&md, source, "prescan=initial");
    let direct = md.parse_document(source);
    let ranges: Vec<_> = direct
        .events(direct.root())
        .filter_map(|event| {
            if matches!(event, StructuralEvent::Exit(_)) {
                return None;
            }
            let node = event.node();
            node.is::<Linkified>()
                .then(|| node.srcmap().unwrap().get_byte_offsets())
        })
        .collect();
    assert_eq!(ranges.len(), 3);
    assert_eq!(
        ranges
            .iter()
            .map(|&(start, end)| &source[start..end])
            .collect::<Vec<_>>(),
        vec!["https://example.com", "a@b.co", "www.example.org"]
    );
    assert_eq!(
        md.render_document_as(&direct, "text"),
        format!("{source}\n")
    );
    for limit in [0, 1, 2] {
        md.max_nesting = limit;
        assert_document_valid(&md, "> https://example.com\n\n雪 a@b.co");
    }
    md.max_nesting = 100;
    md.remove_rule::<LinkifyPrescan>();
    assert_document_valid_with_config(&md, source, "prescan=removed");
    assert_eq!(
        md.render_document(&md.parse_document(source)),
        format!("<p>{source}</p>\n")
    );
    md.add_rule::<LinkifyPrescan>()
        .before::<markdown_it::parser::block::builtin::BlockParserRule>();
    assert_document_valid_with_config(&md, source, "prescan=before_blocks");
    md.remove_rule::<LinkifyPrescan>();
    md.add_rule::<LinkifyPrescan>()
        .after::<markdown_it::parser::block::builtin::BlockParserRule>()
        .before::<markdown_it::parser::inline::builtin::InlineParserRule>();
    assert_document_valid_with_config(&md, source, "prescan=after_blocks");
    md.remove_rule::<LinkifyPrescan>();
    md.add_rule::<LinkifyPrescan>()
        .after::<markdown_it::parser::inline::builtin::InlineParserRule>();
    for source in ["", "https://example.com"] {
        assert_direct_configuration_panics(&md, source, "supported core rules");
    }
}

#[cfg(feature = "linkify")]
#[test]
fn direct_linkify_prescan_runs_before_document_postprocessors() {
    use markdown_it::plugins::extra::{linkify, smartquotes, typographer};
    let mut md = MarkdownIt::new();
    typographer::add(&mut md);
    smartquotes::add(&mut md);
    linkify::add(&mut md);
    // Expected outputs captured from the pre-migration legacy parser.
    for (source, expected) in [
        ("a~~\"foo\"~~", "<p>a~~“foo”~~</p>\n"),
        (
            "\"雪\"... https://example.com/(c)",
            "<p>“雪”… <a href=\"https://example.com/(c)\">https://example.com/(c)</a></p>\n",
        ),
        (
            "ping a@b.co (tm) www.example.org",
            "<p>ping <a href=\"mailto:a@b.co\">a@b.co</a> ™ www.example.org</p>\n",
        ),
    ] {
        let document = md.parse_document(source);
        assert_eq!(md.render_document(&document), expected, "{source}");
    }
}

#[test]
fn direct_core_preparations_share_rule_order_and_lifetime() {
    use markdown_it::parser::block::builtin::BlockParserRule;
    use markdown_it::parser::core::{CoreRule, DocumentCoreRule};
    use markdown_it::parser::extset::RootExtSet;
    use markdown_it::parser::inline::builtin::InlineParserRule;
    use markdown_it::plugins::cmark::block::reference::ReferenceMap;

    struct Preparation<const AFTER_BLOCK: bool>;
    impl<const AFTER_BLOCK: bool> Preparation<AFTER_BLOCK> {
        fn prepare(_: &str, _: &MarkdownIt, root_ext: &mut RootExtSet) {
            assert_eq!(root_ext.contains::<ReferenceMap>(), AFTER_BLOCK);
            root_ext
                .get_or_insert_default::<Vec<bool>>()
                .push(AFTER_BLOCK);
        }
    }
    impl<const AFTER_BLOCK: bool> CoreRule for Preparation<AFTER_BLOCK> {
        fn document_rule() -> DocumentCoreRule {
            DocumentCoreRule::PrepareState(Self::prepare)
        }
    }

    let mut md = MarkdownIt::new();
    md.add_rule::<Preparation<false>>()
        .before::<BlockParserRule>();
    md.add_rule::<Preparation<true>>()
        .after::<BlockParserRule>()
        .before::<InlineParserRule>();
    let source = "[x][id]\n\n[id]: /url";
    assert_document_valid_with_config(&md, source, "preparations=before_and_after_blocks");
    let document = md.parse_document(source);
    assert_eq!(
        document
            .node(document.root())
            .cast::<Root>()
            .unwrap()
            .ext
            .get::<Vec<bool>>()
            .unwrap(),
        &[false, true]
    );

    md.remove_rule::<Preparation<false>>();
    md.remove_rule::<Preparation<true>>();
    let document = md.parse_document(source);
    assert!(
        !document
            .node(document.root())
            .cast::<Root>()
            .unwrap()
            .ext
            .contains::<Vec<bool>>()
    );

    md.add_rule::<Preparation<true>>()
        .after::<BlockParserRule>()
        .before::<InlineParserRule>();
    assert_document_valid_with_config(&md, source, "preparations=after_blocks");
    let document = md.parse_document(source);
    assert_eq!(
        document
            .node(document.root())
            .cast::<Root>()
            .unwrap()
            .ext
            .get::<Vec<bool>>()
            .unwrap(),
        &[true]
    );
}

#[test]
fn direct_footnotes_match_snapshots() {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::extra::footnote::add(&mut md);

    let sources = [
        "Text[^a]\n\n[^a]: note",
        "[^a]: first paragraph\n\n    second paragraph\n\nText[^a]",
        "Text[^a] and[^a]\n\n[^a]: note",
        "inline^[note **strong**] text",
        "Text[^a] B^[inline]\n\n[^a]: named",
        "> quote[^a]\n\n[^a]: note",
        "Text without references\n\n[^unused]: never referenced",
        "Text[^a]\n\n[^a]: outer^[inner]",
        "[label ^[note]](/url)",
        "![alt ^[note]](/img)",
        "Text ^[code `]` span] end",
        "Text ^[escaped \\] and [link](/url) and ![alt](/img)] end",
        "Text ^[outer ^[inner]] end",
        "外[^雪]\n\n[^雪]: 雪",
        "no footnotes here",
    ];
    for source in sources {
        assert_document_valid(&md, source);
    }
}

#[test]
fn direct_directives_match_snapshots() {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::directives::add(&mut md);

    let sources = [
        "",
        "hello :name{a=\"b\"} world",
        "Note: warning",
        ":::bad trailing",
        "::name{cia=\"llo\"}",
        ":::name{cia=\"llo\"}\nworld\n:::",
        ":::name\n:::child\nhello\n:::\n:::",
        ":::name\n::::child\nhello\n::::\n:::",
        ":::name\nhello\n::::",
        "- :::name\n  hello\noutside",
        ":name{#my-id .my-class}",
        ":name{title=\"Ciallo World\"}",
        ":name{disabled}",
        "hello :name{a=\"b\"} world\n\n::leaf{x=\"y\"}\n\n:::box\ncontent\n:::",
        "外:雪{名=\"值\"} end",
        ":::a\n:::b\n:::c\ndeep\n:::\n:::\n:::",
        "[caption :name{title=\"[x]\"}](/url)",
        "![alt :name{a=\"[\"}](/img)",
        ":name{label=\"a[b]c\"}",
    ];
    for source in sources {
        assert_document_valid(&md, source);
    }
}

#[test]
fn direct_directives_custom_renderers_match_expected_output() {
    use markdown_it::plugins::directives::{self, DirectiveKind, DirectiveNode, DirectiveRenderer};

    fn render_badge(
        kind: DirectiveKind,
        name: &str,
        attrs: &[(String, String)],
        node: DirectiveNode<'_>,
        fmt: &mut DirectiveRenderer<'_, '_>,
    ) {
        assert_eq!(kind, DirectiveKind::Text);
        assert_eq!(name, "badge");
        assert!(node.is::<directives::TextDirective>());
        assert_eq!(
            node.cast::<directives::TextDirective>().unwrap().attrs,
            attrs
        );
        assert!(node.srcmap().is_some());
        assert!(!node.ext().is_empty());
        assert_eq!(node.children().len(), 0);
        let count = fmt.ext().get_or_insert_default::<RenderCount>();
        count.0 += 1;
        let count = count.0.to_string();
        let label = attrs
            .iter()
            .find_map(|(key, value)| (key == "label").then_some(value.as_str()))
            .unwrap_or("");
        fmt.open(
            "mark",
            &[
                ("class".into(), "badge".into()),
                ("data-count".into(), count),
            ],
        );
        fmt.text(label);
        fmt.close("mark");
    }

    #[derive(Debug, Default)]
    struct RenderCount(usize);

    fn render_leaf(
        kind: DirectiveKind,
        name: &str,
        attrs: &[(String, String)],
        node: DirectiveNode<'_>,
        fmt: &mut DirectiveRenderer<'_, '_>,
    ) {
        assert_eq!(kind, DirectiveKind::Leaf);
        assert_eq!(name, "callout");
        assert!(node.is::<directives::LeafDirective>());
        assert_eq!(node.children().len(), 0);
        assert!(node.srcmap().is_some());
        fmt.cr();
        fmt.open("aside", node.attrs());
        fmt.text(&attrs[0].1);
        fmt.self_close("hr", &[]);
        fmt.close("aside");
        fmt.cr();
    }

    fn render_panel(
        kind: DirectiveKind,
        name: &str,
        _: &[(String, String)],
        node: DirectiveNode<'_>,
        fmt: &mut DirectiveRenderer<'_, '_>,
    ) {
        assert_eq!(kind, DirectiveKind::Container);
        assert_eq!(name, "panel");
        assert!(node.is::<directives::ContainerDirective>());
        assert!(node.srcmap().is_some());
        assert!(!node.ext().is_empty());
        assert!(node.children().len() > 0);
        assert!(node.children().all(|child| !child.name().is_empty()));
        fmt.cr();
        fmt.open("section", node.attrs());
        fmt.contents(node.children());
        // This must see the state written by nested badge callbacks.
        let count = fmt.ext().get::<RenderCount>().map_or(0, |count| count.0);
        fmt.text_raw(&format!("<!-- badges: {count} -->"));
        fmt.softbreak();
        fmt.close("section");
        fmt.cr();
    }

    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut md);
    directives::add(&mut md);
    directives::add_render(&mut md, DirectiveKind::Text, "badge", render_badge);
    directives::add_render(&mut md, DirectiveKind::Leaf, "callout", render_leaf);
    directives::add_render(&mut md, DirectiveKind::Container, "panel", render_panel);

    for xhtml in [false, true] {
        md.render_options.xhtml_out = xhtml;
        md.render_options.breaks = xhtml;
        for source in [
            ":badge{label=\"Beta\"}",
            "hello :badge{label=\"雪 & <tag>\"} world",
            "::callout{title=\"雪 & <tag>\"}",
            ":::panel\nhello **world** :badge{label=\"Beta\"}\n\n::callout{title=\"Note\"}\n:::",
            ":::panel\n:badge{label=\"First\"}\n\n:::panel\n:badge{label=\"Second\"}\n:::\n\n:::default\nfallback\n:::\n:::",
        ] {
            assert_document_valid(&md, source);
        }
    }
}

#[test]
fn direct_directive_callbacks_render_current_document_children() {
    use markdown_it::document::edit::EditBatch;
    use markdown_it::parser::inline::Text;
    use markdown_it::plugins::directives::{self, DirectiveKind, DirectiveNode, DirectiveRenderer};
    use markdown_it::{DocumentNodeRenderer, DocumentRenderContext, DocumentWriter, NodeRef};

    fn panel(
        _: DirectiveKind,
        _: &str,
        _: &[(String, String)],
        node: DirectiveNode<'_>,
        fmt: &mut DirectiveRenderer<'_, '_>,
    ) {
        fmt.open("section", node.attrs());
        fmt.contents(node.children());
        fmt.close("section");
        fmt.cr();
    }

    struct CustomTextRenderer;
    impl DocumentNodeRenderer<Text> for CustomTextRenderer {
        fn render(
            &self,
            _: NodeRef<'_>,
            text: &Text,
            _: &mut DocumentRenderContext<'_>,
            output: &mut DocumentWriter,
        ) {
            output.write_str(&format!("custom:{}", text.content));
        }
    }

    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut md);
    directives::add(&mut md);
    directives::add_render(&mut md, DirectiveKind::Container, "panel", panel);
    md.add_document_renderer::<Text, _>("html", CustomTextRenderer);
    let mut document = md.parse_document(":::panel\nbefore\n:::");
    let panel = document.children(document.root())[0];
    let paragraph = document.children(panel)[0];
    let text = document.children(paragraph)[0];
    let mut edits = EditBatch::new();
    edits.set_attribute(panel, "data-edited", "yes");
    edits.replace_text(text, 0..6, "after");
    edits.commit(&mut document);
    assert_eq!(
        md.render_document(&document),
        "<section data-edited=\"yes\">\n<p>custom:after</p>\n</section>\n"
    );
}

#[test]
fn direct_draft_finalizers_run_in_order_on_all_parse_paths() {
    use markdown_it::parser::core::{CoreRule, DocumentCoreRule};
    use markdown_it::parser::inline::builtin::InlineParserRule;

    struct Finalize<const SECOND: bool>;
    impl<const SECOND: bool> CoreRule for Finalize<SECOND> {
        fn document_rule() -> DocumentCoreRule {
            DocumentCoreRule::FinalizeDraft(|root, _| {
                assert_eq!(root.attrs().len(), usize::from(SECOND));
                if SECOND {
                    assert_eq!(root.attrs()[0].1, "false");
                }
                root.attrs_mut()
                    .push(("finalizer".into(), SECOND.to_string()));
            })
        }
    }

    for (preset, mut md) in [
        ("empty", MarkdownIt::empty()),
        ("default", MarkdownIt::new()),
    ] {
        for _ in 0..2 {
            md.add_rule::<Finalize<false>>().after::<InlineParserRule>();
            md.add_rule::<Finalize<true>>().after::<Finalize<false>>();
            for limit in [100, 0] {
                md.max_nesting = limit;
                for source in ["", "text", "# 雪\n\n*inline*"] {
                    assert_document_valid_with_config(
                        &md,
                        source,
                        &format!("preset={preset};finalizers=registered"),
                    );
                    let document = md.parse_document(source);
                    assert_eq!(
                        document.node(document.root()).attrs(),
                        &[
                            ("finalizer".into(), "false".into()),
                            ("finalizer".into(), "true".into()),
                        ]
                    );
                }
            }
            md.remove_rule::<Finalize<false>>();
            md.remove_rule::<Finalize<true>>();
            for source in ["", "text"] {
                assert_document_valid_with_config(
                    &md,
                    source,
                    &format!("preset={preset};finalizers=removed"),
                );
                let document = md.parse_document(source);
                assert!(document.node(document.root()).attrs().is_empty());
            }
        }
    }
}

#[cfg(feature = "syntect")]
#[test]
fn syntect_runs_automatically_and_observes_runtime_settings() {
    use markdown_it::plugins::extra::syntect;
    let mut md = MarkdownIt::new();
    syntect::add(&mut md);
    let source = "```rust\nfn main() {}\n```";
    let document = md.parse_document(source);
    let code = document.children(document.root())[0];
    assert!(document.node(code).is::<syntect::SyntectSnippet>());
    assert!(md.render_document(&document).contains("<span"));
    assert_eq!(md.render_document_as(&document, "text"), "fn main() {}\n");
    syntect::set_to_classed_with_prefix(&mut md, "custom-");
    let document = md.parse_document(source);
    assert!(md.render_document(&document).contains("custom-code"));
    assert_document_structure(&md, source);
}
