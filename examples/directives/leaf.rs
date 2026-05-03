use markdown_it::plugins::extra::directives::DirectiveKind;
use markdown_it::{Node, Renderer};

pub fn render_youtube(
    kind: DirectiveKind,
    _name: &str,
    attrs: &[(String, String)],
    _node: &Node,
    fmt: &mut dyn Renderer,
) {
    assert_eq!(kind, DirectiveKind::Leaf);

    let video_id = attrs
        .iter()
        .find(|(k, _)| k == "v")
        .map(|(_, v)| v.as_str())
        .unwrap_or("");

    fmt.cr();
    fmt.open("div", &[("class", "video-container".to_owned())]);
    fmt.open("iframe", &[
        // it not work. google's reason
        ("src", format!("https://www.youtube-nocookie.com/embed/{}", video_id)),
        ("title", "YouTube video player".to_owned()),
        ("frameborder", "0".to_owned()),
        ("allow", "accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share".to_owned()),
        ("allowfullscreen", "true".to_owned()),
    ]);
    fmt.close("iframe");
    fmt.close("div");
    fmt.cr();
}
