use markdown_it::plugins::directives::{self, Attrs, DirectiveKind, TextDirective};
use markdown_it::{MarkdownIt, Node, Renderer};

fn render(src: &str) -> String {
    let mut md = MarkdownIt::new();
    markdown_it::plugins::cmark::add(&mut md);
    directives::add(&mut md);

    md.parse(src).render().trim().to_owned()
}

fn render_with(src: &str, configure: impl FnOnce(&mut MarkdownIt)) -> String {
    let mut md = MarkdownIt::new();
    markdown_it::plugins::cmark::add(&mut md);
    directives::add(&mut md);
    configure(&mut md);

    md.parse(src).render().trim().to_owned()
}

fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> &'a str {
    attrs
        .iter()
        .find_map(|(key, value)| (key == name).then_some(value.as_str()))
        .unwrap_or("")
}

fn render_badge(
    kind: DirectiveKind,
    name: &str,
    attrs: &[(String, String)],
    _node: &Node,
    fmt: &mut dyn Renderer,
) {
    assert_eq!(kind, DirectiveKind::Text);
    assert_eq!(name, "badge");

    let html_attrs = [
        ("data-kind", format!("{kind:?}")),
        ("data-tone", attr(attrs, "tone").to_owned()),
    ];
    fmt.open("mark", &html_attrs);
    fmt.text(attr(attrs, "label"));
    fmt.close("mark");
}

fn render_leaf_callout(
    kind: DirectiveKind,
    name: &str,
    attrs: &[(String, String)],
    node: &Node,
    fmt: &mut dyn Renderer,
) {
    assert_eq!(kind, DirectiveKind::Leaf);
    assert_eq!(name, "callout");
    assert!(node.children.is_empty());

    let html_attrs = [("data-name", name.to_owned())];
    fmt.cr();
    fmt.open("aside", &html_attrs);
    fmt.open("strong", &[]);
    fmt.text(attr(attrs, "title"));
    fmt.close("strong");
    fmt.close("aside");
    fmt.cr();
}

fn render_panel(
    kind: DirectiveKind,
    name: &str,
    attrs: &[(String, String)],
    node: &Node,
    fmt: &mut dyn Renderer,
) {
    assert_eq!(kind, DirectiveKind::Container);
    assert_eq!(name, "panel");

    let html_attrs = [("data-title", attr(attrs, "title").to_owned())];
    fmt.cr();
    fmt.open("section", &html_attrs);
    fmt.contents(&node.children);
    fmt.close("section");
    fmt.cr();
}

#[test]
fn text_directive() {
    let html = render("hello :name{a=\"b\"} world");
    assert_eq!(
        html,
        "<p>hello <span class=\"directive name\" a=\"b\"></span> world</p>"
    );
}

#[test]
fn text_directive_requires_name_immediately_after_colon() {
    let html = render("Note: warning");
    assert_eq!(html, "<p>Note: warning</p>");
}

#[test]
fn text_directive_does_not_start_after_another_colon() {
    let html = render(":::bad trailing");
    assert_eq!(html, "<p>:::bad trailing</p>");
}

#[test]
fn leaf_directive() {
    let html = render("::name{cia=\"llo\"}");
    assert_eq!(html, "<div class=\"directive name\" cia=\"llo\"></div>");
}

#[test]
fn container_directive() {
    let html = render(":::name{cia=\"llo\"}\nworld\n:::");
    assert_eq!(
        html,
        "<div class=\"directive name\" cia=\"llo\">\n<p>world</p>\n</div>"
    );
}

#[test]
fn container_directive_nested() {
    let html = render(":::name\n:::child\nhello\n:::\n:::");
    assert_eq!(
        html,
        "<div class=\"directive name\">\n<div class=\"directive child\">\n<p>hello</p>\n</div>\n</div>"
    );
}

#[test]
fn container_directive_nested_with_longer_marker() {
    let html = render(":::name\n::::child\nhello\n::::\n:::");
    assert_eq!(
        html,
        "<div class=\"directive name\">\n<div class=\"directive child\">\n<p>hello</p>\n</div>\n</div>"
    );
}

#[test]
fn container_directive_closed_by_longer_marker() {
    let html = render(":::name\nhello\n::::");
    assert_eq!(html, "<div class=\"directive name\">\n<p>hello</p>\n</div>");
}

#[test]
fn container_directive_respects_outdent_in_list() {
    let html = render("- :::name\n  hello\noutside");
    assert_eq!(
        html,
        "<ul>\n<li>\n<div class=\"directive name\">\n<p>hello</p>\n</div>\n</li>\n</ul>\n<p>outside</p>"
    );
}

#[test]
fn directive_shorthand_attributes() {
    let html = render(":name{#my-id .my-class}");
    assert_eq!(
        html,
        "<p><span class=\"directive name my-class\" id=\"my-id\"></span></p>"
    );
}

#[test]
fn directive_quoted_attributes() {
    let html = render(":name{title=\"Ciallo World\"}");
    assert_eq!(
        html,
        "<p><span class=\"directive name\" title=\"Ciallo World\"></span></p>"
    );
}

#[test]
fn directive_boolean_attributes() {
    let html = render(":name{disabled}");
    assert_eq!(
        html,
        "<p><span class=\"directive name\" disabled=\"\"></span></p>"
    );
}

#[test]
fn text_directive_uses_registered_custom_render() {
    let html = render_with("hello :badge{label=\"Beta\" tone=\"new\"} world", |md| {
        directives::add_render(md, DirectiveKind::Text, "badge", render_badge);
    });

    assert_eq!(
        html,
        "<p>hello <mark data-kind=\"Text\" data-tone=\"new\">Beta</mark> world</p>"
    );
}

#[test]
fn leaf_directive_uses_registered_custom_render() {
    let html = render_with("::callout{title=\"Heads-up\"}", |md| {
        directives::add_render(md, DirectiveKind::Leaf, "callout", render_leaf_callout);
    });

    assert_eq!(
        html,
        "<aside data-name=\"callout\"><strong>Heads-up</strong></aside>"
    );
}

#[test]
fn container_directive_uses_registered_custom_render_and_children() {
    let html = render_with(":::panel{title=\"Intro\"}\nCiallo **world**\n:::", |md| {
        directives::add_render(md, DirectiveKind::Container, "panel", render_panel);
    });

    assert_eq!(
        html,
        "<section data-title=\"Intro\">\n<p>Ciallo <strong>world</strong></p>\n</section>"
    );
}

#[test]
fn default_renderer_passes_user_attributes_through_without_sanitizing() {
    let cases = [
        (
            r#":name{title="safe" class="admin" id="root" onclick="alert(1)" style="color:red" data-directive-kind="evil" data-directive-name="evil"}"#,
            r#"<p><span class="directive name admin" title="safe" id="root" onclick="alert(1)" style="color:red" data-directive-kind="evil" data-directive-name="evil"></span></p>"#,
        ),
        (
            r#"::name{title="safe" onclick="alert(1)" style="color:red"}"#,
            r#"<div class="directive name" title="safe" onclick="alert(1)" style="color:red"></div>"#,
        ),
        (
            r#":::name{title="safe" onclick="alert(1)" style="color:red"}
body
:::"#,
            r#"<div class="directive name" title="safe" onclick="alert(1)" style="color:red">
<p>body</p>
</div>"#,
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(render(source), expected, "source: {source}");
    }
}

#[test]
fn default_renderer_preserves_node_attributes() {
    let html = render_with(":name", markdown_it::plugins::sourcepos::add);

    assert!(html.contains(r#"<span data-sourcepos=""#));
    assert!(html.contains(r#"class="directive name""#));
}

#[test]
fn directive_attribute_values_are_html_escaped() {
    assert_eq!(
        render(r#":name{title="&<>\""}"#),
        r#"<p><span class="directive name" title="&amp;&lt;&gt;&quot;"></span></p>"#
    );
}

fn parse_text_attrs(source: &str) -> Attrs {
    let mut md = MarkdownIt::new();
    markdown_it::plugins::cmark::add(&mut md);
    directives::add(&mut md);

    let ast = md.parse(source);
    let mut result = None;
    ast.walk(|node, _| {
        if result.is_none()
            && let Some(directives) = node.cast::<TextDirective>()
        {
            result = Some(directives.attrs.clone());
        }
    });

    result.expect("expected a text directive")
}

#[test]
fn directive_attributes_remain_available_in_ast() {
    assert_eq!(
        parse_text_attrs(r#":badge{label="Beta" onclick="alert(1)"}"#),
        vec![
            ("label".to_owned(), "Beta".to_owned()),
            ("onclick".to_owned(), "alert(1)".to_owned()),
        ]
    );
}

#[test]
fn registered_renderer_escapes_text_and_ignores_unknown_attributes() {
    let html = render_with(
        r#":badge{label="<img src=x onerror=alert(1)>" tone="new" onclick="alert(1)"}"#,
        |md| directives::add_render(md, DirectiveKind::Text, "badge", render_badge),
    );

    assert_eq!(
        html,
        r#"<p><mark data-kind="Text" data-tone="new">&lt;img src=x onerror=alert(1)&gt;</mark></p>"#,
    )
}
