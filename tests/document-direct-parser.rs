use markdown_it::parser::core::Root;
use markdown_it::{DocumentParseError, MarkdownIt};

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
}

#[test]
fn direct_parser_rejects_unmigrated_syntax_rules() {
    let md = MarkdownIt::new();
    assert!(matches!(
        md.parse_document_direct("# heading"),
        Err(DocumentParseError::UnsupportedConfiguration)
    ));
}
