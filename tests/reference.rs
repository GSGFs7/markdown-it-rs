use markdown_it::MarkdownIt;
use markdown_it::presets::Preset;

#[test]
fn reference_title_requires_whitespace_after_destination() {
    let mut md = MarkdownIt::with_preset(Preset::MarkdownItDefault);
    md.render_options.breaks = true;
    md.render_options.xhtml_out = true;
    assert_eq!(
        md.render("[foo]: <bar> (\naz)\n\n[foo]\no]\n"),
        "<p><a href=\"bar\" title=\"\naz\">foo</a><br />\no]</p>\n",
    );
    for (open, close) in [('(', ')'), ('\'', '\''), ('"', '"')] {
        assert_eq!(
            md.render(&format!("[foo]: <bar> {open}a\nb{close}\n\n[foo]\n")),
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
        for prefix in ["", "> ", "- "] {
            let continuation = if prefix == "- " { "  " } else { prefix };
            let (before, after) = match prefix {
                "> " => ("<blockquote>\n<p>", "</p>\n</blockquote>\n"),
                "- " => ("<ul>\n<li>", "</li>\n</ul>\n"),
                _ => ("<p>", "</p>\n"),
            };
            assert_eq!(
                md.render(&format!(
                    "{prefix}[foo]: <bar>{open}a\n{continuation}b{close}\n"
                )),
                format!("{before}[foo]: &lt;bar&gt;{escaped_open}a<br />\nb{escaped_close}{after}"),
            );
        }
    }
}

#[test]
fn adjacent_multiline_title_in_blockquote_remains_visible() {
    for preset in [Preset::MarkdownItDefault, Preset::CommonMark] {
        let md = MarkdownIt::with_preset(preset);
        let destination = match preset {
            Preset::CommonMark => "<bar>",
            _ => "&lt;bar&gt;",
        };
        assert_eq!(
            md.render("> [foo]: <bar>\"a\n> b\"\n>  \t\n> \nar\n"),
            format!(
                "<blockquote>\n<p>[foo]: {destination}&quot;a\nb&quot;</p>\n</blockquote>\n<p>ar</p>\n"
            ),
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

#[test]
fn reference_links_are_literal_without_the_reference_plugin() {
    use markdown_it::plugins::cmark::{block, inline};

    let mut md = MarkdownIt::empty();
    block::paragraph::add(&mut md);
    inline::link::add(&mut md);
    inline::image::add(&mut md);

    assert_eq!(md.render("[id][ref]"), "<p>[id][ref]</p>\n");
    assert_eq!(md.render("[id]"), "<p>[id]</p>\n");
    assert_eq!(md.render("![id][ref]"), "<p>![id][ref]</p>\n");

    // Inline targets do not depend on the reference plugin.
    assert_eq!(md.render("[id](/url)"), "<p><a href=\"/url\">id</a></p>\n");
}

#[test]
fn injected_reference_map_resolves_without_the_definition_scanner() {
    use markdown_it::parser::block::BlockParserRule;
    use markdown_it::parser::core::{CoreRule, CoreState};
    use markdown_it::plugins::cmark::{block, inline};
    use markdown_it::reference::ReferenceMap;

    #[derive(Default)]
    struct AddReferences;

    impl CoreRule for AddReferences {
        fn run(&self, state: &mut CoreState<'_>) {
            let mut references = ReferenceMap::default();
            references.insert("id".into(), "/external".into(), None);
            state.root_ext_mut().insert(references);
        }
    }

    let mut md = MarkdownIt::empty();
    block::paragraph::add(&mut md);
    inline::link::add(&mut md);
    md.add_rule::<AddReferences>().before::<BlockParserRule>();

    assert_eq!(md.render("[id]"), "<p><a href=\"/external\">id</a></p>\n");
}
