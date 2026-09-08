use markdown_it::MarkdownIt;

pub struct ParserConfiguration {
    pub name: &'static str,
    pub parser: MarkdownIt,
}

pub fn document_parse_configurations() -> Vec<ParserConfiguration> {
    let empty = MarkdownIt::empty();

    let mut paragraph = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut paragraph);

    let mut migrated_inline = MarkdownIt::empty();
    markdown_it::plugins::cmark::inline::newline::add(&mut migrated_inline);
    markdown_it::plugins::cmark::inline::escape::add(&mut migrated_inline);
    markdown_it::plugins::cmark::inline::backticks::add(&mut migrated_inline);
    markdown_it::plugins::cmark::inline::emphasis::add(&mut migrated_inline);
    markdown_it::plugins::cmark::inline::autolink::add(&mut migrated_inline);
    markdown_it::plugins::cmark::inline::entity::add(&mut migrated_inline);
    markdown_it::plugins::html::html_inline::add(&mut migrated_inline);
    markdown_it::plugins::cmark::block::paragraph::add(&mut migrated_inline);

    vec![
        ParserConfiguration {
            name: "text-fallback",
            parser: empty,
        },
        ParserConfiguration {
            name: "paragraph-text",
            parser: paragraph,
        },
        ParserConfiguration {
            name: "paragraph-migrated-inline",
            parser: migrated_inline,
        },
    ]
}
