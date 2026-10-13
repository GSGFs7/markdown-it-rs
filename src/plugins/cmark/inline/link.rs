//! Links
//!
//! `![link](<to> "stuff")`
//!
//! <https://spec.commonmark.org/0.30/#links>
use crate::MarkdownIt;
use crate::document::{NodeRef, NodeValue};
use crate::parser::inline::helpers::full_link;
use crate::render::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    TransparentDocumentRenderer,
    write_html_close,
    write_html_open,
};

#[derive(Debug)]
pub struct Link {
    pub url: String,
    pub title: Option<String>,
}

struct LinkDocumentRenderer;

impl DocumentNodeRenderer<Link> for LinkDocumentRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        link: &Link,
        context: &mut DocumentRenderContext<'_>,
        output: &mut crate::DocumentWriter,
    ) {
        let mut attrs = node.attrs().clone();
        attrs.push(("href".into(), link.url.clone()));
        if let Some(title) = link.title.as_ref().filter(|title| !title.is_empty()) {
            attrs.push(("title".into(), title.clone()));
        }

        write_html_open(output, "a", &attrs);
        context.render_children(node.id(), output);
        write_html_close(output, "a");
    }
}

impl NodeValue for Link {}

pub fn add(md: &mut MarkdownIt) {
    full_link::add::<false>(md, |document, href, title| {
        document.create_node(Link {
            url: href.unwrap_or_default(),
            title,
        })
    });
    md.add_document_renderer::<Link, _>("html", LinkDocumentRenderer);
    md.add_document_renderer::<Link, _>("text", TransparentDocumentRenderer);
}

#[cfg(test)]
mod tests {
    #[test]
    fn inline_targets_with_long_leading_spaces_preserve_parsing_and_fallback() {
        let md = crate::MarkdownIt::new();
        let spaces = " ".repeat(4096);
        assert_eq!(
            md.render(&format!("[foo]({spaces}/valid)")),
            "<p><a href=\"/valid\">foo</a></p>\n",
        );
        assert_eq!(
            md.render(&format!("[foo]({spaces}/unfinished\n\n[foo]: /url\n")),
            format!("<p><a href=\"/url\">foo</a>({spaces}/unfinished</p>\n"),
        );
    }

    #[test]
    fn missing_inline_target_content_does_not_fall_back_to_reference() {
        let mut md = crate::MarkdownIt::empty();
        crate::plugins::cmark::add(&mut md);
        md.max_nesting = 20;
        md.render_options.breaks = true;
        for whitespace in ["", " ", "\t", " \t\n"] {
            let source = format!("[foo]({whitespace}\n\n[foo]: /url1\n");
            assert_eq!(md.render(&source), "<p>[foo](</p>\n", "{source}");
        }
    }

    #[test]
    fn invalid_nonempty_inline_target_can_still_resolve_reference() {
        let md = crate::MarkdownIt::new();
        assert_eq!(
            md.render("[foo](/unfinished\n\n[foo]: /url1\n"),
            "<p><a href=\"/url1\">foo</a>(/unfinished</p>\n",
        );
        assert_eq!(
            md.render("[foo](<unfinished\n\n[foo]: /url1\n"),
            "<p><a href=\"/url1\">foo</a>(&lt;unfinished</p>\n",
        );
        assert_eq!(
            md.render("[foo]()\n\n[foo]: /url1\n"),
            "<p><a href=\"\">foo</a></p>\n",
        );
    }
}
