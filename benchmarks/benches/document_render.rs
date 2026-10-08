use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use markdown_it_benchmarks::corpus;
fn benchmark(c: &mut Criterion) {
    let mut md = markdown_it::MarkdownIt::new();
    markdown_it::plugins::html::add(&mut md);
    for corpus in corpus::standard() {
        let source = corpus.source();
        let document = md.parse_document(source);
        let mut group = c.benchmark_group(format!("document-render/{}", corpus.name));
        group.throughput(Throughput::Bytes(corpus.len() as u64));
        group.bench_function("arena-html", |b| {
            b.iter(|| black_box(md.render_document(black_box(&document))))
        });
        group.bench_function("parse-render", |b| {
            b.iter(|| black_box(md.render(black_box(source))))
        });
        group.finish();
    }
}
criterion_group!(benches, benchmark);
criterion_main!(benches);
