#[test]
fn front_matter_extracts_yaml_and_does_not_render() {
    use markdown_it::parser::core::Root;
    use markdown_it::plugins::extra::front_matter::{FrontMatter, FrontMatterKind};

    let md = &mut markdown_it::MarkdownIt::new();
    markdown_it::plugins::extra::front_matter::add(md);
    markdown_it::plugins::cmark::add(md);

    let ast = md.parse("---\ntitle: Hello\ntags:\n  - rust\n---\n# Post\n");
    let root = ast.cast::<Root>().unwrap();
    let front_matter = root.ext.get::<FrontMatter>().unwrap();

    assert_eq!(front_matter.kind, FrontMatterKind::Yaml);
    assert_eq!(front_matter.raw, "title: Hello\ntags:\n  - rust");
    assert_eq!(front_matter.start_line, 0);
    assert_eq!(front_matter.end_line, 4);
    assert_eq!(ast.render(), "<h1>Post</h1>\n");
}

#[test]
fn front_matter_extracts_toml() {
    use markdown_it::parser::core::Root;
    use markdown_it::plugins::extra::front_matter::{FrontMatter, FrontMatterKind};

    let md = &mut markdown_it::MarkdownIt::new();
    markdown_it::plugins::extra::front_matter::add(md);
    markdown_it::plugins::cmark::add(md);

    let ast = md.parse("+++\ntitle = \"Hello\"\n+++\nBody");
    let root = ast.cast::<Root>().unwrap();
    let front_matter = root.ext.get::<FrontMatter>().unwrap();

    assert_eq!(front_matter.kind, FrontMatterKind::Toml);
    assert_eq!(front_matter.raw, "title = \"Hello\"");
    assert_eq!(ast.render(), "<p>Body</p>\n");
}

#[test]
fn front_matter_can_be_parsed_by_user_callback() {
    use markdown_it::parser::core::Root;
    use markdown_it::plugins::extra::front_matter::{FrontMatter, FrontMatterKind};

    #[derive(Debug, PartialEq, Eq)]
    struct Metadata {
        title: String,
    }

    let md = &mut markdown_it::MarkdownIt::new();
    markdown_it::plugins::extra::front_matter::add(md);
    markdown_it::plugins::cmark::add(md);

    let ast = md.parse("---\ntitle: Hello\n---\nBody");
    let root = ast.cast::<Root>().unwrap();
    let front_matter = root.ext.get::<FrontMatter>().unwrap();

    let metadata = front_matter
        .parse_with(|kind, raw| match kind {
            FrontMatterKind::Yaml => raw
                .strip_prefix("title: ")
                .map(|title| Metadata {
                    title: title.to_owned(),
                })
                .ok_or("missing title"),
            FrontMatterKind::Toml => Err("unsupported front matter format"),
        })
        .unwrap();

    assert_eq!(
        metadata,
        Metadata {
            title: "Hello".to_owned(),
        }
    );
}

#[test]
fn front_matter_respects_max_line_limit() {
    use markdown_it::parser::core::Root;
    use markdown_it::plugins::extra::front_matter::FrontMatter;

    let md = &mut markdown_it::MarkdownIt::new();
    markdown_it::plugins::extra::front_matter::add_with_max_lines(md, 3);
    markdown_it::plugins::cmark::add(md);

    let ast = md.parse("---\ntitle: Hello\nstill: metadata\n---\nBody");
    let root = ast.cast::<Root>().unwrap();

    assert!(root.ext.get::<FrontMatter>().is_none());
    assert_eq!(
        ast.render(),
        "<hr>\n<h2>title: Hello\nstill: metadata</h2>\n<p>Body</p>\n"
    );
}
