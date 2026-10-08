use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use markdown_it::plugins::extra::tasklist::{self, TaskListDocumentTransform};
use markdown_it::{Document, DocumentTransform, MarkdownIt};
use markdown_it_benchmarks::corpus;

fn transform_registered(document: &mut Document, md: &MarkdownIt) {
    md.run_document_transforms(document);
}

fn parser() -> MarkdownIt {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::html::add(&mut md);
    md
}

fn tasklist_heavy() -> String {
    let mut source = String::with_capacity(corpus::LARGE_CORPUS_BYTES);
    let mut index = 0;
    while source.len() < corpus::LARGE_CORPUS_BYTES {
        source.push_str(&format!(
            "- [ ] pending {index}\n- [x] completed **item**\n- ordinary item\n  - [X] nested task\n\n1. [ ] ordered task\n\n   details\n\n"
        ));
        index += 1;
    }
    source
}

fn benchmark(c: &mut Criterion) {
    let parser = parser();
    let transform = TaskListDocumentTransform;
    let mut document_transforms = MarkdownIt::empty();
    tasklist::add(&mut document_transforms);
    let standard = corpus::standard();
    let tasklist_heavy = tasklist_heavy();

    for (name, source) in standard
        .iter()
        .map(|corpus| (corpus.name, corpus.source()))
        .chain(std::iter::once(("tasklist-heavy", tasklist_heavy.as_str())))
    {
        let mut group = c.benchmark_group(format!("tasklist-transform/corpus/{name}"));
        group.throughput(Throughput::Bytes(source.len() as u64));
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
        group.bench_function("document-build-edits", |b| {
            let document = parser.parse_document(source);
            b.iter(|| black_box(transform.run(black_box(&document))))
        });
        group.bench_function("document-commit", |b| {
            b.iter_batched(
                || {
                    let document = parser.parse_document(source);
                    let edits = transform.run(&document);
                    (document, edits)
                },
                |(mut document, edits)| {
                    edits.commit(black_box(&mut document));
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
