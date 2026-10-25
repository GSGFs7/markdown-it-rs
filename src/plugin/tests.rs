use super::*;
use crate::document::NodeValue;

#[derive(Debug, PartialEq, Eq)]
struct Word(String);

#[derive(Debug)]
struct Plain;

impl InlinePlugin for Plain {
    const RULE: &'static str = "plain";
    const MARKER: char = '@';

    fn parse(cx: &mut InlinePluginState<'_>) -> Option<Self> {
        cx.consume("@plain").then_some(Plain)
    }

    fn render_html(&self, html: &mut Html) {
        html.element("i").text("plain");
    }
}

impl InlinePlugin for Word {
    const RULE: &'static str = "word";
    const MARKER: char = '@';

    fn parse(cx: &mut InlinePluginState<'_>) -> Option<Self> {
        let rest = cx.remaining().strip_prefix('@')?;
        let end = rest
            .find(|ch: char| !ch.is_ascii_alphabetic())
            .unwrap_or(rest.len());
        cx.advance(end + 1).then_some(Word(rest[..end].to_owned()))
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
fn installs_static_inline_leaf_and_renderer() {
    let md = MarkdownIt::builder()
        .plugin(PluginSpec::of::<Word>())
        .build();
    assert_eq!(
        md.render("say @hello!"),
        "<p>say <span class=\"word\">hello</span>!</p>\n"
    );
}

#[test]
fn static_leaf_type_can_be_registered_directly() {
    let md = MarkdownIt::builder().plugin(Plain).build();
    assert_eq!(md.render("say @plain!"), "<p>say <i>plain</i>!</p>\n");
}

#[test]
fn leaf_unifies_static_and_dynamic_registration() {
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
        .plugin(PluginSpec::of::<Word>())
        .plugin(bang)
        .build();
    assert_eq!(
        md.render("@hello!"),
        "<p><span class=\"word\">hello</span><b>bang</b></p>\n"
    );
}

#[test]
fn plugin_spec_with_groups_static_and_dynamic_leaves() {
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

    let group = PluginSpec::new("group").with(Plain).with(bang);
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
