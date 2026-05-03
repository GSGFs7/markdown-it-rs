use markdown_it::plugins::extra::directives::DirectiveKind;
use markdown_it::{Node, Renderer};

pub fn render_badge(
    kind: DirectiveKind,
    _name: &str,
    attrs: &[(String, String)],
    _node: &Node,
    fmt: &mut dyn Renderer,
) {
    assert_eq!(kind, DirectiveKind::Text);

    let label = attrs
        .iter()
        .find(|(k, _)| k == "label")
        .map(|(_, v)| v.as_str())
        .unwrap_or("BADGE");
    let badge_type = attrs
        .iter()
        .find(|(k, _)| k == "type")
        .map(|(_, v)| v.as_str())
        .unwrap_or("info");

    fmt.open("span", &[("class", format!("badge badge-{}", badge_type))]);
    fmt.text(label);
    fmt.close("span");
}
