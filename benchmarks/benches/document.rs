use std::hint::black_box;

use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use markdown_it_benchmarks::corpus;

fn parser() -> markdown_it::MarkdownIt {
    let mut md = markdown_it::MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::html::add(&mut md);
    md
}

fn benchmark(c: &mut Criterion) {
    let md = parser();

    for corpus in corpus::standard() {
        let source = corpus.source();
        let legacy_html = md.parse(source).render();
        let document_html = md.parse_document(source).into_legacy().render();
        assert_eq!(legacy_html, document_html, "{} roundtrip", corpus.name);

        let mut group = c.benchmark_group(format!("document-build/{}", corpus.name));
        group.throughput(Throughput::Bytes(corpus.len() as u64));
        group.bench_function("legacy-tree", |b| {
            b.iter(|| black_box(md.parse(black_box(source))))
        });
        group.bench_function("arena-document", |b| {
            b.iter(|| black_box(md.parse_document(black_box(source))))
        });
        group.bench_function("legacy-to-arena", |b| {
            b.iter_batched(
                || md.parse(source),
                |root| black_box(markdown_it::Document::from_legacy(black_box(source), root)),
                BatchSize::SmallInput,
            )
        });
        group.finish();
    }
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
