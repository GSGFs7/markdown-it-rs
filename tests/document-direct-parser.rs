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
    markdown_it::plugins::cmark::inline::newline::add(&mut partial_inline);
    assert!(matches!(
        partial_inline.parse_document_direct("soft\nbreak"),
        Err(DocumentParseError::UnsupportedConfiguration)
    ));
}
