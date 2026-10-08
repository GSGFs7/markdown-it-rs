use std::sync::LazyLock;

#[test]
fn title_example() {
    let parser = &mut markdown_it::MarkdownIt::empty();
    markdown_it::plugins::cmark::add(parser);

    let ast = parser.parse_document("Hello **world**!");
    let html = parser.render_document(&ast);

    assert_eq!(html, "<p>Hello <strong>world</strong>!</p>\n");
}

#[test]
fn lazy_singleton() {
    static MD: LazyLock<markdown_it::MarkdownIt> = LazyLock::new(|| {
        let mut parser = markdown_it::MarkdownIt::empty();
        markdown_it::plugins::cmark::add(&mut parser);
        parser
    });

    let ast = MD.parse_document("Hello **world**!");
    let html = MD.render_document(&ast);

    assert_eq!(html, "<p>Hello <strong>world</strong>!</p>\n");
}

#[test]
fn no_plugins() {
    let md = &mut markdown_it::MarkdownIt::empty();
    let node = md.parse_document("hello\nworld");
    let result = md.render_document(&node);
    assert_eq!(result, "hello\nworld\n");
}

#[test]
fn no_max_indent() {
    let md = &mut markdown_it::MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(md);
    markdown_it::plugins::cmark::block::list::add(md);
    md.max_indent = i32::MAX;
    let node = md.parse_document("        paragraph\n      - item");
    let result = md.render_document(&node);
    assert_eq!(result, "<p>paragraph</p>\n<ul>\n<li>item</li>\n</ul>\n");
}

/*#[test]
fn no_block_parser() {
    let md = &mut markdown_it::MarkdownIt::empty();
    markdown_it::plugins::cmark::add(md);
    md.remove_rule::<markdown_it::parser::block::builtin::BlockParserRule>();
    let node = md.parse_document("hello *world*");
    let result = md.render_document(&node);
    assert_eq!(result, "hello <em>world</em>");
}*/

fn run(input: &str, output: &str) {
    let output = if output.is_empty() {
        "".to_owned()
    } else {
        output.to_owned() + "\n"
    };
    let md = &mut markdown_it::MarkdownIt::empty();
    markdown_it::plugins::cmark::add(md);
    markdown_it::plugins::html::add(md);
    markdown_it::plugins::extra::beautify_links::add(md);
    let node = md.parse_document(&(input.to_owned() + "\n"));
    for event in node.events(node.root()) {
        assert!(event.node().srcmap().is_some());
    }
    let result = md.render_document(&node);
    assert_eq!(result, output);
}

mod markdown_it_rs_extras {
    use super::run;

    #[test]
    fn regression_test_img() {
        // ! at end of line
        run("Hello!", "<p>Hello!</p>");
    }

    #[test]
    fn regression_list_markers() {
        run("- foo\n- bar", "<ul>\n<li>foo</li>\n<li>bar</li>\n</ul>");
        run("1. foo\n1. bar", "<ol>\n<li>foo</li>\n<li>bar</li>\n</ol>");
    }

    #[test]
    fn tab_offset_in_lists() {
        run(
            "   > -\tfoo\n   >\n   >         foo\n",
            r#"<blockquote>
<ul>
<li>
<p>foo</p>
<pre><code> foo
</code></pre>
</li>
</ul>
</blockquote>"#,
        );
    }

    #[test]
    fn null_char_replacement() {
        run("&#0;", "<p>\u{FFFD}</p>");
        run("\0", "<p>\u{FFFD}</p>");
    }

    #[test]
    fn cr_only_newlines() {
        run("foo\rbar", "<p>foo\nbar</p>");
        run("    foo\r    bar", "<pre><code>foo\nbar\n</code></pre>");
    }

    #[test]
    fn cr_lf_newlines() {
        run("foo\r\nbar", "<p>foo\nbar</p>");
        run("    foo\r\n    bar", "<pre><code>foo\nbar\n</code></pre>");
    }

    #[test]
    fn beautify_links() {
        run(
            "<https://www.reddit.com/r/programming/comments/vxttiq/comment/ifyqsqt/?utm_source=reddit&utm_medium=web2x&context=3>",
            "<p><a href=\"https://www.reddit.com/r/programming/comments/vxttiq/comment/ifyqsqt/?utm_source=reddit&amp;utm_medium=web2x&amp;context=3\">www.reddit.com/r/programming/comments/…/ifyqsqt/?…</a></p>",
        );
    }

    #[test]
    fn regression_test_newlines_with_images() {
        run(
            "There is a newline in this image  ![here\nit is](https://github.com/executablebooks/)",
            "<p>There is a newline in this image  <img src=\"https://github.com/executablebooks/\" alt=\"here\nit is\"></p>",
        );
    }

    #[test]
    fn test_node_ext_propagation() {
        use markdown_it::parser::block::BlockRule;
        use markdown_it::parser::core::{CoreRule, DocumentCoreRule};
        use markdown_it::parser::inline::{InlineRule, Text};
        use markdown_it::{DocumentBlockState, DocumentInlineState, MarkdownIt, NodeDraft};

        #[derive(Debug, Default)]
        struct NodeErrors(Vec<&'static str>);
        struct MyInlineRule;
        impl InlineRule for MyInlineRule {
            const MARKER: char = '@';

            fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
                if !state.remaining().starts_with('@') {
                    return None;
                }
                let mut node = NodeDraft::new(Text {
                    content: "@".into(),
                });
                let err = node.ext_mut().get_or_insert_default::<NodeErrors>();
                err.0.push("inline");
                Some((Some(node), 1))
            }
        }

        struct MyBlockRule;
        impl BlockRule for MyBlockRule {
            fn run(state: &mut DocumentBlockState) -> Option<(NodeDraft, usize)> {
                let err = state.node.ext_mut().get_or_insert_default::<NodeErrors>();
                err.0.push("block");
                None
            }
        }

        struct MyCoreRule;
        impl CoreRule for MyCoreRule {
            fn document_rule() -> DocumentCoreRule {
                DocumentCoreRule::FinalizeDraft(|root, _| {
                    root.ext_mut()
                        .get_or_insert_default::<NodeErrors>()
                        .0
                        .push("core");
                })
            }
        }

        let md = &mut markdown_it::MarkdownIt::empty();
        markdown_it::plugins::cmark::add(md);

        md.block.add_rule::<MyBlockRule>();
        md.add_rule::<MyCoreRule>().after_all();

        let text1 = r#"*hello @world*"#;
        let ast = md.parse_document(text1);
        let mut collected: Vec<&str> = vec![];

        for event in ast.events(ast.root()) {
            if matches!(event, markdown_it::StructuralEvent::Exit(_)) {
                continue;
            }
            let node = event.node();
            if let Some(errors) = node.ext().get::<NodeErrors>() {
                collected.extend(errors.0.iter());
            }
        }

        assert_eq!(collected, vec!["block", "core"],);

        let mut direct_md = MarkdownIt::empty();
        direct_md.inline.add_rule::<MyInlineRule>();
        let document = direct_md.parse_document("@");
        let collected: Vec<_> = document
            .events(document.root())
            .filter_map(|event| event.node().ext().get::<NodeErrors>())
            .flat_map(|errors| errors.0.iter().copied())
            .collect();
        assert_eq!(collected, vec!["inline"]);
    }

    #[cfg(feature = "syntect")]
    #[test]
    fn syntect_classed_mode_renders_language_class() {
        let md = &mut markdown_it::MarkdownIt::empty();
        markdown_it::plugins::cmark::add(md);
        markdown_it::plugins::extra::syntect::add(md);
        markdown_it::plugins::extra::syntect::set_to_classed(md);

        let html = md.render("```rust ignore-me\nfn main() {}\n```");

        assert!(html.contains(r#"<code class="syntect-code language-rust">"#));
    }

    #[cfg(feature = "syntect")]
    #[test]
    fn syntect_classed_mode_escapes_language_class() {
        let md = &mut markdown_it::MarkdownIt::empty();
        markdown_it::plugins::cmark::add(md);
        markdown_it::plugins::extra::syntect::add(md);
        markdown_it::plugins::extra::syntect::set_to_classed(md);

        let html = md.render("```rust&quot; onclick=&quot;alert(1)\nfn main() {}\n```");

        assert!(html.contains("language-rust&quot;"));
        assert!(!html.contains(r#"onclick="alert(1)""#));
    }

    #[cfg(feature = "syntect")]
    #[test]
    fn syntect_classed_mode_respects_custom_lang_prefix() {
        let md = &mut markdown_it::MarkdownIt::empty();
        markdown_it::plugins::cmark::add(md);
        markdown_it::plugins::cmark::block::fence::set_lang_prefix(md, "lang-");
        markdown_it::plugins::extra::syntect::add(md);
        markdown_it::plugins::extra::syntect::set_to_classed(md);

        let html = md.render("```rust\nfn main() {}\n```");

        assert!(html.contains(r#"<code class="syntect-code lang-rust">"#));
    }

    #[cfg(feature = "syntect")]
    #[test]
    fn syntect_parses_attached_line_spec_after_language() {
        let md = &mut markdown_it::MarkdownIt::empty();
        markdown_it::plugins::cmark::add(md);
        markdown_it::plugins::extra::syntect::add(md);

        let html = md.render("```rust{2}\nfn main() {\n    println!(\"hi\");\n}\n```");

        assert!(html.contains(r#"<code class="language-rust">"#));
        assert!(html.contains(r#"<span class="syntect-line syntect-line-highlighted">"#));
    }

    #[cfg(feature = "syntect")]
    #[test]
    fn syntect_theme_css_depends_on_mode() {
        let md = &mut markdown_it::MarkdownIt::empty();
        markdown_it::plugins::extra::syntect::add(md);

        assert_eq!(markdown_it::plugins::extra::syntect::theme_css(md), None);

        markdown_it::plugins::extra::syntect::set_to_classed(md);
        let css = markdown_it::plugins::extra::syntect::theme_css(md);

        assert!(css.is_some());
        assert!(css.unwrap().contains(".syntect-code"));
    }

    #[cfg(feature = "syntect")]
    #[test]
    #[should_panic(expected = "unknown syntect theme: definitely-not-a-theme")]
    fn syntect_invalid_theme_panics() {
        let md = &mut markdown_it::MarkdownIt::empty();
        markdown_it::plugins::cmark::add(md);
        markdown_it::plugins::extra::syntect::add(md);
        markdown_it::plugins::extra::syntect::set_theme(md, "definitely-not-a-theme");

        let _ = md.render("```rust\nfn main() {}\n```");
    }
}

mod examples {
    include!("../examples/ferris/main.rs");

    #[test]
    fn test_examples() {
        main();
    }
}
