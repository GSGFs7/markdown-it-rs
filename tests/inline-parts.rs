use std::ops::Range;

use markdown_it::MarkdownIt;
use markdown_it::parser::inline::helpers::paired_span::{EscapePolicy, NestingPolicy, PairOptions};
use markdown_it::plugin::{InlineContainerPart, PluginSpec};

#[derive(Debug)]
struct Annotation;

fn annotation() -> PluginSpec {
    let label = PairOptions::new("[", "]")
        .escape(EscapePolicy::Backslash)
        .nesting(NestingPolicy::SamePair);
    let note = PairOptions::new("{", "}")
        .escape(EscapePolicy::Backslash)
        .nesting(NestingPolicy::SamePair);
    PluginSpec::new("annotation")
        .inline_container::<Annotation>("annotation")
        .marker('[')
        .parse(move |cx| {
            cx.consume_part(&label)?;
            cx.consume_part(&note)?;
            Some(Annotation)
        })
        .render_html(|_, html| {
            let label = html.render_part(0);
            let note = html.render_part(1);
            assert_eq!(html.render_part(2), "");
            html.element("ruby").raw(label).element("rt").raw(note);
        })
        .render_text(|_, text| {
            text.render_part(0);
            text.write_str("(");
            text.render_part(1);
            text.write_str(")");
            text.render_part(2);
        })
        .finish()
}

#[test]
fn composed_pairs_parse_markdown_in_both_parts() {
    let md = MarkdownIt::builder().plugin(annotation()).build();
    let doc = md.parse("前[**文字**]{*说明* & <}尾");
    assert_eq!(
        md.render_document(&doc, "html"),
        "<p>前<ruby><strong>文字</strong><rt><em>说明</em> &amp; &lt;</rt></ruby>尾</p>\n"
    );
    assert_eq!(md.render_document(&doc, "text"), "前文字(说明 & <)尾\n");
    let paragraph = doc.children(doc.root())[0];
    let node = doc.children(paragraph)[1];
    assert!(doc.value::<Annotation>(node).is_some());
    let parts = doc.children(node);
    assert_eq!(parts.len(), 2);
    for &part in parts {
        assert!(doc.value::<InlineContainerPart>(part).is_some());
        assert_eq!(doc.parent(part), Some(node));
        for &child in doc.children(part) {
            assert_eq!(doc.parent(child), Some(part));
        }
    }
    #[cfg(feature = "sourcemap")]
    {
        let source = doc.source();
        for (&part, body) in parts.iter().zip(["**文字**", "*说明* & <"]) {
            let (start, end) = doc.node(part).srcmap().unwrap().get_byte_offsets();
            assert_eq!(&source[start..end], body);
        }
    }
}

fn single_part_plugin() -> PluginSpec {
    PluginSpec::new("single-part")
        .inline_container::<Annotation>("single-part")
        .marker('{')
        .parse(|cx| {
            let source = cx.source();
            let end = source.find('}')?;
            cx.part(1..end);
            cx.advance_bytes(end + 1);
            Some(Annotation)
        })
        .render_html(|_, html| {
            let part = html.render_part(0);
            let missing = html.render_part(1);
            html.element("mark").raw(part).raw(missing);
        })
        .finish()
}

#[test]
fn single_range_containers_also_expose_one_part() {
    let md = MarkdownIt::builder().plugin(single_part_plugin()).build();
    for (source, expected) in [
        ("{*x*}", "<p><mark><em>x</em></mark></p>\n"),
        ("{}", "<p><mark></mark></p>\n"),
    ] {
        let doc = md.parse(source);
        assert_eq!(md.render_document(&doc, "html"), expected);
        let paragraph = doc.children(doc.root())[0];
        let node = doc.children(paragraph)[0];
        let parts = doc.children(node);
        assert_eq!(parts.len(), 1);
        assert!(doc.value::<InlineContainerPart>(parts[0]).is_some());
    }
}

#[test]
fn empty_parts_render_in_formats_without_a_part_renderer() {
    let plugin = PluginSpec::new("html-only")
        .inline_container::<Annotation>("html-only")
        .marker('{')
        .parse(|cx| {
            let source = cx.source();
            let end = source.find('}')?;
            cx.part(1..end);
            cx.part(1..1);
            cx.advance_bytes(end + 1);
            Some(Annotation)
        })
        .html_tag("span")
        .finish();
    let md = MarkdownIt::builder().plugin(plugin).build();
    let doc = md.parse("{x}");
    assert_eq!(md.render_document(&doc, "html"), "<p><span>x</span></p>\n");
    assert_eq!(md.render_document(&doc, "text"), "x\n");
}

#[test]
fn empty_parts_keep_their_indices() {
    let md = MarkdownIt::builder().plugin(annotation()).build();
    for (source, html, text) in [
        ("[]{说明}", "<ruby><rt>说明</rt></ruby>", "(说明)"),
        ("[文字]{}", "<ruby>文字<rt></rt></ruby>", "文字()"),
        ("[]{}", "<ruby><rt></rt></ruby>", "()"),
    ] {
        let doc = md.parse(source);
        assert_eq!(md.render_document(&doc, "html"), format!("<p>{html}</p>\n"));
        assert_eq!(md.render_document(&doc, "text"), format!("{text}\n"));
    }
}

#[test]
fn failed_suffix_rolls_back_and_keeps_normal_links() {
    let md = MarkdownIt::builder().plugin(annotation()).build();
    assert_eq!(md.render("[文字]{未结束"), "<p>[文字]{未结束</p>\n");
    assert_eq!(
        md.render("[文字](/url)"),
        "<p><a href=\"/url\">文字</a></p>\n"
    );
    assert_eq!(
        md.render("[文字]{[链接](/url)}"),
        "<p><ruby>文字<rt><a href=\"/url\">链接</a></rt></ruby></p>\n"
    );
}

#[test]
fn nesting_escapes_and_child_sessions_are_preserved() {
    let md = MarkdownIt::builder().plugin(annotation()).build();
    assert_eq!(
        md.render("[外[内]{注}]{说明}"),
        "<p><ruby>外<ruby>内<rt>注</rt></ruby><rt>说明</rt></ruby></p>\n"
    );
    assert_eq!(
        md.render(r"[文\]字]{说\}明}"),
        "<p><ruby>文]字<rt>说}明</rt></ruby></p>\n"
    );
    assert_eq!(
        md.render("[*文字]{说明*}"),
        "<p><ruby>*文字<rt>说明*</rt></ruby></p>\n"
    );
    let mut limited = MarkdownIt::builder().plugin(annotation()).build();
    limited.max_nesting = 1;
    assert_eq!(
        limited.render("[*文字*]{*说明*}"),
        "<p><ruby>*文字*<rt>*说明*</rt></ruby></p>\n"
    );
}

fn parts_plugin(ranges: Vec<Range<usize>>, consumed: usize) -> PluginSpec {
    PluginSpec::new("parts")
        .inline_container::<Annotation>("parts")
        // Renderers may be registered before the parser.
        .html_tag("span")
        .render_text(|_, text| text.render_children())
        .parse(move |cx| {
            if !cx.advance_bytes(consumed) {
                return None;
            }
            for range in &ranges {
                cx.part_at(range.clone());
            }
            Some(Annotation)
        })
        .finish()
}

#[test]
fn render_children_transparently_renders_all_parts_in_returned_order() {
    let md = MarkdownIt::builder()
        .plugin(parts_plugin(vec![3..4, 2..2, 1..2], 5))
        .build();
    let doc = md.parse("[a|b]");
    assert_eq!(md.render_document(&doc, "html"), "<p><span>ba</span></p>\n");
    assert_eq!(md.render_document(&doc, "text"), "ba\n");
    let md = MarkdownIt::builder()
        .plugin(parts_plugin(vec![], 1))
        .build();
    let doc = md.parse("x");
    assert_eq!(md.render_document(&doc, "html"), "<p><span></span></p>\n");
    let paragraph = doc.children(doc.root())[0];
    let node = doc.children(paragraph)[0];
    assert!(doc.children(node).is_empty());
}

#[test]
#[allow(clippy::reversed_empty_ranges, clippy::single_range_in_vec_init)] // the invalid ranges are the test input
fn invalid_parts_are_rejected() {
    for ranges in [vec![1..2], vec![0..99], vec![2..1], vec![0..3, 4..5]] {
        let md = MarkdownIt::builder()
            .plugin(parts_plugin(ranges, 3))
            .build();
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| md.parse("雪tail"))).is_err()
        );
    }
    let md = MarkdownIt::builder()
        .plugin(parts_plugin(vec![], 0))
        .build();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| md.parse("x"))).is_err());
}
