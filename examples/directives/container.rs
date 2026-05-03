use markdown_it::plugins::extra::directives::DirectiveKind;
use markdown_it::{Node, Renderer};

pub fn render_alert(
    kind: DirectiveKind,
    name: &str,
    _attrs: &[(String, String)],
    node: &Node,
    fmt: &mut dyn Renderer,
) {
    assert_eq!(kind, DirectiveKind::Container);

    let title = match name {
        "note" => "Note",
        "tip" => "Tip",
        "important" => "Important",
        "warning" => "Warning",
        "caution" => "Caution",
        _ => name,
    };

    fmt.cr();
    fmt.open(
        "div",
        &[
            ("class", "markdown-alert".to_owned()),
            ("class", format!("markdown-alert-{name}")),
        ],
    );
    fmt.open("p", &[("class", "markdown-alert-title".to_owned())]);
    fmt.text(title);
    fmt.close("p");
    fmt.contents(&node.children);
    fmt.close("div");
    fmt.cr();
}
