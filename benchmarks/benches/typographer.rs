use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use markdown_it::plugins::extra::typographer::{self};
use markdown_it::{Document, MarkdownIt};
use markdown_it_benchmarks::corpus;

fn transform_registered(document: &mut Document, md: &MarkdownIt) {
    md.run_transforms(document);
}

fn parser() -> MarkdownIt {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::html::add(&mut md);
    md
}

fn benchmark(c: &mut Criterion) {
    let parser = parser();
    let mut transforms = MarkdownIt::empty();
    typographer::add(&mut transforms);

    for corpus in corpus::standard() {
        let source = corpus.source();
        let mut group = c.benchmark_group(format!("typographer-transform/corpus/{}", corpus.name));
        group.throughput(Throughput::Bytes(corpus.len() as u64));
        group.bench_function("document-registry", |b| {
            b.iter_batched(
                || parser.parse(source),
                |mut document| {
                    transform_registered(black_box(&mut document), black_box(&transforms));
                    black_box(document);
                },
                BatchSize::SmallInput,
            )
        });
        group.finish();
    }
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
