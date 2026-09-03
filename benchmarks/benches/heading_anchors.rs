use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use markdown_it::parser::core::CoreRule;
use markdown_it::plugins::extra::heading_anchors::{
    self,
    AddHeadingAnchors,
    HeadingAnchorsOptions,
};
use markdown_it::{Document, MarkdownIt, Node};
use markdown_it_benchmarks::corpus;

fn transform_legacy(root: &mut Node, md: &MarkdownIt) {
    AddHeadingAnchors::run(root, md);
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

fn heading_heavy() -> String {
    let mut source = String::with_capacity(corpus::LARGE_CORPUS_BYTES);
    let mut index = 0;
    while source.len() < corpus::LARGE_CORPUS_BYTES {
        source.push_str(&format!(
            "# Repeated heading {index}\n\nSection *with emphasis* &amp; Unicode 标题\n---\n\n"
        ));
        index += 1;
    }
    while source.len() > corpus::LARGE_CORPUS_BYTES {
        source.pop();
    }
    source
}

fn benchmark(c: &mut Criterion) {
    let mut parser = parser();
    parser.ext.insert(HeadingAnchorsOptions::default());
    let mut document_transforms = MarkdownIt::empty();
    heading_anchors::add_document(&mut document_transforms);
    let standard = corpus::standard();
    let heading_heavy = heading_heavy();

    for (name, source) in standard
        .iter()
        .map(|corpus| (corpus.name, corpus.source()))
        .chain(std::iter::once(("heading-heavy", heading_heavy.as_str())))
    {
        let mut legacy = parser.parse(source);
        transform_legacy(&mut legacy, &parser);
        let legacy_html = legacy.render();
        let mut document = parser.parse_document(source);
        transform_registered(&mut document, &document_transforms);
        assert_eq!(legacy_html, document.into_legacy().render(), "{name}");

        let mut group = c.benchmark_group(format!("heading-anchors-transform/corpus/{name}"));
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
