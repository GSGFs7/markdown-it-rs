//! Configured block leaf plugin.
//!
//! Uses a [`PluginSpec`] to match a line starting with `:: `, create a leaf
//! node with a typed `Notice` payload, and render its text as `<aside>`.
//!
//! # What it demonstrates
//!
//! - Reading and consuming block lines from a configured plugin.
//! - Creating a block-level leaf node with a typed payload.
//! - Rolling back consumed lines when a match ultimately fails.
//!
//! # Test coverage
//!
//! - `block_leaf_reads_and_consumes_lines_without_losing_following_blocks`:
//!   consuming the notice line preserves its payload and the following
//!   paragraphs, while rendering escapes HTML.
//! - `failed_block_leaf_rolls_back_consumption`: a line consumed before a failed
//!   validation is rolled back, so ordinary text still becomes a paragraph.

use markdown_it::plugin::PluginSpec;

#[derive(Debug)]
struct Notice(String);

pub fn notice_plugin() -> PluginSpec {
    PluginSpec::new("notice")
        .block_leaf::<Notice>("notice")
        .parse(|cx| {
            let line = cx.line(0)?.to_owned();
            if !cx.advance_lines(1) {
                return None;
            }
            Some(Notice(line.strip_prefix(":: ")?.to_owned()))
        })
        .render_html(|notice, html| {
            html.element("aside").text(&notice.0);
        })
        .finish()
}

#[cfg(test)]
mod tests {
    use markdown_it::MarkdownIt;

    use super::*;

    fn block_parser() -> MarkdownIt {
        MarkdownIt::builder().plugin(notice_plugin()).build()
    }

    #[test]
    fn block_leaf_reads_and_consumes_lines_without_losing_following_blocks() {
        let md = block_parser();
        let doc = md.parse(":: 雪<&\n\n后文");
        let children = doc.children(doc.root());
        assert_eq!(children.len(), 2);
        assert_eq!(doc.value::<Notice>(children[0]).unwrap().0, "雪<&");
        assert!(
            md.render_document(&doc, "html")
                .contains("<aside>雪&lt;&amp;</aside>")
        );
        assert!(md.render_document(&doc, "html").contains("<p>后文</p>"));
    }

    #[test]
    fn failed_block_leaf_rolls_back_consumption() {
        assert_eq!(block_parser().render("普通文本"), "<p>普通文本</p>\n");
    }
}
