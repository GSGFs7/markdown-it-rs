use super::*;
use crate::MarkdownIt;
use crate::document::NodeValue;

#[derive(Debug, PartialEq, Eq)]
struct Word(String);

#[derive(Debug)]
struct Plain;

impl Plain {
    fn plugin() -> PluginSpec {
        configured_leaf("plain", '@', Self::parse, Self::render_html)
    }

    fn parse(cx: &mut InlinePluginState<'_>) -> Option<Self> {
        cx.consume("@plain").then_some(Plain)
    }

    fn render_html(&self, html: &mut Html) {
        html.element("i").text("plain");
    }
}

impl Word {
    fn plugin() -> PluginSpec {
        configured_leaf("word", '@', Self::parse, Self::render_html)
    }

    fn parse(cx: &mut InlinePluginState<'_>) -> Option<Self> {
        let rest = cx.remaining().strip_prefix('@')?;
        let end = rest
            .find(|ch: char| !ch.is_ascii_alphabetic())
            .unwrap_or(rest.len());
        let word = cx.remaining_slice(1..end + 1)?.to_owned();
        cx.advance(end + 1).then_some(Word(word))
    }

    fn render_html(&self, html: &mut Html) {
        html.element("span").class("word").text(&self.0);
    }
}

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
fn installs_inline_leaf_and_renderer() {
    let md = MarkdownIt::builder().plugin(Word::plugin()).build();
    assert_eq!(
        md.render("say @hello!"),
        "<p>say <span class=\"word\">hello</span>!</p>\n"
    );
}

#[test]
fn same_local_inline_name_in_child_plugins_has_independent_ordering() {
    use crate::parser::inline::{InlineRule, InlineState};

    #[derive(Debug)]
    struct Leaf;
    #[derive(Debug)]
    struct Container;
    struct Late;
    impl InlineRule for Late {
        const MARKER: char = '@';
        fn run(_: &mut InlineState<'_>) -> Option<(Option<crate::document::NodeId>, usize)> {
            None
        }
    }

    for container_first in [false, true] {
        let leaf = PluginSpec::new("leaf")
            .inline_leaf::<Leaf>("run")
            .marker('@')
            .parse(|cx| cx.consume("@").then_some(Leaf))
            .render_html(|_, html| {
                html.text("leaf");
            })
            .finish();
        let container = PluginSpec::new("container")
            .inline_container::<Container>("run")
            .marker('@')
            .parse(|cx| cx.consume("@").then_some((Container, 1..1)))
            .html_tag("span")
            .finish();
        let (group, delayed, before, after) = if container_first {
            (
                PluginSpec::new("group").with(container).with(leaf),
                "container",
                "<p><span></span></p>\n",
                "<p>leaf</p>\n",
            )
        } else {
            (
                PluginSpec::new("group").with(leaf).with(container),
                "leaf",
                "<p>leaf</p>\n",
                "<p><span></span></p>\n",
            )
        };
        let mut md = MarkdownIt::builder().plugin(group).build();
        assert_eq!(md.render("@"), before);

        // Internal test of registration isolation, not a public named-ordering
        // API. Only the selected plugin is delayed; its sibling can now win.
        md.inline
            .add_rule::<Late>()
            .before_mark(super::identity::rule_mark("inline", delayed, "run"));
        assert_eq!(md.render("@"), after);
    }
}

#[test]
#[should_panic(expected = "was added more than once")]
fn local_rule_names_are_unique_across_syntax_and_transforms() {
    #[derive(Debug)]
    struct Item;
    let plugin = PluginSpec::new("duplicate")
        .inline_leaf::<Item>("run")
        .parse(|_| None)
        .render_text(|_, _| {})
        .finish()
        .transform("run", |_, _| {});
    MarkdownIt::builder().plugin(plugin).build();
}

#[test]
#[should_panic(expected = "was added more than once")]
fn same_name_plugin_merge_shares_the_local_rule_namespace() {
    #[derive(Debug)]
    struct Item;
    let child = PluginSpec::new("merged")
        .block_leaf::<Item>("run")
        .parse(|_| None)
        .render_text(|_, _| {})
        .finish();
    let plugin = PluginSpec::new("merged")
        .transform("run", |_, _| {})
        .with(child);
    MarkdownIt::builder().plugin(plugin).build();
}

#[test]
fn same_local_pair_names_preserve_shared_scanner_lengths() {
    #[derive(Debug)]
    struct Single;
    #[derive(Debug)]
    struct Double;
    let single = PluginSpec::new("single")
        .inline_pair::<Single>("run")
        .delimiter::<'|', 1, true>()
        .value(|| Single)
        .html_tag("i")
        .finish();
    let double = PluginSpec::new("double")
        .inline_pair::<Double>("run")
        .delimiter::<'|', 2, true>()
        .value(|| Double)
        .html_tag("b")
        .finish();
    let md = MarkdownIt::builder()
        .plugin(PluginSpec::new("group").with(single).with(double))
        .build();
    assert_eq!(md.render("|one| ||two||"), "<p><i>one</i> <b>two</b></p>\n");
}

#[test]
fn leaf_spec_can_be_registered_directly() {
    let md = MarkdownIt::builder().plugin(Plain::plugin()).build();
    assert_eq!(md.render("say @plain!"), "<p>say <i>plain</i>!</p>\n");
}

#[test]
fn leaf_specs_can_be_registered_together() {
    #[derive(Debug)]
    struct Bang;

    let bang = configured_leaf::<Bang, _, _>(
        "bang",
        '!',
        |cx| cx.consume("!").then_some(Bang),
        |_, html| {
            html.element("b").text("bang");
        },
    );

    let md = MarkdownIt::builder()
        .plugin(Word::plugin())
        .plugin(bang)
        .build();
    assert_eq!(
        md.render("@hello!"),
        "<p><span class=\"word\">hello</span><b>bang</b></p>\n"
    );
}

#[test]
fn plugin_spec_groups_leaf_specs() {
    #[derive(Debug)]
    struct Bang;

    let bang = configured_leaf::<Bang, _, _>(
        "bang",
        '!',
        |cx| cx.consume("!").then_some(Bang),
        |_, html| {
            html.element("b").text("bang");
        },
    );

    let group = PluginSpec::new("group").with(Plain::plugin()).with(bang);
    let md = MarkdownIt::builder().plugin(group).build();
    assert_eq!(md.render("@plain!"), "<p><i>plain</i><b>bang</b></p>\n");
}

#[test]
fn failed_parse_rolls_back_consumption() {
    let leaf = configured_leaf::<Word, _, _>(
        "rollback",
        '@',
        |cx| {
            if !cx.consume("@") {
                return None;
            }
            if cx.consume("ok") {
                Some(Word("ok".into()))
            } else {
                None
            }
        },
        |word, html| {
            html.element("b").text(&word.0);
        },
    );

    let md = MarkdownIt::builder().plugin(leaf).build();
    assert_eq!(md.render("@nope @ok"), "<p>@nope <b>ok</b></p>\n");
}

#[test]
fn html_text_and_attributes_are_escaped() {
    let leaf = configured_leaf::<Word, _, _>(
        "escape",
        '@',
        |cx| {
            cx.consume("@");
            Some(Word("<&\"".into()))
        },
        |word, html| {
            html.element("span").attr("title", &word.0).text(&word.0);
        },
    );

    let md = MarkdownIt::builder().plugin(leaf).build();
    assert_eq!(
        md.render("@"),
        "<p><span title=\"&lt;&amp;&quot;\">&lt;&amp;&quot;</span></p>\n"
    );
}

#[test]
#[should_panic(expected = "was added more than once")]
fn duplicate_plugins_panic() {
    MarkdownIt::builder()
        .plugin(PluginSpec::new("same"))
        .plugin(PluginSpec::new("same"))
        .build();
}

#[test]
fn nested_plugin_specs_keep_their_dependencies() {
    let later = configured_leaf::<Word, _, _>(
        "later",
        '@',
        |cx| cx.consume("@").then_some(Word("LATER".into())),
        |word, html| {
            html.text(&word.0);
        },
    );
    let first = configured_leaf::<Word, _, _>(
        "first",
        '@',
        |cx| cx.consume("@").then_some(Word("FIRST".into())),
        |word, html| {
            html.text(&word.0);
        },
    );

    let group = PluginSpec::new("group")
        .with(PluginSpec::new("later").with(later).after("first"))
        .with(PluginSpec::new("first").with(first));

    let md = MarkdownIt::builder().plugin(group).build();
    assert_eq!(md.render("@"), "<p>FIRST</p>\n");
}

#[test]
fn void_html_elements_respect_xhtml_out() {
    let br = configured_leaf::<Word, _, _>(
        "void",
        '^',
        |cx| cx.consume("^").then_some(Word("x".into())),
        |_, html| {
            html.element("br").void();
        },
    );

    let mut md = MarkdownIt::builder().plugin(br).build();
    assert_eq!(md.render("^"), "<p><br></p>\n");

    md.render_options.xhtml_out = true;
    assert_eq!(md.render("^"), "<p><br /></p>\n");
}

#[test]
fn nested_html_elements_are_ordered() {
    let leaf = configured_leaf::<Word, _, _>(
        "nested",
        '@',
        |cx| {
            cx.consume("@");
            Some(Word("x".into()))
        },
        |_, html| {
            html.element("div")
                .class("outer")
                .text("a")
                .element("span")
                .class("inner")
                .text("b");
        },
    );

    let md = MarkdownIt::builder().plugin(leaf).build();
    assert_eq!(
        md.render("@"),
        "<p><div class=\"outer\">a<span class=\"inner\">b</span></div></p>\n"
    );
}

#[derive(Debug)]
struct Span;

fn span() -> PluginSpec {
    PluginSpec::new("span")
        .inline_container::<Span>("span")
        .marker('{')
        .parse(|cx| {
            let source = cx.source();
            if !source.starts_with('{') {
                return None;
            }
            let end = source.find('}')?;
            cx.advance(end + 1);
            Some((Span, 1..end))
        })
        .html_tag("span")
        .finish()
}

#[test]
fn configured_inline_container_parses_children_and_wraps_them() {
    let md = MarkdownIt::builder().plugin(span()).build();
    assert_eq!(
        md.render("前{ *雪* `x` }后"),
        "<p>前<span> <em>雪</em> <code>x</code> </span>后</p>\n"
    );
    // A failed match rolls back local consumption.
    assert_eq!(md.render("{unterminated"), "<p>{unterminated</p>\n");
}

#[test]
fn failed_inline_container_match_does_not_leave_child_nodes() {
    #[derive(Debug)]
    struct Reject;

    let reject = PluginSpec::new("reject")
        .inline_container::<Reject>("reject")
        .marker('{')
        .parse(|cx| {
            let source = cx.source();
            if !source.starts_with('{') {
                return None;
            }
            let end = source.find('}')?;
            if cx.source_slice(1..end)?.contains("no") {
                return None;
            }
            cx.advance(end + 1);
            Some((Reject, 1..end))
        })
        .html_tag("span")
        .finish();

    let with_plugin = MarkdownIt::builder().plugin(reject).build().parse("{no}");
    let baseline = MarkdownIt::builder().build().parse("{no}");
    assert_eq!(with_plugin.len(), baseline.len());
    assert_eq!(with_plugin.children(with_plugin.root()).len(), 1);
}

#[test]
fn configured_inline_container_can_render_children_in_callbacks() {
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
            let end = source.find("]")?;
            cx.advance(end + 1);
            Some((Boxed("note".into()), 2..end))
        })
        .render_html(|value, html| {
            let children = html.render_children();
            let mut element = html.element("span");
            element
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
fn configured_inline_container_accepts_empty_child_range() {
    let md = MarkdownIt::builder().plugin(span()).build();
    assert_eq!(md.render("{}"), "<p><span></span></p>\n");
}

#[test]
fn inline_container_cursor_rejects_utf8_splits_and_out_of_bounds() {
    #[derive(Debug)]
    struct Bound;

    let bound = PluginSpec::new("bound")
        .inline_container::<Bound>("bound")
        .marker('{')
        .parse(|cx| {
            let source = cx.source();
            if !source.starts_with('{') {
                return None;
            }

            assert!(cx.advance(1));
            assert_eq!(cx.consumed(), 1);
            // Splitting the UTF-8 encoding of 雪 must fail without moving.
            assert!(!cx.advance(1));
            assert_eq!(cx.consumed(), 1);
            assert!(!cx.advance(usize::MAX));
            assert_eq!(cx.consumed(), 1);

            assert!(cx.advance('雪'.len_utf8()));
            assert!(cx.consume("}"));
            Some((Bound, 1..4))
        })
        .html_tag("span")
        .finish();

    let md = MarkdownIt::builder().plugin(bound).build();
    assert_eq!(md.render("{雪}"), "<p><span>雪</span></p>\n");
}

fn range_plugin(range: std::ops::Range<usize>) -> PluginSpec {
    PluginSpec::new("range")
        .inline_container::<Span>("range")
        .marker('{')
        .parse(move |cx| {
            let source = cx.source();
            if !source.starts_with('{') {
                return None;
            }
            cx.advance(1);
            Some((Span, range.clone()))
        })
        .html_tag("span")
        .finish()
}

#[test]
#[should_panic(expected = "invalid child range")]
fn inline_container_rejects_reversed_child_range() {
    MarkdownIt::builder()
        .plugin(range_plugin(std::ops::Range { start: 2, end: 1 }))
        .build()
        .render("{x}");
}

#[test]
#[should_panic(expected = "invalid child range")]
fn inline_container_rejects_out_of_bounds_child_range() {
    MarkdownIt::builder()
        .plugin(range_plugin(0..100))
        .build()
        .render("{x}");
}

#[test]
#[should_panic(expected = "invalid child range")]
fn inline_container_rejects_overflow_sized_child_range() {
    MarkdownIt::builder()
        .plugin(range_plugin(1..usize::MAX))
        .build()
        .render("{x}");
}

#[test]
#[should_panic(expected = "succeeded without consuming input")]
fn configured_inline_container_rejects_zero_length_success() {
    let empty = PluginSpec::new("empty")
        .inline_container::<Span>("empty")
        .marker('{')
        .parse(|_| Some((Span, 0..0)))
        .html_tag("span")
        .finish();

    MarkdownIt::builder().plugin(empty).build().render("{");
}

#[test]
fn inline_container_check_skips_child_source_during_link_scanning() {
    let md = MarkdownIt::builder().plugin(span()).build();
    // The `]` inside the container must not close the link label while the
    // link scanner probes the source.
    assert_eq!(
        md.render("[{a]b}](/url)"),
        "<p><a href=\"/url\"><span>a]b</span></a></p>\n"
    );
}

#[cfg(feature = "sourcemap")]
#[test]
fn inline_container_keeps_source_maps_of_container_and_children() {
    let document = MarkdownIt::builder()
        .plugin(span())
        .build()
        .parse("前{雪}后");
    let paragraph = document.children(document.root())[0];
    let container = document.children(paragraph)[1];
    let child = document.children(container)[0];
    assert_eq!(
        document
            .node(container)
            .srcmap()
            .unwrap()
            .get_byte_offsets(),
        (3, 8)
    );
    assert_eq!(
        document.node(child).srcmap().unwrap().get_byte_offsets(),
        (4, 7)
    );
}

#[derive(Debug)]
struct Notice(String);

fn notice(interrupts_paragraph: bool) -> PluginSpec {
    PluginSpec::new("notice")
        .block_leaf::<Notice>("notice")
        .interrupts_paragraph(interrupts_paragraph)
        .parse(|cx| {
            let text = cx.line(0)?.strip_prefix("!!! ")?.to_owned();
            cx.advance_lines(1);
            Some(Notice(text))
        })
        .render_html(|notice, html| {
            html.element("aside").class("notice").text(&notice.0);
        })
        .finish()
}

#[test]
fn configured_block_leaf_parses_and_renders() {
    let md = MarkdownIt::builder().plugin(notice(false)).build();
    assert_eq!(
        md.render("before\n\n!!! hello\nafter"),
        "<p>before</p>\n<aside class=\"notice\">hello</aside>\n<p>after</p>\n"
    );
    assert_eq!(
        md.render("!!! hello"),
        "<aside class=\"notice\">hello</aside>\n"
    );
}

#[test]
fn configured_block_leaf_interrupts_paragraph_only_when_enabled() {
    let without = MarkdownIt::builder().plugin(notice(false)).build();
    assert_eq!(
        without.render("before\n!!! hello"),
        "<p>before\n!!! hello</p>\n"
    );

    let with = MarkdownIt::builder().plugin(notice(true)).build();
    assert_eq!(
        with.render("before\n!!! hello"),
        "<p>before</p>\n<aside class=\"notice\">hello</aside>\n"
    );
}

#[test]
fn configured_block_leaf_can_consume_multiple_lines() {
    #[derive(Debug)]
    struct Fence(String);

    let fence = PluginSpec::new("fence")
        .block_leaf::<Fence>("fence")
        .parse(|cx| {
            if cx.line(0)? != "%%" {
                return None;
            }
            let mut body = Vec::new();
            let mut lines = 1;
            while let Some(line) = cx.line(lines) {
                if line == "%%" {
                    cx.advance_lines(lines + 1);
                    return Some(Fence(body.join("\n")));
                }
                body.push(line);
                lines += 1;
            }
            None
        })
        .render_html(|fence, html| {
            html.element("pre").text(&fence.0);
        })
        .finish();

    let md = MarkdownIt::builder().plugin(fence).build();
    assert_eq!(
        md.render("%%\nhello\nworld\n%%\nafter"),
        "<pre>hello\nworld</pre>\n<p>after</p>\n"
    );
    assert_eq!(md.render("%%\nunterminated"), "<p>%%\nunterminated</p>\n");

    // A multi-line leaf must not consume outdented lines inside a list item.
    assert_eq!(
        md.render("- %%\n  inside\n\noutside\n%%"),
        "<ul>\n<li>%%\ninside</li>\n</ul>\n<p>outside\n%%</p>\n"
    );
}

#[test]
fn configured_block_leaf_check_rejects_indented_code_like_builtin_rules() {
    let source = "> before\n    !!! hello";
    let plain = MarkdownIt::builder().build();
    let with_notice = MarkdownIt::builder().plugin(notice(true)).build();

    // The indented line is a lazy paragraph continuation, not a notice that
    // ends the blockquote.
    assert_eq!(with_notice.render(source), plain.render(source));
}

#[test]
#[should_panic(expected = "succeeded without consuming input")]
fn configured_block_leaf_rejects_zero_line_success() {
    let leaf = PluginSpec::new("empty")
        .block_leaf::<Notice>("empty")
        .parse(|_| Some(Notice("empty".into())))
        .render_html(|_, html| {
            html.text("empty");
        })
        .finish();

    MarkdownIt::builder()
        .plugin(leaf)
        .build()
        .render("anything");
}

#[derive(Debug)]
#[allow(dead_code)]
struct Admonition(String);

fn admonition(interrupts_paragraph: bool) -> PluginSpec {
    PluginSpec::new("admonition")
        .block_container::<Admonition>("admonition")
        .interrupts_paragraph(interrupts_paragraph)
        .parse(|cx| {
            let title = cx.line(0)?.strip_prefix(":::")?.trim().to_owned();
            let close = (1..cx.remaining_lines()).find(|&line| cx.line(line) == Some(":::"));
            let body_end = close.unwrap_or(cx.remaining_lines());
            Some(
                BlockContainerParse::new(Admonition(title), body_end - 1)
                    .close_lines(usize::from(close.is_some())),
            )
        })
        .html_tag("div")
        .finish()
}

#[test]
fn configured_block_container_parses_body_as_blocks() {
    let md = MarkdownIt::builder().plugin(admonition(false)).build();
    assert_eq!(
        md.render("::: note\nbody *here*\n:::"),
        "<div>\n<p>body <em>here</em></p>\n</div>\n"
    );
}

#[test]
fn configured_block_container_accepts_nested_blocks_and_following_content() {
    let md = MarkdownIt::builder().plugin(admonition(false)).build();
    let html = md.render("::: note\n- one\n- two\n\n> quote\n\n```\ncode\n```\n:::\nafter");

    assert!(html.contains("<div>"));
    assert!(html.contains("<ul>"));
    assert!(html.contains("<blockquote>"));
    assert!(html.contains("<pre><code>code\n</code></pre>"));
    assert!(html.ends_with("</div>\n<p>after</p>\n"));
}

#[test]
fn configured_block_container_supports_unclosed_body() {
    let md = MarkdownIt::builder().plugin(admonition(false)).build();
    assert_eq!(
        md.render("::: note\nbody\nlast"),
        "<div>\n<p>body\nlast</p>\n</div>\n"
    );
}

#[test]
fn configured_block_container_works_inside_list_items() {
    let md = MarkdownIt::builder().plugin(admonition(false)).build();
    let html = md.render("- ::: note\n  body\n  :::\n\nafter");

    assert!(html.contains("<li>"));
    assert!(html.contains("<div>\n<p>body</p>\n</div>"));
    assert!(html.ends_with("<p>after</p>\n"));
}

#[test]
fn configured_block_container_interrupts_paragraph_only_when_enabled() {
    let without = MarkdownIt::builder().plugin(admonition(false)).build();
    assert_eq!(
        without.render("before\n::: note\nbody\n:::"),
        "<p>before\n::: note\nbody\n:::</p>\n"
    );

    let with = MarkdownIt::builder().plugin(admonition(true)).build();
    assert_eq!(
        with.render("before\n::: note\nbody\n:::"),
        "<p>before</p>\n<div>\n<p>body</p>\n</div>\n"
    );
}

#[test]
fn configured_block_container_strips_content_indent() {
    #[derive(Debug)]
    struct Indented;

    let container = PluginSpec::new("indented")
        .block_container::<Indented>("indented")
        .parse(|cx| {
            if cx.line(0)? != "%%%" {
                return None;
            }
            let close = (1..cx.remaining_lines()).find(|&line| cx.line(line) == Some("%%%"))?;
            Some(
                BlockContainerParse::new(Indented, close - 1)
                    .close_lines(1)
                    .content_indent(2),
            )
        })
        .html_tag("div")
        .finish();

    let md = MarkdownIt::builder().plugin(container).build();
    assert_eq!(
        md.render("%%%\n    body\n%%%"),
        "<div>\n<p>body</p>\n</div>\n"
    );
    // A body line with less indentation must still be parsed rather than be
    // silently swallowed when nested tokenization sees an outdent.
    assert_eq!(md.render("%%%\nbody\n%%%"), "<div>\n<p>body</p>\n</div>\n");
}

#[test]
fn block_container_and_leaf_share_installation_order() {
    #[derive(Debug)]
    struct First;
    #[derive(Debug)]
    struct Second;

    let first = PluginSpec::new("first")
        .block_container::<First>("first")
        .parse(|cx| {
            cx.line(0)?
                .starts_with("!!")
                .then_some(BlockContainerParse::new(First, 0))
        })
        .render_html(|_, html| {
            html.element("p").class("first").text("first");
        })
        .finish();

    let second = PluginSpec::new("second")
        .block_container::<Second>("second")
        .parse(|cx| {
            cx.line(0)?
                .starts_with("!!")
                .then_some(BlockContainerParse::new(Second, 0))
        })
        .render_html(|_, html| {
            html.element("p").class("second").text("second");
        })
        .finish();

    let md = MarkdownIt::builder().plugin(first).plugin(second).build();
    assert_eq!(md.render("!! marker"), "<p class=\"first\">first</p>\n");
}

#[test]
#[should_panic(expected = "consumed past the source window")]
fn configured_block_container_rejects_layout_past_outdent_window() {
    #[derive(Debug)]
    struct CrossOutdent;

    let container = PluginSpec::new("cross-outdent")
        .block_container::<CrossOutdent>("cross-outdent")
        .parse(|cx| {
            if cx.line(0)? != "%%%" {
                return None;
            }
            // The block window inside the list item ends before `outside`.
            // These line counts would pass against state.line_max but cross the
            // outdent and swallow the following blocks.
            Some(BlockContainerParse::new(CrossOutdent, 3))
        })
        .html_tag("div")
        .finish();

    MarkdownIt::builder()
        .plugin(container)
        .build()
        .render("- %%%\n  body\n\noutside\nlast");
}

#[test]
#[should_panic(expected = "must consume at least one opening line")]
fn configured_block_container_rejects_zero_opening_lines() {
    #[derive(Debug)]
    struct Empty;

    let container = PluginSpec::new("empty")
        .block_container::<Empty>("empty")
        .parse(|_| Some(BlockContainerParse::new(Empty, 1).open_lines(0)))
        .html_tag("div")
        .finish();

    MarkdownIt::builder()
        .plugin(container)
        .build()
        .render("anything");
}

#[derive(Debug)]
struct Highlight;

#[test]
fn configured_inline_pair_parses_and_renders_children() {
    let highlight = PluginSpec::new("highlight")
        .inline_pair::<Highlight>("highlight")
        .delimiter::<'=', 2, true>()
        .value(|| Highlight)
        .html_tag("mark")
        .finish();

    let md = MarkdownIt::builder().plugin(highlight).build();
    assert_eq!(
        md.render("a ==marked **bold**=="),
        "<p>a <mark>marked <strong>bold</strong></mark></p>\n"
    );
}

#[test]
fn configured_inline_pair_html_tag_preserves_node_attributes() {
    #[derive(Debug)]
    struct Highlight;

    let highlight = PluginSpec::new("highlight")
        .inline_pair::<Highlight>("highlight")
        .delimiter::<'=', 2, true>()
        .value(|| Highlight)
        .html_tag("mark")
        .finish()
        .transform("attrs", |document, _| {
            let paragraph = document.children(document.root())[0];
            let pair = document
                .children(paragraph)
                .iter()
                .copied()
                .find(|&node| document.node(node).is::<Highlight>())
                .unwrap();
            document
                .node_mut(pair)
                .attrs_mut()
                .push(("class".into(), "highlight".into()));
        });

    let md = MarkdownIt::builder().plugin(highlight).build();
    assert_eq!(
        md.render("a ==x=="),
        "<p>a <mark class=\"highlight\">x</mark></p>\n"
    );
}

#[test]
fn configured_inline_pair_supports_html_and_text_callbacks() {
    #[derive(Debug)]
    struct Marked(String);

    fn append_text<W: std::fmt::Write>(output: &mut W, value: &str) {
        write!(output, "{value}").unwrap();
    }

    let marked = PluginSpec::new("pair-callback")
        .inline_pair::<Marked>("pair-callback")
        .delimiter::<'=', 2, true>()
        .value(|| Marked("x".into()))
        .render_html(|value, html| {
            let children = html.render_children();
            html.element("mark")
                .class("pair")
                .attr("data-value", &value.0)
                .raw(children);
        })
        .render_text(|_, text| {
            append_text(text, "[");
            text.render_children();
            text.write_fmt(format_args!("]"));
        })
        .finish();

    let md = MarkdownIt::builder().plugin(marked).build();
    let document = md.parse("a ==**b**==");
    assert_eq!(
        md.render_document(&document, "html"),
        "<p>a <mark class=\"pair\" data-value=\"x\"><strong>b</strong></mark></p>\n"
    );
    assert_eq!(md.render_document(&document, "text"), "a [b]\n");
}

#[test]
fn configured_inline_pair_honors_can_split_word() {
    #[derive(Debug)]
    struct Placeholder;

    let placeholder = PluginSpec::new("placeholder")
        .inline_pair::<Placeholder>("placeholder")
        .delimiter::<'^', 1, false>()
        .value(|| Placeholder)
        .html_tag("span")
        .finish();

    let md = MarkdownIt::builder().plugin(placeholder).build();
    assert_eq!(
        md.render("foo^bar^baz and ^ok^"),
        "<p>foo^bar^baz and <span>ok</span></p>\n"
    );
}

#[test]
fn configured_inline_pair_shares_marker_between_lengths() {
    #[derive(Debug)]
    struct One;

    #[derive(Debug)]
    struct Two;

    let one = PluginSpec::new("one")
        .inline_pair::<One>("one")
        .delimiter::<'+', 1, true>()
        .value(|| One)
        .html_tag("i")
        .finish();
    let two = PluginSpec::new("two")
        .inline_pair::<Two>("two")
        .delimiter::<'+', 2, true>()
        .value(|| Two)
        .html_tag("b")
        .finish();

    let md = MarkdownIt::builder().plugin(one).plugin(two).build();
    assert_eq!(
        md.render("+one+ and ++two++"),
        "<p><i>one</i> and <b>two</b></p>\n"
    );
}

#[test]
fn configured_inline_pair_reuses_existing_static_marker_scanner() {
    #[derive(Debug)]
    struct Triple;

    let triple = PluginSpec::new("triple")
        .inline_pair::<Triple>("triple")
        .delimiter::<'*', 3, true>()
        .value(|| Triple)
        .html_tag("mark")
        .finish();

    // The default preset already has a static `*` scanner for emphasis and
    // strong. The new length must still be reachable through that scanner.
    let md = MarkdownIt::builder().plugin(triple).build();
    assert_eq!(md.render("***hello***"), "<p><mark>hello</mark></p>\n");
    assert_eq!(
        md.render("*em* **strong** ***mark***"),
        "<p><em>em</em> <strong>strong</strong> <mark>mark</mark></p>\n"
    );
}

#[test]
#[should_panic(expected = "length 2 is already registered")]
fn configured_inline_pair_rejects_duplicate_marker_length() {
    #[derive(Debug)]
    struct First;

    #[derive(Debug)]
    struct Second;

    let first = PluginSpec::new("first")
        .inline_pair::<First>("first")
        .delimiter::<'=', 2, true>()
        .value(|| First)
        .html_tag("mark")
        .finish();
    let second = PluginSpec::new("second")
        .inline_pair::<Second>("second")
        .delimiter::<'=', 2, true>()
        .value(|| Second)
        .html_tag("b")
        .finish();

    MarkdownIt::builder().plugin(first).plugin(second).build();
}

#[test]
#[should_panic(expected = "can_split_word must agree")]
fn configured_inline_pair_rejects_can_split_word_conflict() {
    #[derive(Debug)]
    struct First;

    #[derive(Debug)]
    struct Second;

    let first = PluginSpec::new("first")
        .inline_pair::<First>("first")
        .delimiter::<'=', 1, true>()
        .value(|| First)
        .html_tag("mark")
        .finish();
    let second = PluginSpec::new("second")
        .inline_pair::<Second>("second")
        .delimiter::<'=', 2, false>()
        .value(|| Second)
        .html_tag("b")
        .finish();

    MarkdownIt::builder().plugin(first).plugin(second).build();
}

#[test]
#[should_panic(expected = "marker '*' length 1 is already registered")]
fn configured_inline_pair_rejects_conflict_with_builtin_pair() {
    #[derive(Debug)]
    struct Star;

    let star = PluginSpec::new("star")
        .inline_pair::<Star>("star")
        .delimiter::<'*', 1, true>()
        .value(|| Star)
        .html_tag("mark")
        .finish();

    MarkdownIt::builder().plugin(star).build();
}

#[test]
fn transform_runs_after_inline_and_can_rewrite_document() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use crate::document::Text;
    use crate::plugins::cmark::inline::emphasis::Em;

    let observed = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&observed);
    let plugin = PluginSpec::new("prefix").transform("prefix", move |document, _md| {
        let paragraph = document.children(document.root())[0];
        let first = document.children(paragraph)[0];
        assert!(document.node(first).is::<Em>());
        flag.store(true, Ordering::SeqCst);

        let prefix = document.create_node(Text {
            content: "> ".into(),
        });
        document.insert_before(first, prefix);
    });

    let md = MarkdownIt::builder().plugin(plugin).build();
    assert_eq!(md.render("*x*"), "<p>&gt; <em>x</em></p>\n");
    assert!(observed.load(Ordering::SeqCst));
}

#[test]
fn transform_order_follows_plugin_dependencies_and_namespaces_marks() {
    use std::sync::{Arc, Mutex};

    let order = Arc::new(Mutex::new(Vec::new()));

    let first_order = Arc::clone(&order);
    let first = PluginSpec::new("first").transform("run", move |_, _| {
        first_order.lock().unwrap().push("first");
    });

    let second_order = Arc::clone(&order);
    let second = PluginSpec::new("second")
        .transform("run", move |_, _| {
            second_order.lock().unwrap().push("second");
        })
        .after("first");

    let group = PluginSpec::new("group").with(second).with(first);
    let md = MarkdownIt::builder().plugin(group).build();
    md.parse("text");

    assert_eq!(*order.lock().unwrap(), ["first", "second"]);
}

#[test]
fn transform_does_not_require_the_inline_rule() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use crate::parser::inline::InlineParserRule;

    let ran = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&ran);
    let plugin = PluginSpec::new("rewrite").transform("rewrite", move |_, _| {
        flag.store(true, Ordering::SeqCst);
    });

    let mut md = MarkdownIt::builder().plugin(plugin).build();
    md.remove_rule::<InlineParserRule>();
    md.parse("text");

    assert!(ran.load(Ordering::SeqCst));
}

#[test]
#[should_panic(expected = "was added more than once")]
fn duplicate_transform_names_in_one_plugin_panic() {
    let plugin = PluginSpec::new("duplicate")
        .transform("same", |_, _| {})
        .transform("same", |_, _| {});

    MarkdownIt::builder().plugin(plugin).build();
}

#[test]
fn transform_plugin_dependency_beats_late_core_ordering() {
    use std::sync::{Arc, Mutex};

    use crate::parser::core::{CoreRule, CoreState};

    let order = Arc::new(Mutex::new(Vec::new()));

    let first_order = Arc::clone(&order);
    let first = PluginSpec::new("first").transform("run", move |_, _| {
        first_order.lock().unwrap().push("first");
    });

    let second_order = Arc::clone(&order);
    let second = PluginSpec::new("second")
        .transform("run", move |_, _| {
            second_order.lock().unwrap().push("second");
        })
        .after("first");

    let group = PluginSpec::new("group").with(second).with(first);
    let mut md = MarkdownIt::builder().plugin(group).build();

    struct Late(Arc<Mutex<Vec<&'static str>>>);
    impl CoreRule for Late {
        fn run(&self, _state: &mut CoreState<'_>) {
            self.0.lock().unwrap().push("late");
        }
    }

    // A rule registered after the transforms that must run before the first
    // transform used to be able to reorder the two transforms.
    md.add_rule_instance(Late(Arc::clone(&order)))
        .before_mark(super::transform_rule_mark("first", "run"));

    md.parse("text");

    assert_eq!(*order.lock().unwrap(), ["late", "first", "second"]);
}

#[test]
fn transform_order_preserves_indirect_dependencies_through_anchor_plugins() {
    use std::sync::{Arc, Mutex};

    use crate::parser::core::{CoreRule, CoreState};

    let order = Arc::new(Mutex::new(Vec::new()));

    let first_order = Arc::clone(&order);
    let first = PluginSpec::new("first").transform("run", move |_, _| {
        first_order.lock().unwrap().push("first");
    });

    // This plugin has no transform, but it carries an ordering constraint
    // between `first` and `last`.
    let bridge = PluginSpec::new("bridge").after("first");

    let last_order = Arc::clone(&order);
    let last = PluginSpec::new("last")
        .transform("run", move |_, _| {
            last_order.lock().unwrap().push("last");
        })
        .after("bridge");

    struct Late(Arc<Mutex<Vec<&'static str>>>);
    impl CoreRule for Late {
        fn run(&self, _state: &mut CoreState<'_>) {
            self.0.lock().unwrap().push("late");
        }
    }

    let mut md = MarkdownIt::builder()
        .plugin(last)
        .plugin(bridge)
        .plugin(first)
        .build();
    md.add_rule_instance(Late(Arc::clone(&order)))
        .before_mark(super::transform_rule_mark("first", "run"));

    md.parse("text");

    assert_eq!(*order.lock().unwrap(), ["late", "first", "last"]);
}

#[test]
fn multiple_transforms_in_one_plugin_run_in_declaration_order() {
    use std::sync::{Arc, Mutex};

    use crate::parser::core::{CoreRule, CoreState};

    let order = Arc::new(Mutex::new(Vec::new()));

    let first_order = Arc::clone(&order);
    let second_order = Arc::clone(&order);
    let plugin = PluginSpec::new("ordered")
        .transform("first", move |_, _| {
            first_order.lock().unwrap().push("first");
        })
        .transform("second", move |_, _| {
            second_order.lock().unwrap().push("second");
        });

    struct Late(Arc<Mutex<Vec<&'static str>>>);
    impl CoreRule for Late {
        fn run(&self, _state: &mut CoreState<'_>) {
            self.0.lock().unwrap().push("late");
        }
    }

    let mut md = MarkdownIt::builder().plugin(plugin).build();
    md.add_rule_instance(Late(Arc::clone(&order)))
        .before_mark(super::transform_rule_mark("ordered", "first"));
    md.parse("text");

    assert_eq!(*order.lock().unwrap(), ["late", "first", "second"]);
}

#[test]
fn transform_identities_are_unambiguous_when_names_contain_colons() {
    use std::sync::{Arc, Mutex};

    use super::transform_rule_mark;

    assert_ne!(
        transform_rule_mark("a:b", "c"),
        transform_rule_mark("a", "b:c")
    );

    let seen = Arc::new(Mutex::new(Vec::new()));

    let first_seen = Arc::clone(&seen);
    let first = PluginSpec::new("a:b").transform("c", move |_, _| {
        first_seen.lock().unwrap().push("a:b/c");
    });

    let second_seen = Arc::clone(&seen);
    let second = PluginSpec::new("a").transform("b:c", move |_, _| {
        second_seen.lock().unwrap().push("a/b:c");
    });

    let md = MarkdownIt::builder().plugin(first).plugin(second).build();
    md.parse("text");

    assert_eq!(*seen.lock().unwrap(), ["a:b/c", "a/b:c"]);
}

#[test]
fn transform_group_dependency_covers_child_plugins() {
    use std::sync::{Arc, Mutex};

    use crate::parser::core::{CoreRule, CoreState};

    let order = Arc::new(Mutex::new(Vec::new()));

    let first_order = Arc::clone(&order);
    let first = PluginSpec::new("first").transform("run", move |_, _| {
        first_order.lock().unwrap().push("first");
    });

    let second_order = Arc::clone(&order);
    let second = PluginSpec::new("second")
        .transform("run", move |_, _| {
            second_order.lock().unwrap().push("second");
        })
        .after("group");

    let group = PluginSpec::new("group").with(first);
    let root = PluginSpec::new("root").with(group).with(second);
    let mut md = MarkdownIt::builder().plugin(root).build();

    struct Late(Arc<Mutex<Vec<&'static str>>>);
    impl CoreRule for Late {
        fn run(&self, _state: &mut CoreState<'_>) {
            self.0.lock().unwrap().push("late");
        }
    }

    md.add_rule_instance(Late(Arc::clone(&order)))
        .before_mark(super::transform_rule_mark("first", "run"));

    md.parse("text");

    assert_eq!(*order.lock().unwrap(), ["late", "first", "second"]);
}

#[test]
fn missing_before_and_after_targets_are_ignored_without_require() {
    let plugin = PluginSpec::new("optional")
        .before("missing-before")
        .after("missing-after");

    MarkdownIt::builder().plugin(plugin).build();
}

#[test]
#[should_panic(expected = "missing dependency")]
fn require_rejects_missing_targets() {
    let plugin = PluginSpec::new("required").require("missing");

    MarkdownIt::builder().plugin(plugin).build();
}

#[test]
fn inline_leaf_can_read_parser_configuration() {
    #[derive(Debug)]
    struct Leaf;

    let plugin = PluginSpec::new("config-leaf")
        .inline_leaf::<Leaf>("config-leaf")
        .marker('@')
        .parse(|cx| {
            assert_eq!(cx.markdown_it().max_nesting, 7);
            cx.consume("@").then_some(Leaf)
        })
        .render_html(|_, html| {
            html.text("leaf");
        })
        .finish();

    let md = MarkdownIt::builder()
        .plugin(PluginSpec::custom("configure", |md| md.max_nesting = 7))
        .plugin(plugin)
        .build();
    assert_eq!(md.render("a @"), "<p>a leaf</p>\n");
}

#[test]
fn block_leaf_can_read_parser_configuration() {
    #[derive(Debug)]
    struct Leaf;

    let plugin = PluginSpec::new("config-block")
        .block_leaf::<Leaf>("config-block")
        .parse(|cx| {
            assert_eq!(cx.markdown_it().max_nesting, 7);
            if cx.line(0)? != "%%%" {
                return None;
            }
            cx.advance_lines(1);
            Some(Leaf)
        })
        .render_html(|_, html| {
            html.element("div").text("leaf");
        })
        .finish();

    let md = MarkdownIt::builder()
        .plugin(PluginSpec::custom("configure", |md| md.max_nesting = 7))
        .plugin(plugin)
        .build();
    assert_eq!(md.render("%%%"), "<div>leaf</div>\n");
}

#[test]
fn inline_container_source_reference_survives_cursor_advance() {
    #[derive(Debug)]
    struct Span;

    let plugin = PluginSpec::new("borrowed")
        .inline_container::<Span>("borrowed")
        .marker('{')
        .parse(|cx| {
            let source = cx.source();
            if !cx.consume("{") {
                return None;
            }
            let end = source.find('}')?;
            if !cx.advance(end) {
                return None;
            }
            Some((Span, 1..end))
        })
        .html_tag("span")
        .finish();

    let md = MarkdownIt::builder().plugin(plugin).build();
    assert_eq!(md.render("{hi}"), "<p><span>hi</span></p>\n");
}
