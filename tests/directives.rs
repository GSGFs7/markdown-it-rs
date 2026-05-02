use markdown_it::plugins::extra::directives::{self, DirectiveKind};
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
    assert_eq!(
        html,
        "<div class=\"directive name\">\n<p>hello</p>\n</div>"
    );
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
