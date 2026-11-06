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
        ("short-plain", "A short plain paragraph.".to_owned()),
        (
            "short-links",
            "[label](https://example.test) <https://example.test>".to_owned(),
        ),
        (
            "autolinks",
            "<https://example.test/path> <user@example.test>\n\n".repeat(4096),
        ),
        (
            "long-autolink",
            format!("<https://example.test/{}>\n", "a".repeat(64 * 1024)),
        ),
        (
            "unclosed-autolink",
            format!("<https://example.test/{}\n", "雪".repeat(21845)),
        ),
        (
            "sparse-link-label",
            format!(
                "[{}](https://example.test)\n",
                "ordinary words ".repeat(4096)
            ),
        ),
    ]);

    for (configuration, md) in [
        ("commonmark-html", commonmark),
        ("default", MarkdownIt::new()),
    ] {
        for (name, source) in &sources {
            let mut group = c.benchmark_group(format!("memchr/{configuration}/{name}"));
            group.throughput(Throughput::Bytes(source.len() as u64));
            // Repeated, warmed parsing; construction and source generation are excluded.
            // Both operations include destruction of their returned value.
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
