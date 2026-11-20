#![no_main]

use std::sync::LazyLock;

use libfuzzer_sys::fuzz_target;
use markdown_it::MarkdownIt;
use markdown_it::presets::Preset;

static PARSERS: LazyLock<[MarkdownIt; 2]> = LazyLock::new(|| {
    [
        MarkdownIt::new(),
        MarkdownIt::with_preset(Preset::CommonMark),
    ]
});

fuzz_target!(|source: &str| {
    for parser in PARSERS.iter() {
        let document = parser.parse(source);
        std::hint::black_box(parser.render_document(&document, "html"));
    }
});
