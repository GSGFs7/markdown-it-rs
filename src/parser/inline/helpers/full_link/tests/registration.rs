use super::*;

#[test]
fn custom_prefix_policies_are_independent_of_marker_and_label_context() {
    fn verify<const PREFIX: char>() {
        for fallback in [
            InlineTargetFallback::Reference,
            InlineTargetFallback::Reject,
        ] {
            for label_context in [LabelContext::NestedLink, LabelContext::Independent] {
                let mut md = utf8_parser::<PREFIX>();
                add_prefix_with_options::<PREFIX, true>(
                    &mut md,
                    LinkOptions {
                        inline_target_fallback: fallback,
                        label_context,
                    },
                    |document, href, title| {
                        document.create_node(Link {
                            url: href.unwrap_or_default(),
                            title,
                        })
                    },
                );
                let source = format!("{PREFIX}[foo](/unfinished");
                let mut references = ReferenceMap::default();
                references.insert("foo".into(), "/url".into(), None);
                let mut root_ext = crate::common::extset::RootExtSet::default();
                root_ext.insert(references);
                let mut document =
                    crate::document::Document::new("", crate::document::Root::new(""));
                let rules = md.inline.rules();
                let mut context =
                    InlineState::new(&mut document, &source, 0, source.len(), &md, rules, 0, 0);
                context.root_ext = Some(&root_ext);
                let expected_end = match fallback {
                    InlineTargetFallback::Reference => Some(PREFIX.len_utf8() + "[foo]".len()),
                    InlineTargetFallback::Reject => None,
                };
                assert_eq!(
                    LinkPrefixScanner::<PREFIX, true>::check(&mut context),
                    expected_end
                );
                let retained_prefix = if fallback == InlineTargetFallback::Reject {
                    PREFIX.to_string()
                } else {
                    String::new()
                };
                assert_eq!(
                    md.render(&format!("{source}\n\n[foo]: /url\n")),
                    format!("<p>{retained_prefix}<a href=\"/url\">foo</a>(/unfinished</p>\n"),
                );
                assert_eq!(
                    md.render(&format!("{PREFIX}[foo](/inline)")),
                    "<p><a href=\"/inline\">foo</a></p>\n",
                );
                assert_eq!(
                    md.render(&format!("{PREFIX}[foo][ref]\n\n[ref]: /url\n")),
                    "<p><a href=\"/url\">foo</a></p>\n",
                );
            }
        }
    }
    verify::<'~'>();
    verify::<'雪'>();
    verify::<'!'>();
}

#[test]
fn custom_prefix_uses_utf8_byte_length() {
    for (source, html) in [
        ("雪[文字](/url)", "<p><a href=\"/url\">文字</a></p>\n"),
        (
            "雪[文字][ref]\n\n[ref]: /url",
            "<p><a href=\"/url\">文字</a></p>\n",
        ),
        ("雪[文字]", "<p>雪[文字]</p>\n"),
    ] {
        assert_eq!(utf8_parser::<'雪'>().render(source), html, "{source}");
    }

    assert_eq!(
        utf8_parser::<'😀'>().render("😀[x](/url)"),
        "<p><a href=\"/url\">x</a></p>\n",
    );
    assert_eq!(
        utf8_parser::<'~'>().render("~[x](/url)"),
        "<p><a href=\"/url\">x</a></p>\n",
    );
}

#[test]
fn registered_link_check_does_not_require_running_the_factory() {
    let mut document = crate::document::Document::new("", crate::document::Root::new(""));
    let mut md = MarkdownIt::empty();
    add::<false>(&mut md, |_, _, _| {
        panic!("check must not call arena node factory")
    });
    let ruleset = md.inline.rules();
    let source = "[x](/url)";
    let mut context = InlineState::new(&mut document, source, 0, source.len(), &md, ruleset, 0, 0);
    let token = context.skip_token().unwrap();
    assert_eq!(token, source.len());
    assert_eq!(context.link_level, 0);
    assert!(context.skip_token().is_none());
}

#[test]
fn registered_prefix_check_does_not_require_running_the_factory() {
    let mut document = crate::document::Document::new("", crate::document::Root::new(""));
    let mut md = MarkdownIt::empty();
    add_prefix::<'雪', true>(&mut md, |_, _, _| {
        panic!("check must not call arena node factory")
    });
    let ruleset = md.inline.rules();
    let source = "雪[x](/url)";
    let mut context = InlineState::new(&mut document, source, 0, source.len(), &md, ruleset, 0, 0);
    let token = context.skip_token().unwrap();
    assert_eq!(token, source.len());
    assert_eq!(context.link_level, 0);
    assert!(context.skip_token().is_none());
}

#[test]
fn direct_prefix_uses_utf8_byte_offsets() {
    fn check<const PREFIX: char>() {
        let mut md = MarkdownIt::empty();
        crate::plugins::cmark::block::paragraph::add(&mut md);
        crate::plugins::cmark::inline::link::add(&mut md);
        add_prefix::<PREFIX, true>(&mut md, |document, href, title| {
            document.create_node(Link {
                url: href.unwrap_or_default(),
                title,
            })
        });
        // Expected outputs captured from the pre-migration legacy parser.
        let cases = [
            (
                format!("{PREFIX}[雪](/url)"),
                "<p><a href=\"/url\">雪</a></p>\n".to_owned(),
                1,
            ),
            (
                format!("[a {PREFIX}[雪](/in)](/out)"),
                "<p><a href=\"/out\">a <a href=\"/in\">雪</a></a></p>\n".to_owned(),
                2,
            ),
            (
                format!("{PREFIX}[雪](/unfinished"),
                format!("<p>{PREFIX}[雪](/unfinished</p>\n"),
                0,
            ),
        ];
        for (source, expected, link_count) in cases {
            let document = md.parse(&source);
            assert_eq!(md.render_document(&document, "html"), expected, "{source}");
            let links = document
                .events(document.root())
                .filter(|event| {
                    matches!(event, crate::document::StructuralEvent::Enter(_))
                        && event.node().is::<Link>()
                })
                .count();
            assert_eq!(links, link_count, "{source}");
            #[cfg(feature = "sourcemap")]
            if link_count > 0 {
                let start = source.find("[雪]").unwrap() + 1;
                let text_ranges: Vec<_> = document
                    .events(document.root())
                    .filter_map(|event| {
                        let node = event.node();
                        node.cast::<crate::document::Text>()
                            .filter(|text| text.content == "雪")
                            .map(|_| node.srcmap().unwrap().get_byte_offsets())
                    })
                    .collect();
                assert_eq!(text_ranges, [(start, start + "雪".len())], "{source}");
            }
        }
    }
    check::<'~'>();
    check::<'雪'>();
    check::<'🦀'>();
}

#[test]
fn add_prefix_uses_the_parser_wide_reference_table() {
    let md = utf8_parser::<'^'>();
    assert_eq!(
        md.render("^[foo][ref]\n\n[ref]: /url"),
        "<p><a href=\"/url\">foo</a></p>\n"
    );
}

#[test]
fn link_rules_resolve_injected_references_without_the_reference_definition_plugin() {
    use crate::parser::block::BlockParserRule;
    use crate::parser::core::{CoreRule, CoreState};
    use crate::plugins::cmark::block::paragraph;
    use crate::plugins::cmark::inline::link;

    #[derive(Default)]
    struct AddReferences;

    impl CoreRule for AddReferences {
        fn run(&self, state: &mut CoreState<'_>) {
            let mut references = ReferenceMap::default();
            references.insert("id".into(), "/external".into(), None);
            state.root_ext_mut().insert(references);
        }
    }

    let mut md = MarkdownIt::empty();
    paragraph::add(&mut md);
    link::add(&mut md);
    add_prefix::<'^', true>(&mut md, |document, href, title| {
        document.create_node(Link {
            url: href.unwrap_or_default(),
            title,
        })
    });
    md.add_rule::<AddReferences>().before::<BlockParserRule>();

    assert_eq!(
        md.render("[id] ^[id]"),
        "<p><a href=\"/external\">id</a> <a href=\"/external\">id</a></p>\n"
    );
}

#[test]
fn nested_scan_respects_disabled_nesting_with_references() {
    let mut md = MarkdownIt::empty();
    crate::plugins::cmark::add(&mut md);
    super::add_prefix::<'~', false>(&mut md, |document, href, title| {
        document.create_node(Link {
            url: href.unwrap_or_default(),
            title,
        })
    });

    let html = md.render("~[outer [only-original]](/outer)\n\n[only-original]: /inner");
    assert!(!html.contains("href=\"/outer\""), "{html}");
    assert!(
        html.contains("<a href=\"/inner\">only-original</a>"),
        "{html}"
    );
}

#[test]
fn all_link_rules_share_the_document_reference_map() {
    use crate::parser::block::BlockParserRule;
    use crate::parser::core::{CoreRule, CoreState};
    use crate::reference::{CustomReferenceMap, DefaultReferenceMap, ReferenceMap};

    #[derive(Debug, Default)]
    struct CustomRefs(DefaultReferenceMap);

    impl CustomReferenceMap for CustomRefs {
        fn insert(&mut self, label: String, destination: String, title: Option<String>) -> bool {
            self.0.insert(label, destination, title)
        }

        fn get(&self, label: &str) -> Option<(&str, Option<&str>)> {
            if label == "id" {
                Some(("/custom", None))
            } else {
                self.0.get(label)
            }
        }
    }

    #[derive(Default)]
    struct AddCustomRefs;
    impl CoreRule for AddCustomRefs {
        fn run(&self, state: &mut CoreState<'_>) {
            state
                .root_ext_mut()
                .insert(ReferenceMap::new(CustomRefs::default()));
        }
    }

    let mut md = MarkdownIt::empty();
    crate::plugins::cmark::add(&mut md);
    super::add_prefix::<'^', true>(&mut md, |document, href, title| {
        document.create_node(Link {
            url: href.unwrap_or_default(),
            title,
        })
    });
    md.add_rule::<AddCustomRefs>().before::<BlockParserRule>();
    assert_eq!(
        md.render("[id] ^[id]\n\n[id]: /default"),
        "<p><a href=\"/custom\">id</a> <a href=\"/custom\">id</a></p>\n"
    );
}
