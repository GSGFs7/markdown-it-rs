use markdown_it::plugins::directives::DirectiveKind;
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
    fmt.open("div", &[("class".into(), "video-container".to_owned())]);
    fmt.open("iframe", &[
        // it not work. google's reason
        ("src".into(), format!("https://www.youtube-nocookie.com/embed/{}", video_id)),
        ("title".into(), "YouTube video player".to_owned()),
        ("frameborder".into(), "0".to_owned()),
        ("allow".into(), "accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share".to_owned()),
        ("allowfullscreen".into(), "true".to_owned()),
    ]);
    fmt.close("iframe");
    fmt.close("div");
    fmt.cr();
}
