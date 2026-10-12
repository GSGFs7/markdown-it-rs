use markdown_it::{MarkdownIt, Preset};

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
