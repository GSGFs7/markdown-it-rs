#![no_main]

use std::sync::LazyLock;

use libfuzzer_sys::fuzz_target;
use markdown_it::MarkdownIt;

static PARSER: LazyLock<MarkdownIt> = LazyLock::new(|| {
    let mut parser = MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut parser);
    markdown_it::plugins::html::add(&mut parser);
    markdown_it::plugins::extra::add(&mut parser);
    parser
});

fuzz_target!(|source: &str| {
    let document = PARSER.parse(source);
    std::hint::black_box(PARSER.render_document(&document, "html"));
});
