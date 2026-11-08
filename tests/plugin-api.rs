use markdown_it::MarkdownIt;
use markdown_it::document::NodeValue;
use markdown_it::plugin::{Html, InlinePluginState, MarkdownItBuilder, Plugin, PluginSpec};

#[derive(Debug)]
struct Emoji {
    glyph: &'static str,
    label: &'static str,
}

const EMOJIS: &[(&str, &str, &str)] =
    &[(":rocket:", "🚀", "rocket"), (":warning:", "⚠️", "warning")];

#[test]
fn inline_container_parses_children_through_the_public_api() {
    #[derive(Debug)]
    struct Boxed(String);

    let boxed = PluginSpec::new("boxed")
        .inline_container::<Boxed>("boxed")
        .marker('[')
        .parse(|cx| {
            let source = cx.source();
            if !source.starts_with("[!") {
                return None;
            }
            let end = source.find(']')?;
            cx.advance(end + 1);
            Some((Boxed("note".into()), 2..end))
        })
        .render_html(|value, html| {
            let children = html.render_children();
            html.element("span")
                .class("box")
                .attr("data-kind", &value.0)
                .raw(children);
        })
        .render_text(|_, text| {
            text.write_str("[");
            text.render_children();
            text.write_str("]");
        })
        .finish();

    let md = MarkdownIt::builder().plugin(boxed).build();
    let document = md.parse("[!*雪*]");
    assert_eq!(
        md.render_document(&document, "html"),
        "<p><span class=\"box\" data-kind=\"note\"><em>雪</em></span></p>\n"
    );
    assert_eq!(md.render_document(&document, "text"), "[雪]\n");
}

#[test]
fn inline_pair_lowers_to_low_level_rule_and_keeps_captured_values() {
    use markdown_it::parser::inline::helpers::emph_pair::EmphPairScanner;

    #[derive(Debug)]
    struct Pair(String);

    let label = String::from("captured configuration");
    let pair = PluginSpec::new("pair")
        .inline_pair::<Pair>("pair")
        .delimiter::<'🦀', 2, true>()
        .value(move || Pair(label.clone()))
        .html_tag("mark")
        .finish();
    let md = MarkdownIt::builder().plugin(pair).build();
    assert!(md.inline.has_rule::<EmphPairScanner<'🦀', true>>());

    let document = md.parse("🦀🦀a **b**🦀🦀");
    assert_eq!(
        md.render_document(&document, "html"),
        "<p><mark>a <strong>b</strong></mark></p>\n"
    );
    let paragraph = document.children(document.root())[0];
    let wrapper = document.node(document.children(paragraph)[0]);
    assert_eq!(wrapper.cast::<Pair>().unwrap().0, "captured configuration");
    #[cfg(feature = "sourcemap")]
    assert_eq!(wrapper.srcmap().unwrap().get_byte_offsets(), (0, 23));
}

#[test]
fn inline_pair_and_later_low_level_registration_share_the_scanner() {
    use markdown_it::parser::inline::helpers::emph_pair;

    #[derive(Debug)]
    struct One;
    #[derive(Debug)]
    struct Two;

    struct OneRenderer;
    impl markdown_it::render::NodeRenderer<One> for OneRenderer {
        fn render(
            &self,
            node: &markdown_it::document::Node,
            _: &One,
            context: &mut markdown_it::render::RenderContext<'_>,
            output: &mut markdown_it::render::Writer,
        ) {
            output.write_str("<i>");
            context.render_children(node.id(), output);
            output.write_str("</i>");
        }
    }

    let pair = PluginSpec::new("two")
        .inline_pair::<Two>("two")
        .delimiter::<'%', 2, true>()
        .value(|| Two)
        .html_tag("b")
        .finish();
    let mut md = MarkdownIt::builder().plugin(pair).build();
    emph_pair::add_with::<'%', 1, true>(&mut md, |document| document.create_node(One));
    md.add_renderer::<One, _>("html", OneRenderer);
    assert_eq!(
        md.render("%one% and %%two%%"),
        "<p><i>one</i> and <b>two</b></p>\n"
    );
}

#[test]
fn inline_pair_conflicts_are_rejected_before_any_installer_runs() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[derive(Debug)]
    struct Pair;

    let installed = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&installed);
    let mut group = PluginSpec::new("group").with(PluginSpec::custom("observer", move |_| {
        flag.store(true, Ordering::SeqCst);
    }));
    for name in ["first", "second"] {
        group = group.with(
            PluginSpec::new(name)
                .inline_pair::<Pair>("pair")
                .delimiter::<'=', 2, true>()
                .value(|| Pair)
                .html_tag("mark")
                .finish(),
        );
    }
    let mut md = MarkdownIt::new();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        group.install(&mut md);
    }));
    assert!(result.is_err());
    assert!(!installed.load(Ordering::SeqCst));
}

fn emoji() -> PluginSpec {
    PluginSpec::new("emoji")
        .inline_leaf::<Emoji>("shortcode")
        .marker(':')
        .parse(|cx| {
            let &(shortcode, glyph, label) =
                EMOJIS.iter().find(|(code, _, _)| cx.starts_with(code))?;

            cx.consume(shortcode);
            Some(Emoji { glyph, label })
        })
        .render_html(|emoji, html| {
            html.element("span")
                .class("emoji")
                .attr("role", "img")
                .attr("aria-label", emoji.label)
                .text(emoji.glyph);
        })
        .finish()
}

#[test]
fn readme_emoji_example_works() {
    let md = MarkdownIt::builder().plugin(emoji()).build();

    assert_eq!(
        md.render("Ready :rocket:"),
        "<p>Ready <span class=\"emoji\" role=\"img\" aria-label=\"rocket\">🚀</span></p>\n"
    );
    assert_eq!(
        md.render(":warning: :missing:"),
        "<p><span class=\"emoji\" role=\"img\" aria-label=\"warning\">⚠️</span> :missing:</p>\n"
    );
}

#[derive(Debug)]
struct Tag(String);

fn configured_leaf<T, F, R>(name: &str, marker: char, parse: F, render: R) -> PluginSpec
where
    T: NodeValue,
    F: Fn(&mut InlinePluginState<'_>) -> Option<T> + Send + Sync + 'static,
    R: Fn(&T, &mut Html) + Send + Sync + 'static,
{
    PluginSpec::new(name)
        .inline_leaf::<T>(name)
        .marker(marker)
        .parse(parse)
        .render_html(render)
        .finish()
}

#[test]
fn fluent_leaves_keep_the_parent_identity_and_order_all_its_rules() {
    let plugin = PluginSpec::new("group")
        .after("first")
        .inline_leaf::<Tag>("at")
        .marker('@')
        .parse(|cx| cx.consume("@").then(|| Tag("GROUP".into())))
        .render_html(|tag, html| {
            html.text(&tag.0);
        })
        .finish()
        .inline_leaf::<Tag>("caret")
        .marker('^')
        .parse(|cx| cx.consume("^").then(|| Tag("CARET".into())))
        .render_html(|tag, html| {
            html.text(&tag.0);
        })
        .finish();
    assert_eq!(plugin.name(), "group");

    let md = MarkdownIt::builder()
        .plugin(plugin)
        .plugin(token_spec("first", '@', "FIRST"))
        // Internal leaf names are not separate plugin identities.
        .plugin(PluginSpec::new("at").after("group"))
        .build();
    assert_eq!(md.render("@ ^"), "<p>FIRST CARET</p>\n");
}

#[test]
fn fluent_leaf_supports_captured_config_default_marker_and_rollback() {
    let label = String::from("<done>");
    let plugin = PluginSpec::new("todo")
        .inline_leaf::<Tag>("word")
        // Callbacks can be supplied in either order.
        .render_html(|tag, html| {
            html.text(&tag.0);
        })
        .parse(move |cx| {
            if !cx.consume("TODO") || !cx.consume("!") {
                return None;
            }
            Some(Tag(label.clone()))
        })
        .finish();
    let md = MarkdownIt::builder().plugin(plugin).build();
    assert_eq!(md.render("x TODO TODO!"), "<p>x TODO &lt;done&gt;</p>\n");
}

#[test]
#[should_panic(expected = "was added more than once")]
fn fluent_leaf_rejects_duplicate_rules_within_one_plugin() {
    let mut plugin = PluginSpec::new("duplicate");
    for _ in 0..2 {
        plugin = plugin
            .inline_leaf::<Tag>("same")
            .marker('@')
            .parse(|cx| cx.consume("@").then(|| Tag("x".into())))
            .render_html(|tag, html| {
                html.text(&tag.0);
            })
            .finish();
    }
    MarkdownIt::builder().plugin(plugin).build();
}

#[test]
fn failed_match_rolls_back_text() {
    let leaf = configured_leaf::<Tag, _, _>(
        "tag",
        '@',
        |cx| {
            if !cx.consume("@") {
                return None;
            }
            let end = cx.remaining().find('}')?;
            let value = cx.remaining_slice(0..end)?;
            if value.is_empty() {
                return None;
            }
            cx.advance(end + 1);
            Some(Tag(value.to_owned()))
        },
        |tag, html| {
            html.element("code").text(&tag.0);
        },
    );

    let md = MarkdownIt::builder().plugin(leaf).build();
    assert_eq!(md.render("@ broken"), "<p>@ broken</p>\n");
}

#[test]
fn multiple_leaves_share_the_inline_rule_chain() {
    let a = configured_leaf::<Tag, _, _>(
        "a",
        '%',
        |cx| cx.consume("%a").then_some(Tag("a".into())),
        |tag, html| {
            html.element("b").text(&tag.0);
        },
    );
    let b = configured_leaf::<Tag, _, _>(
        "b",
        '&',
        |cx| cx.consume("&b").then_some(Tag("b".into())),
        |tag, html| {
            html.element("b").text(&tag.0);
        },
    );

    let md = MarkdownIt::builder().plugin(a).plugin(b).build();
    assert_eq!(md.render("%a &b"), "<p><b>a</b> <b>b</b></p>\n");
}

#[test]
fn builder_can_start_from_an_empty_parser() {
    let leaf = configured_leaf::<Tag, _, _>(
        "plain",
        '^',
        |cx| cx.consume("^").then_some(Tag("x".into())),
        |tag, html| {
            html.element("span").text(&tag.0);
        },
    );

    let md = MarkdownItBuilder::empty().plugin(leaf).build();
    assert_eq!(
        md.render("^ *not emphasis*"),
        "<span>x</span> *not emphasis*\n"
    );
}

#[test]
fn html_builder_escapes_untrusted_payload() {
    #[derive(Debug)]
    struct Raw(&'static str);

    let leaf = configured_leaf::<Raw, _, _>(
        "raw",
        '~',
        |cx| cx.consume("~").then_some(Raw("<script>&\"")),
        |raw, html| {
            html.element("span").attr("data-value", raw.0).text(raw.0);
        },
    );

    let md = MarkdownIt::builder().plugin(leaf).build();
    assert_eq!(
        md.render("~"),
        "<p><span data-value=\"&lt;script&gt;&amp;&quot;\">&lt;script&gt;&amp;&quot;</span></p>\n"
    );
}

#[test]
fn default_marker_is_tried_at_every_character() {
    #[derive(Debug)]
    struct Everywhere;

    let leaf = configured_leaf::<Everywhere, _, _>(
        "everywhere",
        '\0',
        |cx| {
            if !cx.consume("TODO") {
                return None;
            }
            Some(Everywhere)
        },
        |_, html| {
            html.element("strong").text("TODO");
        },
    );

    let md = MarkdownIt::builder().plugin(leaf).build();
    assert_eq!(md.render("a TODO b"), "<p>a <strong>TODO</strong> b</p>\n");
}

#[test]
fn wildcard_installation_invalidates_warmed_ascii_and_unicode_scanners() {
    use markdown_it::plugins::cmark::inline::entity::EntityScanner;

    for unicode in [false, true] {
        let mut md = MarkdownIt::new();
        if unicode {
            token_spec("unicode", '雪', "snow").install(&mut md);
        }
        let source = "a TODO 雪 [TODO](/url)";
        let snow = if unicode { "snow" } else { "雪" };
        assert_eq!(
            md.render(source),
            format!("<p>a TODO {snow} <a href=\"/url\">TODO</a></p>\n")
        );

        configured_leaf::<Tag, _, _>(
            "todo",
            '\0',
            |cx| cx.consume("TODO").then(|| Tag("done".into())),
            |tag, html| {
                html.text(&tag.0);
            },
        )
        .install(&mut md);
        let expected = format!("<p>a done {snow} <a href=\"/url\">done</a></p>\n");
        assert_eq!(md.render(source), expected);

        // Rebuilding the scanner after marker changes must keep wildcard mode.
        md.inline.remove_rule::<EntityScanner>();
        assert_eq!(md.render(source), expected);
        md.inline.add_rule::<EntityScanner>();
        assert_eq!(md.render(source), expected);
    }
}

#[test]
#[should_panic(expected = "was added more than once")]
fn duplicate_plugin_names_panic_before_installation() {
    MarkdownIt::builder()
        .plugin(PluginSpec::new("same"))
        .plugin(PluginSpec::new("same"))
        .build();
}

fn token_leaf(name: &str, marker: char, output: &'static str) -> PluginSpec {
    configured_leaf::<Tag, _, _>(
        name,
        marker,
        move |cx| (cx.consume_char()? == marker).then(|| Tag(output.into())),
        |tag, html| {
            html.text(&tag.0);
        },
    )
}

fn token_spec(name: &str, marker: char, output: &'static str) -> PluginSpec {
    token_leaf(name, marker, output)
}

#[test]
fn removing_builtin_preserves_dynamic_marker_even_after_scanner_compilation() {
    use markdown_it::plugins::cmark::inline::entity::EntityScanner;
    let mut md = MarkdownIt::builder()
        .plugin(token_spec("token", '&', "MATCH"))
        .build();
    assert_eq!(md.render("a &"), "<p>a MATCH</p>\n");
    md.inline.remove_rule::<EntityScanner>();
    assert_eq!(md.render("a &"), "<p>a MATCH</p>\n");
    // Removing an already absent rule must preserve the marker as well.
    md.inline.remove_rule::<EntityScanner>();
    assert_eq!(md.render("a &"), "<p>a MATCH</p>\n");
}

#[test]
fn explicit_renderer_after_install_overrides_all_payload_nodes() {
    use markdown_it::document::Node;
    use markdown_it::render::{NodeRenderer, RenderContext, Writer};
    struct Custom;
    impl NodeRenderer<Tag> for Custom {
        fn render(&self, _: &Node, tag: &Tag, _: &mut RenderContext<'_>, output: &mut Writer) {
            output.write_str(&format!("CUSTOM({})", tag.0));
        }
    }

    let mut md = MarkdownIt::builder()
        .plugin(token_spec("a", '@', "A"))
        .build();
    assert_eq!(md.render("@"), "<p>A</p>\n");

    md.add_renderer::<Tag, _>("html", Custom);
    assert_eq!(md.render("@"), "<p>CUSTOM(A)</p>\n");
}

#[test]
fn high_level_rules_work_inside_link_labels_and_preserve_literal_failures() {
    let md = MarkdownIt::builder().plugin(emoji()).build();
    assert_eq!(
        md.render("[:rocket:](https://example.com)"),
        "<p><a href=\"https://example.com\"><span class=\"emoji\" role=\"img\" aria-label=\"rocket\">🚀</span></a></p>\n"
    );
    assert_eq!(
        md.render("[:missing:](https://example.com)"),
        "<p><a href=\"https://example.com\">:missing:</a></p>\n"
    );
}

#[test]
fn plugin_dependencies_order_matches_independently_of_registration_order() {
    let md = MarkdownIt::builder()
        .plugin(token_spec("later", '@', "LATER").after("first"))
        .plugin(token_spec("first", '@', "FIRST"))
        .build();
    assert_eq!(md.render("@"), "<p>FIRST</p>\n");
    let md = MarkdownIt::builder()
        .plugin(token_spec("later", '@', "LATER"))
        .plugin(token_spec("first", '@', "FIRST").before("later"))
        .build();
    assert_eq!(md.render("@"), "<p>FIRST</p>\n");
}

fn assert_panics(run: impl FnOnce()) {
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)).is_err());
}

#[test]
fn missing_dependencies_panic_during_build() {
    let missing = PluginSpec::new("missing").require("absent");
    assert_panics(|| {
        MarkdownIt::builder().plugin(missing).build();
    });
}

#[test]
fn dependency_cycles_panic_during_build() {
    assert_panics(|| {
        MarkdownIt::builder()
            .plugin(PluginSpec::new("a").after("b"))
            .plugin(PluginSpec::new("b").after("a"))
            .build();
    });
}

#[test]
fn direct_installation_validates_before_changing_the_parser() {
    let mut md = MarkdownIt::new();
    assert_panics(|| PluginSpec::new("").install(&mut md));
    assert_eq!(md.render("a @"), "<p>a @</p>\n");

    token_spec("valid", '@', "OK").install(&mut md);
    assert_eq!(md.render("a @"), "<p>a OK</p>\n");

    assert_panics(|| token_spec("valid", '@', "AGAIN").install(&mut md));
    assert_eq!(md.render("a @"), "<p>a OK</p>\n");

    assert_panics(|| PluginSpec::new("before").before("valid").install(&mut md));
    PluginSpec::new("after").after("valid").install(&mut md);
}

#[test]
fn low_level_dependency_errors_are_found_during_build() {
    use markdown_it::parser::inline::InlineRule;
    struct Never;
    impl InlineRule for Never {
        const MARKER: char = '~';
        fn run(
            _: &mut markdown_it::parser::inline::InlineState<'_>,
        ) -> Option<(Option<markdown_it::document::NodeId>, usize)> {
            None
        }
    }
    assert_panics(|| {
        MarkdownIt::builder()
            .preset(|md: &mut MarkdownIt| {
                md.inline.add_rule::<Never>().require_named("absent");
            })
            .build();
    });
}

#[test]
fn direct_installation_finds_low_level_dependency_errors() {
    use markdown_it::parser::core::{CoreRule, CoreState};

    struct Never;
    impl CoreRule for Never {
        fn run(&self, _: &mut CoreState<'_>) {}
    }

    let plugin = PluginSpec::custom("broken", |md| {
        md.add_rule_instance(Never).require_named("absent");
    });

    let mut md = MarkdownIt::new();
    assert_panics(|| plugin.install(&mut md));
}

#[test]
fn wildcard_and_specific_candidates_keep_registration_order() {
    let wildcard = configured_leaf::<Tag, _, _>(
        "wildcard",
        '\0',
        |cx| cx.consume("@").then(|| Tag("WILD".into())),
        |tag, html| {
            html.text(&tag.0);
        },
    );
    let md = MarkdownIt::builder()
        .plugin(wildcard)
        .plugin(token_spec("specific", '@', "SPECIFIC"))
        .build();
    assert_eq!(md.render("a @ b"), "<p>a WILD b</p>\n");
}

#[test]
fn unicode_markers_and_utf8_cursor_boundaries() {
    let leaf = configured_leaf::<Tag, _, _>(
        "unicode",
        '🦀',
        |cx| {
            assert!(!cx.advance(1));
            assert_eq!(cx.consumed(), 0);
            cx.consume("🦀").then(|| Tag("CRAB".into()))
        },
        |tag, html| {
            html.text(&tag.0);
        },
    );
    let md = MarkdownIt::builder().plugin(leaf).build();
    assert_eq!(md.render("中文🦀尾"), "<p>中文CRAB尾</p>\n");
}

macro_rules! leaf_tag {
    ($name:ident, $rule:literal, $marker:literal, $output:literal) => {
        #[derive(Debug)]
        struct $name;

        impl $name {
            fn plugin() -> PluginSpec {
                configured_leaf::<Self, _, _>(
                    $rule,
                    $marker,
                    |cx| (cx.consume_char()? == $marker).then_some(Self),
                    |_, html| {
                        html.element("b").text($output);
                    },
                )
            }
        }
    };
}

leaf_tag!(A, "a", '@', "A");
leaf_tag!(B, "b", '雪', "B");
leaf_tag!(C, "c", '@', "C");

#[test]
fn grouped_leaf_specs_keep_their_renderers_in_nested_parsing() {
    let plugin = PluginSpec::new("group").with(A::plugin()).with(B::plugin());
    let md = MarkdownIt::builder()
        .plugin(plugin)
        .plugin(token_spec("token", '^', "C"))
        .build();
    assert_eq!(md.render("x @ 雪 ^"), "<p>x <b>A</b> <b>B</b> C</p>\n");
    assert_eq!(
        md.render("[x @ 雪 ^](/url)"),
        "<p><a href=\"/url\">x <b>A</b> <b>B</b> C</a></p>\n"
    );
}

#[test]
fn leaf_dependencies_order_rules() {
    let md = MarkdownIt::builder()
        .plugin(A::plugin().after("c"))
        .plugin(C::plugin())
        .build();
    assert_eq!(md.render("@"), "<p><b>C</b></p>\n");
}

#[test]
fn wildcard_leaf_invalidates_warmed_scanners() {
    #[derive(Debug)]
    struct Todo;

    for unicode in [false, true] {
        let mut md = MarkdownIt::new();
        if unicode {
            token_spec("unicode", '雪', "snow").install(&mut md);
        }
        assert_eq!(md.render("x TODO"), "<p>x TODO</p>\n");
        PluginSpec::new("todo")
            .inline_leaf::<Todo>("todo")
            .parse(|cx| cx.consume("TODO").then_some(Todo))
            .render_html(|_, html| {
                html.element("b").text("done");
            })
            .finish()
            .install(&mut md);
        assert_eq!(
            md.render("x TODO [TODO](/url)"),
            "<p>x <b>done</b> <a href=\"/url\"><b>done</b></a></p>\n"
        );
    }
}

#[test]
fn duplicate_leaf_specs_are_rejected_before_installation() {
    let mut md = MarkdownIt::new();
    A::plugin().install(&mut md);
    assert_panics(|| {
        PluginSpec::new("duplicate")
            .with(B::plugin())
            .with(A::plugin())
            .install(&mut md);
    });
    assert_eq!(md.render("@ 雪"), "<p><b>A</b> 雪</p>\n");
    assert_panics(|| {
        MarkdownIt::builder()
            .plugin(A::plugin())
            .plugin(A::plugin())
            .build();
    });
}

#[test]
fn same_payload_type_can_be_used_by_multiple_leaf_rules() {
    let plugin = PluginSpec::new("tags")
        .inline_leaf::<A>("at")
        .marker('@')
        .parse(|cx| cx.consume("@").then_some(A))
        .render_html(|_, html| {
            html.element("b").text("A");
        })
        .finish()
        .inline_leaf::<A>("hash")
        .marker('#')
        .parse(|cx| cx.consume("#").then_some(A))
        .render_html(|_, html| {
            html.element("b").text("A");
        })
        .finish();
    let md = MarkdownIt::builder().plugin(plugin).build();
    assert_eq!(md.render("@ #"), "<p><b>A</b> <b>A</b></p>\n");
}

#[test]
fn leaf_renderer_can_be_overridden_after_installation() {
    use markdown_it::document::Node;
    use markdown_it::render::{NodeRenderer, RenderContext, Writer};
    struct Custom;
    impl NodeRenderer<A> for Custom {
        fn render(&self, _: &Node, _: &A, _: &mut RenderContext<'_>, output: &mut Writer) {
            output.write_str("CUSTOM");
        }
    }

    let mut md = MarkdownIt::new();
    A::plugin().install(&mut md);
    let document = md.parse("@");
    assert_eq!(md.render_document(&document, "html"), "<p><b>A</b></p>\n");

    md.add_renderer::<A, _>("html", Custom);
    assert_eq!(md.render_document(&document, "html"), "<p>CUSTOM</p>\n");
}

#[test]
#[should_panic(expected = "succeeded without consuming input")]
fn leaf_rejects_success_without_consumption() {
    #[derive(Debug)]
    struct Empty;
    let plugin = PluginSpec::new("empty")
        .inline_leaf::<Empty>("empty")
        .marker('@')
        .parse(|_| Some(Empty))
        .render_html(|_, _| {})
        .finish();
    MarkdownIt::builder().plugin(plugin).build().render("@");
}

#[test]
fn custom_plugin_installer_runs() {
    struct SetLimit;
    impl Plugin for SetLimit {
        fn into_spec(self) -> PluginSpec {
            PluginSpec::custom("limit", |md| {
                md.max_nesting = 7;
            })
        }
    }

    let md = MarkdownIt::builder().plugin(SetLimit).build();
    assert_eq!(md.max_nesting, 7);
}
