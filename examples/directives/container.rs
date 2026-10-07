use markdown_it::plugins::directives::{DirectiveKind, DirectiveNode, DirectiveRenderer};

pub fn render_alert(
    kind: DirectiveKind,
    name: &str,
    _attrs: &[(String, String)],
    node: DirectiveNode<'_>,
    fmt: &mut DirectiveRenderer<'_, '_>,
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
            ("class".into(), "markdown-alert".to_owned()),
            ("class".into(), format!("markdown-alert-{name}")),
        ],
    );
    fmt.open("p", &[("class".into(), "markdown-alert-title".to_owned())]);
    fmt.text(title);
    fmt.close("p");
    fmt.contents(node.children());
    fmt.close("div");
    fmt.cr();
}
