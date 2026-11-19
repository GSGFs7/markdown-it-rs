use markdown_it::MarkdownIt;
use markdown_it::parser::inline::helpers::paired_span::{EscapePolicy, NestingPolicy, PairOptions};
use markdown_it::plugin::PluginSpec;

fn parser() -> MarkdownIt {
    #[derive(Debug)]
    struct Literal(String);
    #[derive(Debug)]
    struct Container;

    let literal = PairOptions::new("{{", "}}").escape(EscapePolicy::Backslash);
    let container = PairOptions::new("[[", "]]")
        .escape(EscapePolicy::Backslash)
        .nesting(NestingPolicy::SamePair);
    let plugin = PluginSpec::new("paired-spans")
        .inline_leaf::<Literal>("literal")
        .marker('{')
        .parse(move |cx| {
            let matched = cx.scan_pair(&literal)?;
            let value = Literal(cx.remaining_slice(matched.body)?.to_owned());
            cx.advance_bytes(matched.consumed).then_some(value)
        })
        .render_html(|value, html| {
            html.element("code").text(&value.0);
        })
        .finish()
        .inline_container::<Container>("container")
        .marker('@')
        .parse(move |cx| {
            // Exercise scanning after a prefix has already moved the local cursor.
            if !cx.consume("@") {
                return None;
            }
            cx.consume_part(&container)?;
            Some(Container)
        })
        .html_tag("span")
        .finish();
    MarkdownIt::builder().plugin(plugin).build()
}

#[test]
fn literal_and_markdown_bodies_work_together_and_inside_links() {
    let md = parser();
    assert_eq!(
        md.render("前 {{*雪* <x>}} @[[ *雨* {{*风*}} ]] 尾"),
        "<p>前 <code>*雪* &lt;x&gt;</code> <span> <em>雨</em> <code>*风*</code> </span> 尾</p>\n"
    );
    // Link-label boundary probing calls the same scan as actual node creation.
    assert_eq!(
        md.render("[{{a]b}} @[[*雪*]]](url)"),
        "<p><a href=\"url\"><code>a]b</code> <span><em>雪</em></span></a></p>\n"
    );
}

#[test]
fn images_stay_opaque_when_a_container_is_scanned_inside_a_link_label() {
    let md = parser();
    // The link-label probe must still recognize the image span, otherwise the
    // `]]` inside its destination closes the container early.
    assert_eq!(
        md.render("[@[[![x](/a]]b)]]](/out)"),
        "<p><a href=\"/out\"><span><img src=\"/a%5D%5Db\" alt=\"x\"></span></a></p>\n"
    );
}

#[test]
fn failures_roll_back_local_cursor_and_escaped_initial_markers_stay_text() {
    let md = parser();
    assert_eq!(md.render("前 @[[雪 尾"), "<p>前 @[[雪 尾</p>\n");
    assert_eq!(md.render("@[[a\nb]]"), "<p>@[[a\nb]]</p>\n");
    assert_eq!(md.render(r"\{{雪}}"), "<p>{{雪}}</p>\n");
    assert_eq!(
        md.render("{{}} @[[]]"),
        "<p><code></code> <span></span></p>\n"
    );
    assert_eq!(md.render(r"{{a\}}b}}"), "<p><code>a\\}}b</code></p>\n");
}

#[cfg(feature = "sourcemap")]
#[test]
fn child_ranges_keep_original_utf8_source_positions_after_a_prefix() {
    let document = parser().parse("前 @[[*雪*]] 尾");
    let paragraph = document.children(document.root())[0];
    let wrapper = document.children(paragraph)[1];
    let part = document.children(wrapper)[0];
    let emphasis = document.children(part)[0];
    let text = document.children(emphasis)[0];
    for (id, expected) in [
        (wrapper, (4, 14)),
        (part, (7, 12)),
        (emphasis, (7, 12)),
        (text, (8, 11)),
    ] {
        assert_eq!(
            document.node(id).srcmap().unwrap().get_byte_offsets(),
            expected
        );
    }
}

#[test]
fn leaf_pair_scanning_remains_literal_while_containers_skip_inline_spans() {
    let md = parser();
    assert_eq!(md.render("{{`}}`}}"), "<p><code>`</code>`}}</p>\n");
    assert_eq!(
        md.render("@[[a `]]` b]]"),
        "<p><span>a <code>]]</code> b</span></p>\n"
    );
    // A custom leaf is opaque to the container's boundary checks as well.
    assert_eq!(
        md.render("@[[a {{]]}} b]]"),
        "<p><span>a <code>]]</code> b</span></p>\n"
    );
}
