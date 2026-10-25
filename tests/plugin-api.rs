use markdown_it::{
    Html,
    InlinePlugin,
    InlinePluginState,
    MarkdownIt,
    MarkdownItBuilder,
    NodeValue,
    Plugin,
    PluginSpec,
};

#[derive(Debug)]
struct Emoji {
    glyph: &'static str,
    label: &'static str,
}

const EMOJIS: &[(&str, &str, &str)] =
    &[(":rocket:", "🚀", "rocket"), (":warning:", "⚠️", "warning")];

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
        md.render_html(&document),
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
    impl markdown_it::NodeRenderer<One> for OneRenderer {
        fn render(
            &self,
            node: markdown_it::NodeRef<'_>,
            _: &One,
            context: &mut markdown_it::RenderContext<'_>,
            output: &mut markdown_it::Writer,
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
            let rest = cx.remaining();
            let end = rest.find('}')?;
            let value = &rest[..end];
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
    use markdown_it::{NodeRef, NodeRenderer, RenderContext, Writer};
    struct Custom;
    impl NodeRenderer<Tag> for Custom {
        fn render(
            &self,
            _: NodeRef<'_>,
            tag: &Tag,
            _: &mut RenderContext<'_>,
            output: &mut Writer,
        ) {
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
    let missing = PluginSpec::new("missing").after("absent");
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
            _: &mut markdown_it::InlineState<'_>,
        ) -> Option<(Option<markdown_it::NodeId>, usize)> {
            None
        }
    }
    assert_panics(|| {
        MarkdownItBuilder::with_preset(|md: &mut MarkdownIt| {
            md.inline.add_rule::<Never>().require_named("absent");
        })
        .build();
    });
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

macro_rules! static_tag {
    ($name:ident, $rule:literal, $marker:literal, $output:literal) => {
        #[derive(Debug)]
        struct $name;

        impl InlinePlugin for $name {
            const NAME: &'static str = $rule;
            const MARKER: char = $marker;

            fn parse(cx: &mut InlinePluginState<'_>) -> Option<Self> {
                (cx.consume_char()? == Self::MARKER).then_some(Self)
            }

            fn render_html(&self, html: &mut Html) {
                html.element("b").text($output);
            }
        }
    };
}

static_tag!(A, "a", '@', "A");
static_tag!(B, "b", '雪', "B");
#[derive(Debug)]
struct Todo;
impl InlinePlugin for Todo {
    const NAME: &'static str = "todo";
    const MARKER: char = '\0';

    fn parse(cx: &mut InlinePluginState<'_>) -> Option<Self> {
        cx.consume("TODO").then_some(Self)
    }

    fn render_html(&self, html: &mut Html) {
        html.element("b").text("done");
    }
}
static_tag!(C, "c", '@', "C");

#[test]
fn static_leaves_use_low_level_identity_and_keep_their_renderers() {
    use markdown_it::LeafRule;

    let plugin = PluginSpec::new("static").with(A).with(B);
    let mut md = MarkdownIt::builder()
        .plugin(plugin)
        .plugin(token_spec("dynamic", '^', "C"))
        .build();
    assert!(md.inline.has_rule::<LeafRule<A>>());
    assert!(md.inline.has_rule::<LeafRule<B>>());
    assert_eq!(md.render("x @ 雪 ^"), "<p>x <b>A</b> <b>B</b> C</p>\n");
    assert_eq!(
        md.render("[x @ 雪 ^](/url)"),
        "<p><a href=\"/url\">x <b>A</b> <b>B</b> C</a></p>\n"
    );
    md.inline.remove_rule::<LeafRule<A>>();
    assert!(!md.inline.has_rule::<LeafRule<A>>());
    assert_eq!(md.render("@ 雪 ^"), "<p>@ <b>B</b> C</p>\n");
}

#[test]
fn static_dependencies_order_the_low_level_rules() {
    let md = MarkdownIt::builder()
        .plugin(PluginSpec::new("a").with(A).after("c"))
        .plugin(PluginSpec::new("c").with(C))
        .build();
    assert_eq!(md.render("@"), "<p><b>C</b></p>\n");
}

#[test]
fn static_wildcard_invalidates_warmed_scanners_and_can_be_removed() {
    use markdown_it::LeafRule;
    for unicode in [false, true] {
        let mut md = MarkdownIt::new();
        if unicode {
            token_spec("unicode", '雪', "snow").install(&mut md);
        }
        assert_eq!(md.render("x TODO"), "<p>x TODO</p>\n");
        PluginSpec::new("todo").with(Todo).install(&mut md);
        assert_eq!(
            md.render("x TODO [TODO](/url)"),
            "<p>x <b>done</b> <a href=\"/url\"><b>done</b></a></p>\n"
        );
        md.inline.remove_rule::<LeafRule<Todo>>();
        assert_eq!(md.render("x TODO"), "<p>x TODO</p>\n");
    }
}

#[test]
fn duplicate_static_leaves_are_rejected_before_installation() {
    let mut md = MarkdownIt::new();
    PluginSpec::new("a").with(A).install(&mut md);
    assert_panics(|| {
        PluginSpec::new("duplicate")
            .with(B)
            .with(A)
            .install(&mut md);
    });
    assert_eq!(md.render("@ 雪"), "<p><b>A</b> 雪</p>\n");
    assert_panics(|| {
        MarkdownIt::builder()
            .plugin(PluginSpec::new("a").with(A))
            .plugin(PluginSpec::new("a2").with(A))
            .build();
    });
}

#[test]
fn static_renderer_can_be_overridden_after_installation() {
    use markdown_it::{NodeRef, NodeRenderer, RenderContext, Writer};
    struct Custom;
    impl NodeRenderer<A> for Custom {
        fn render(&self, _: NodeRef<'_>, _: &A, _: &mut RenderContext<'_>, output: &mut Writer) {
            output.write_str("CUSTOM");
        }
    }

    let mut md = MarkdownIt::new();
    PluginSpec::new("static").with(A).install(&mut md);
    let document = md.parse("@");
    assert_eq!(md.render_html(&document), "<p><b>A</b></p>\n");

    md.add_renderer::<A, _>("html", Custom);
    assert_eq!(md.render_html(&document), "<p>CUSTOM</p>\n");
}

#[test]
#[should_panic(expected = "succeeded without consuming input")]
fn static_leaf_rejects_success_without_consumption() {
    #[derive(Debug)]
    struct Empty;
    impl InlinePlugin for Empty {
        const NAME: &'static str = "empty";
        const MARKER: char = '@';

        fn parse(_: &mut InlinePluginState<'_>) -> Option<Self> {
            Some(Empty)
        }

        fn render_html(&self, _: &mut Html) {}
    }

    MarkdownIt::builder().plugin(Empty).build().render("@");
}

#[test]
fn static_leaves_can_be_registered_with_only_low_level_apis() {
    use markdown_it::{LeafRenderer, LeafRule};

    let mut md = MarkdownIt::new();
    md.inline
        .add_rule::<LeafRule<A>>()
        .after_named("text")
        .before_all();
    md.inline
        .add_rule::<LeafRule<B>>()
        .after_named("text")
        .before::<LeafRule<A>>()
        .before_all();
    md.add_renderer::<A, _>("html", LeafRenderer::<A>::default());
    md.add_renderer::<B, _>("html", LeafRenderer::<B>::default());
    assert_eq!(md.render("x 雪"), "<p>x <b>B</b></p>\n");
    md.inline.remove_rule::<LeafRule<B>>();
    assert_eq!(md.render("x 雪"), "<p>x 雪</p>\n");
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
