use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use markdown_it::parser::core::CoreRule;
use markdown_it::plugins::sourcepos::{self, SyntaxPosRule};
use markdown_it::{Document, MarkdownIt, Node};
use markdown_it_benchmarks::corpus;

fn transform_legacy(root: &mut Node, md: &MarkdownIt) {
    SyntaxPosRule::run(root, md);
}

fn transform_registered(document: &mut Document, md: &MarkdownIt) {
    md.run_document_transforms(document).unwrap();
}

fn parser() -> MarkdownIt {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::html::add(&mut md);
    md
}

fn benchmark(c: &mut Criterion) {
    let parser = parser();
    let mut document_transforms = MarkdownIt::empty();
    sourcepos::add_document(&mut document_transforms);
    let standard = corpus::standard();

    for corpus in standard {
        let name = corpus.name;
        let source = corpus.source();
        let mut legacy = parser.parse(source);
        transform_legacy(&mut legacy, &parser);
        let legacy_html = legacy.render();
        let mut document = parser.parse_document(source);
        transform_registered(&mut document, &document_transforms);
        assert_eq!(legacy_html, document.into_legacy().render(), "{name}");

        let mut group = c.benchmark_group(format!("sourcepos-transform/corpus/{name}"));
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_function("legacy", |b| {
            b.iter_batched(
                || parser.parse(source),
                |mut root| {
                    transform_legacy(black_box(&mut root), black_box(&parser));
                    black_box(root);
                },
                BatchSize::SmallInput,
            )
        });
        group.bench_function("document-registry", |b| {
            b.iter_batched(
                || parser.parse_document(source),
                |mut document| {
                    transform_registered(black_box(&mut document), black_box(&document_transforms));
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
