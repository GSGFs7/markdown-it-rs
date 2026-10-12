use markdown_it::{MarkdownIt, Preset};

#[test]
fn adjacent_multiline_title_matches_markdown_it() {
    let mut md = MarkdownIt::with_preset(Preset::MarkdownItDefault);
    md.render_options.breaks = true;
    md.render_options.xhtml_out = true;
    assert_eq!(
        md.render("[foo]: <bar>(\naz)\n\n[foo]\no]\n"),
        "<p><a href=\"bar\" title=\"\naz\">foo</a><br />\no]</p>\n",
    );
    for (open, close) in [('(', ')'), ('\'', '\''), ('"', '"')] {
        assert_eq!(
            md.render(&format!("[foo]: <bar>{open}a\nb{close}\n\n[foo]\n")),
            "<p><a href=\"bar\" title=\"a\nb\">foo</a></p>\n",
        );
        let escaped_open = if open == '"' {
            "&quot;".to_owned()
        } else {
            open.to_string()
        };
        let escaped_close = if close == '"' {
            "&quot;".to_owned()
        } else {
            close.to_string()
        };
        assert_eq!(
            md.render(&format!("[foo]: <bar>{open}ab{close}\n\n[foo]\n")),
            format!("<p>[foo]: &lt;bar&gt;{escaped_open}ab{escaped_close}</p>\n<p>[foo]</p>\n"),
        );
    }
}
