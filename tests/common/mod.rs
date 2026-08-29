use markdown_it::{MarkdownIt, Preset};

pub fn markdown_it_fixture_parser() -> MarkdownIt {
    let mut md = MarkdownIt::with_preset(Preset::MarkdownItDefault);

    markdown_it::plugins::html::add(&mut md);
    markdown_it::plugins::cmark::block::fence::set_lang_prefix(&mut md, "");
    markdown_it::plugins::extra::typographer::add(&mut md);
    markdown_it::plugins::extra::smartquotes::add(&mut md);
    #[cfg(feature = "linkify")]
    markdown_it::plugins::extra::linkify::add(&mut md);

    md
}
