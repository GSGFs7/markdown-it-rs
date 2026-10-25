//! Images
//!
//! `![image](<src> "title")`
//!
//! <https://spec.commonmark.org/0.30/#images>
use std::borrow::Cow;

use crate::MarkdownIt;
use crate::document::{NodeRef, StructuralEvent, Text, TextSpecial};
use crate::parser::inline::helpers::full_link;
use crate::plugins::cmark::inline::newline::{Hardbreak, Softbreak};
use crate::plugins::html::html_inline::HtmlInline;
use crate::render::{NodeRenderer, RenderContext, TransparentRenderer, write_html_attrs_with};

#[derive(Debug)]
pub struct Image {
    pub url: String,
    pub title: Option<String>,
}

struct ImageRenderer;

impl NodeRenderer<Image> for ImageRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        image: &Image,
        context: &mut RenderContext<'_>,
        output: &mut crate::Writer,
    ) {
        let alt = collect_alt_text(context, node);
        let title = image.title.as_deref().filter(|title| !title.is_empty());
        let extra = [
            ("src", image.url.as_str()),
            ("alt", alt.as_ref()),
            ("title", title.unwrap_or_default()),
        ];
        output.write_str("<img");
        write_html_attrs_with(
            output,
            node.attrs(),
            &extra[..2 + usize::from(title.is_some())],
        );
        if context.options().xhtml_out {
            output.write_str(" /");
        }
        output.write_char('>');
    }
}

// collect alt text, ignore marker
//
// e.g.
// raw: "![a *b* c](x)"
// children: [text "a ", emph(text "b"), text " c"]
// result: "a b c"
fn collect_alt_text<'a>(context: &RenderContext<'a>, image: NodeRef<'_>) -> Cow<'a, str> {
    match image.children() {
        [] => return Cow::Borrowed(""),
        [child] => {
            let child = context.document().node(*child);
            if child.children().is_empty()
                && let Some(text) = child.cast::<Text>()
            {
                return Cow::Borrowed(&text.content);
            }
        }
        _ => {}
    }

    let mut result = String::new();
    let mut image_depth = 0;
    for event in context.document().events(image.id()) {
        let node = event.node();
        match event {
            StructuralEvent::Enter(_) if node.is::<Image>() => image_depth += 1,
            StructuralEvent::Exit(_) if node.is::<Image>() => image_depth -= 1,
            StructuralEvent::Exit(_) => {}
            _ => append_alt_node(&mut result, node, image_depth > 1),
        }
    }
    Cow::Owned(result)
}

fn append_alt_node(result: &mut String, node: NodeRef<'_>, nested_image: bool) {
    if let Some(text) = node.cast::<Text>() {
        result.push_str(&text.content);
    } else if let Some(text) = node.cast::<TextSpecial>() {
        // JS text_join converts text_special only in the outer image's
        // children. Its alt renderer ignores escapes/entities still marked
        // as text_special inside nested images. Autolinks use ordinary text
        // tokens in JS and must remain visible at every depth.
        if !nested_image || text.info == "autolink" {
            result.push_str(&text.content);
        }
    } else if let Some(html) = node.cast::<HtmlInline>() {
        result.push_str(&html.content);
    } else if node.is::<Softbreak>() || node.is::<Hardbreak>() {
        result.push('\n');
    }
}

pub fn add(md: &mut MarkdownIt) {
    full_link::add_prefix_with_options::<'!', true>(
        md,
        full_link::LinkOptions {
            inline_target_fallback: full_link::InlineTargetFallback::Reject,
            label_context: full_link::LabelContext::Independent,
        },
        |document, href, title| {
            document.create_node(Image {
                url: href.unwrap_or_default(),
                title,
            })
        },
    )
    .alias_named("image");
    md.add_renderer::<Image, _>("html", ImageRenderer);
    md.add_renderer::<Image, _>("text", TransparentRenderer);
}

#[cfg(test)]
mod tests {
    #[test]
    fn alt_boundary_is_local_and_reaches_custom_delimiter_scanners() {
        use crate::parser::inline::helpers::delimiters::{DelimiterRun, set_delimiter_scanner};

        const SOURCE: &str = "雪 ![*x*](/img) *y*";
        let mut md = crate::MarkdownIt::new();
        set_delimiter_scanner(&mut md, |src, start, end, _| {
            let (open, close) = if src == "*x*" {
                assert_eq!(end, 3);
                assert!(start == 0 || start == 2);
                (start == 0, start == 2)
            } else {
                assert_eq!(src, SOURCE);
                assert_eq!(end, SOURCE.len());
                assert!(start == SOURCE.find("*y*").unwrap() || start == SOURCE.len() - 1);
                (
                    start == SOURCE.find("*y*").unwrap(),
                    start == SOURCE.len() - 1,
                )
            };
            DelimiterRun {
                marker: '*',
                can_open: open,
                can_close: close,
                length: 1,
            }
        });
        assert_eq!(
            md.render(SOURCE),
            "<p>雪 <img src=\"/img\" alt=\"x\"> <em>y</em></p>\n",
        );
    }

    #[test]
    #[cfg(feature = "sourcemap")]
    fn independent_alt_source_preserves_multiline_source_maps() {
        use crate::plugins::cmark::inline::emphasis::Em;

        let md = crate::MarkdownIt::new();
        let source = "> 雪 ![*a\n> b*](/img)";
        let document = md.parse(source);
        assert_eq!(
            md.render_html(&document),
            "<blockquote>\n<p>雪 <img src=\"/img\" alt=\"a\nb\"></p>\n</blockquote>\n",
        );
        let emphasis = document
            .events(document.root())
            .find_map(|event| event.node().is::<Em>().then_some(event.node()))
            .unwrap();
        assert_eq!(
            emphasis.srcmap().unwrap().get_byte_offsets(),
            (source.find('*').unwrap(), source.rfind('*').unwrap() + 1),
        );
    }

    #[test]
    fn alt_emphasis_uses_an_independent_source_boundary() {
        let md = crate::MarkdownIt::new();
        for (source, alt) in [
            ("![____&_|\t0___b_____ ___](/img)", "&amp;|\t0___b__ ___"),
            (
                "![*****|\r|c*_a ***\0_**b***** c*****](/img)",
                "|\n|ca ***�b c****",
            ),
        ] {
            assert_eq!(
                md.render(source),
                format!("<p><img src=\"/img\" alt=\"{alt}\"></p>\n"),
                "{source:?}",
            );
        }
    }

    #[test]
    fn nested_image_alt_text_matches_js_text_join_depth() {
        let md = crate::MarkdownIt::new();
        for (source, alt) in [
            (r"![\)]()", ")"),
            (r"![![\)]()]()", ""),
            (r"![![\`]()]()", ""),
            (r"![![\0]()]()", ""),
            (r"![![x\)y &amp; z]()]()", "xy  z"),
            ("![![`x` *y* <a@b.co>]()]()", "x y a@b.co"),
            (r"![before ![![\)]()]() after]()", "before  after"),
        ] {
            assert_eq!(
                md.render(source),
                format!("<p><img src=\"\" alt=\"{alt}\"></p>\n"),
                "{source:?}"
            );
        }
    }

    #[test]
    fn malformed_inline_images_do_not_fall_back_to_references() {
        let md = crate::MarkdownIt::new();
        for suffix in [
            "([]",
            "(/unfinished",
            "(<url>\"title\")",
            "(javascript:alert(1))",
        ] {
            let source = format!("![foo]{suffix}\n\n[foo]: /url \"title\"\n");
            let escaped_suffix = suffix
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;");
            assert_eq!(
                md.render(&source),
                format!("<p>!<a href=\"/url\" title=\"title\">foo</a>{escaped_suffix}</p>\n"),
                "{source}",
            );
        }
    }

    #[test]
    fn valid_inline_and_reference_images_still_parse() {
        let md = crate::MarkdownIt::new();
        for label in ["![foo]", "![foo][]", "![foo][foo]"] {
            let source = format!("{label}\n\n[foo]: /url \"title\"\n");
            assert_eq!(
                md.render(&source),
                "<p><img src=\"/url\" alt=\"foo\" title=\"title\"></p>\n"
            );
        }
        assert_eq!(
            md.render("![foo](/inline \"inline\")\n\n[foo]: /url \"title\"\n"),
            "<p><img src=\"/inline\" alt=\"foo\" title=\"inline\"></p>\n",
        );
    }

    #[test]
    fn malformed_inline_image_checks_match_nested_parsing() {
        let md = crate::MarkdownIt::new();
        assert_eq!(
            md.render("[![foo](/unfinished](/outer)\n\n[foo]: /url \"title\"\n"),
            "<p>[!<a href=\"/url\" title=\"title\">foo</a>(/unfinished](/outer)</p>\n",
        );
        assert_eq!(
            md.render("![![foo](/unfinished](/outer)\n\n[foo]: /url \"title\"\n"),
            "<p><img src=\"/outer\" alt=\"!foo(/unfinished\"></p>\n",
        );
    }

    #[test]
    fn alt_text_uses_a_fresh_inline_nesting_budget() {
        let mut md = crate::MarkdownIt::new();
        for limit in [3, 4, 5, 100] {
            md.max_nesting = limit;
            assert_eq!(
                md.render("![[[foo](uri1)](uri2)](uri3)"),
                "<p><img src=\"uri3\" alt=\"[foo](uri2)\"></p>\n"
            );
        }
        md.max_nesting = 2;
        assert_eq!(
            md.render("![![x](in)](out)"),
            "<p><img src=\"out\" alt=\"x\"></p>\n"
        );
    }
}
