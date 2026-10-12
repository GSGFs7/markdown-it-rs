mod common;

#[test]
fn unclosed_long_run_does_not_suppress_later_code_spans() {
    let md = common::markdown_it_fixture_parser();
    for (source, expected) in [
        (
            "```a``b`c``d`e`\n",
            "<p>```a<code>b`c</code>d<code>e</code></p>\n",
        ),
        (
            "```aaa``b`aaa``bbb`中`<b9!`\t\t#*`eee``\n",
            "<p>```aaa<code>b`aaa</code>bbb<code>中</code>&lt;b9!<code>\t\t#*</code>eee``</p>\n",
        ),
    ] {
        assert_eq!(md.render(source), expected, "source: {source:?}");
    }
}
