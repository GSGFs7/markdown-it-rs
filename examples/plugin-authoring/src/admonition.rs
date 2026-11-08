//! Configured block container plugin.
//!
//! A fenced admonition (`::: title` ... `:::`) is matched from a
//! [`PluginSpec`], its body is parsed as nested block markup, and the result is
//! wrapped in an `<aside>`.
//!
//! # What it demonstrates
//!
//! - Matching a fixed opening and closing line without writing a `BlockRule`.
//! - Parsing paragraphs, lists, quotes and fenced code inside the container.
//! - An unclosed fence consuming the rest of the visible block window.
//! - Rendering children from a high-level block callback.

use markdown_it::plugin::{BlockContainerParse, PluginSpec};

#[derive(Debug)]
pub struct Admonition {
    title: String,
}

pub fn admonition_plugin() -> PluginSpec {
    PluginSpec::new("admonition")
        .block_container::<Admonition>("admonition")
        .parse(|cx| {
            let title = cx.line(0)?.strip_prefix(":::")?.trim().to_owned();
            let close = (1..cx.remaining_lines()).find(|&line| cx.line(line) == Some(":::"));
            let body_end = close.unwrap_or(cx.remaining_lines());
            Some(
                BlockContainerParse::new(Admonition { title }, body_end - 1)
                    .close_lines(usize::from(close.is_some())),
            )
        })
        .render_html(|admonition, html| {
            let children = html.render_children();
            html.element("aside")
                .class("admonition")
                .attr("data-title", &admonition.title)
                .raw(children);
        })
        .render_text(|admonition, text| {
            text.write_fmt(format_args!("[{}]\n", admonition.title));
            text.render_children();
        })
        .finish()
}

#[cfg(test)]
mod tests {
    use markdown_it::MarkdownIt;

    use super::*;

    fn parser() -> MarkdownIt {
        MarkdownIt::builder().plugin(admonition_plugin()).build()
    }

    #[test]
    fn container_parses_body_as_blocks() {
        let md = parser();
        assert_eq!(
            md.render("::: note\nbody *here*\n:::"),
            "<aside class=\"admonition\" data-title=\"note\"><p>body <em>here</em></p>\n</aside>\n"
        );
    }

    #[test]
    fn unclosed_container_consumes_the_rest_of_the_window() {
        let md = parser();
        let html = md.render("::: note\n- one\n- two");
        assert!(html.contains("<ul>"));
        assert!(html.contains("data-title=\"note\""));
        assert!(html.ends_with("</aside>\n"));
    }

    #[test]
    fn closed_container_keeps_following_blocks() {
        let md = parser();
        let html = md.render("::: note\nbody\n:::\nafter");
        assert!(html.ends_with("</aside>\n<p>after</p>\n"));
    }

    #[test]
    fn container_renders_children_as_text() {
        let md = parser();
        assert_eq!(
            md.render_document(&md.parse("::: note\nbody *here*\n:::"), "text"),
            "[note]\nbody here\n"
        );
    }
}
