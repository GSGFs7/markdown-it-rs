use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use markdown_it::MarkdownIt;
use markdown_it_benchmarks::corpus;

fn benchmark(c: &mut Criterion) {
    let mut commonmark = MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut commonmark);
    markdown_it::plugins::html::add(&mut commonmark);

    let mut sources: Vec<_> = corpus::standard()
        .into_iter()
        .map(|corpus| (corpus.name, corpus.source().to_owned()))
        .collect();
    sources.extend([
        ("short-plain", "A short paragraph.".to_owned()),
        ("short-link", "[label](url)".to_owned()),
        (
            "deep-blockquote",
            format!("{}[label](url)\n", "> ".repeat(128)),
        ),
        ("brackets-no-target", "[label] ".repeat(4096)),
        ("target-no-close", format!("{}(url)", "[a".repeat(16384))),
        ("simple-links", "[label](url) ".repeat(4096)),
        ("complex-labels", "[*雪* `code` &amp;](url) ".repeat(2048)),
        (
            "reference-links",
            format!("[r]: /url\n\n{}", "[label][r] [r] ".repeat(2048)),
        ),
        ("nested-labels", "[[[a]]](url) ".repeat(2048)),
        ("special-html", format!("{} <!-- ok -->", "[a".repeat(4096))),
    ]);

    for (configuration, md) in [
        ("commonmark-html", commonmark),
        ("default", MarkdownIt::new()),
    ] {
        for (name, source) in &sources {
            let mut group = c.benchmark_group(format!("link-preflight/{configuration}/{name}"));
            group.throughput(Throughput::Bytes(source.len() as u64));
            black_box(md.render(source));
            group.bench_function("parse", |b| {
                b.iter(|| black_box(md.parse(black_box(source))))
            });
            group.bench_function("parse-render", |b| {
                b.iter(|| black_box(md.render(black_box(source))))
            });
            group.finish();
        }
    }
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
