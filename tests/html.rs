use markdown_it::{MarkdownIt, Preset};

#[test]
fn html_uses_javascript_whitespace() {
    let md = MarkdownIt::with_preset(Preset::CommonMark);
    for space in [
        '\t', '\u{b}', '\u{c}', ' ', '\u{a0}', '\u{1680}', '\u{2000}', '\u{200a}', '\u{2028}',
        '\u{2029}', '\u{202f}', '\u{205f}', '\u{3000}', '\u{feff}',
    ] {
        for source in [
            format!("<b{space}>"),
            format!("</b{space}>"),
            format!("<b{space}title{space}={space}'x'>"),
            format!("<b>{space}"),
            format!("<div{space}?>\n*raw*"),
            format!("<script{space}?>\n\n*raw*"),
        ] {
            assert_eq!(md.render(&source), source, "source: {source:?}");
        }
        for tag in [
            format!("<b{space}>"),
            format!("</b{space}>"),
            format!("<b{space}title{space}={space}'x'>"),
        ] {
            let source = format!("before {tag}after");
            assert_eq!(
                md.render(&source),
                format!("<p>{source}</p>\n"),
                "source: {source:?}"
            );
        }
    }
    // Rust's Unicode whitespace includes NEL, while JavaScript's \s does not.
    for (source, expected) in [
        ("<b\u{85}>", "<p>&lt;b\u{85}&gt;</p>\n"),
        ("</b\u{85}>", "<p>&lt;/b\u{85}&gt;</p>\n"),
        ("<b>\u{85}", "<p><b>\u{85}</p>\n"),
        (
            "<div\u{85}?>\n*text*",
            "<p>&lt;div\u{85}?&gt;\n<em>text</em></p>\n",
        ),
        (
            "<script\u{85}?>\n*text*",
            "<p>&lt;script\u{85}?&gt;\n<em>text</em></p>\n",
        ),
    ] {
        assert_eq!(md.render(source), expected, "source: {source:?}");
    }
}

#[cfg(feature = "linkify")]
#[test]
fn html_anchor_whitespace_preserves_linkify_scope() {
    let mut md = MarkdownIt::with_preset(Preset::CommonMark);
    markdown_it::plugins::extra::linkify::add(&mut md);
    for (open, close) in [("a", "a"), ("A", "A"), ("a", "A")] {
        let source = format!("<{open}\u{feff}>a@example.com</{close}\u{feff}> b@example.com");
        assert_eq!(
            md.render(&source),
            format!(
                "<p><{open}\u{feff}>a@example.com</{close}\u{feff}> <a href=\"mailto:b@example.com\">b@example.com</a></p>\n"
            )
        );
    }
}

#[test]
fn nul_in_unquoted_attribute_value() {
    let md = MarkdownIt::with_preset(Preset::CommonMark);
    for (source, expected) in [
        (
            "`Foo\n----\n`\0c\n\n<a title=\0_>!)t\n---\nof dashes\"/>\n",
            "<h2>`Foo</h2>\n<p>`�c</p>\n<h2><a title=�_>!)t</h2>\n<p>of dashes&quot;/&gt;</p>\n",
        ),
        ("foo <a title=\0_>bar\n", "<p>foo <a title=�_>bar</p>\n"),
        ("<custom title=\0>\n*text*\n", "<custom title=�>\n*text*\n"),
        (
            "foo <a title=\u{1}_>bar\n",
            "<p>foo &lt;a title=\u{1}_&gt;bar</p>\n",
        ),
    ] {
        assert_eq!(md.render(source), expected, "source: {source:?}");
    }
}
