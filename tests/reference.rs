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
#[test]
fn explicit_reference_label_can_contain_a_resolved_link() {
    let md = markdown_it::MarkdownIt::new();
    assert_eq!(
        md.render("[r]: /url\n[s]: /other\n\n[r][\n[x] [s] end]"),
        "<p>[r][\n[x] <a href=\"/other\">s</a> end]</p>\n",
    );
    // The second label is unresolved, so the first must not fall back to its
    // shortcut reference.
    assert_eq!(
        md.render("[r]: /url\n[s]: /other\n\n![r][\n[x] [s] end]"),
        "<p>![r][\n[x] <a href=\"/other\">s</a> end]</p>\n",
    );
}

#[test]
fn invalid_inline_target_probes_reference_at_the_failed_delimiter() {
    let md = MarkdownIt::new();
    for (source, expected) in [
        (
            "[foo [bar](中b0 \r9[ref]\n\n[ref]: /uri\n",
            "<p>[foo <a href=\"/uri\">bar</a></p>\n",
        ),
        (
            "[![mo9 on](mjpg\09b\r(])][ref]\n\n[ref]: /uri\n",
            "<p>[!<a href=\"/uri\">mo9 on</a></p>\n",
        ),
        (
            "[![moon](moon.jp` 中[ref]\n\n[ref]: /uri\n",
            "<p>[!<a href=\"/uri\">moon</a></p>\n",
        ),
    ] {
        assert_eq!(md.render(source), expected, "{source:?}");
    }
}
