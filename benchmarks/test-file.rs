use std::hint::black_box;

use comrak::{format_html, parse_document, Arena, Options};
use criterion::{criterion_group, criterion_main, Criterion};

pub fn benchmark(c: &mut Criterion) {
    let source = std::fs::read_to_string("test-file.md").unwrap();
    let md = &mut markdown_it::MarkdownIt::new();
    markdown_it::plugins::cmark::add(md);
    markdown_it::plugins::html::add(md);
    c.bench_function("markdown-it", |b| {
        b.iter(|| {
            let html = md.parse(&source).render();
            black_box(html);
        })
    });

    let md = &mut markdown_it_v6::MarkdownIt::new();
    markdown_it_v6::plugins::cmark::add(md);
    markdown_it_v6::plugins::html::add(md);
    c.bench_function("markdown-it-v5", |b| {
        b.iter(|| {
            let html = md.parse(&source).render();
            black_box(html);
        })
    });

    c.bench_function("comrak", |b| {
        b.iter(|| {
            let arena = Arena::new();
            let options = Options::default();
            let root = parse_document(&arena, &source, &options);
            let mut output = String::new();
            format_html(root, &options, &mut output).unwrap();
            black_box(output);
        })
    });
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
