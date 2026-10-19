use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use markdown_it::plugins::sourcepos;
use markdown_it::{Document, EditBatch, MarkdownIt, StructuralEvent};
use markdown_it_benchmarks::corpus;

fn parser() -> MarkdownIt {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::html::add(&mut md);
    md
}

fn benchmark(c: &mut Criterion) {
    let parser = parser();
    let standard = corpus::standard();

    for corpus in standard {
        let name = corpus.name;
        let source = corpus.source();
        let mut group = c.benchmark_group(format!("sourcepos-transform/corpus/{name}"));
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_function("build-and-commit-edits", |b| {
            b.iter_batched(
                || parser.parse(source),
                |mut document| {
                    sourcepos::edits(black_box(&document)).commit(black_box(&mut document));
                    black_box(document);
                },
                BatchSize::SmallInput,
            )
        });
        group.bench_function("document-build-edits", |b| {
            let document = parser.parse(source);
            // Diagnostic phase only: its timing includes dropping the
            // uncommitted batch, so it is not additive with document-commit.
            b.iter(|| black_box(sourcepos::edits(black_box(&document))))
        });
        group.bench_function("document-commit", |b| {
            b.iter_batched(
                || {
                    let document = parser.parse(source);
                    let edits = sourcepos::edits(&document);
                    (document, edits)
                },
                |(mut document, edits)| {
                    edits.commit(black_box(&mut document));
                    black_box(document);
                },
                BatchSize::SmallInput,
            )
        });
        group.bench_function("document-commit-ordered", |b| {
            // Use a constant value on every node to isolate the effect of
            // attribute edit ordering from source-position formatting.
            b.iter_batched(
                || attribute_commit_input(&parser, source, false),
                |(mut document, edits)| {
                    edits.commit(black_box(&mut document));
                    black_box(document);
                },
                BatchSize::SmallInput,
            )
        });
        group.bench_function("document-commit-reversed", |b| {
            b.iter_batched(
                || attribute_commit_input(&parser, source, true),
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

fn attribute_commit_input(
    parser: &MarkdownIt,
    source: &str,
    reversed: bool,
) -> (Document, EditBatch) {
    let document = parser.parse(source);
    let mut nodes: Vec<_> = document
        .events(document.root())
        .filter_map(|event| match event {
            StructuralEvent::Enter(node) | StructuralEvent::Leaf(node) => Some(node.id()),
            StructuralEvent::Exit(_) => None,
        })
        .collect();
    if reversed {
        nodes.reverse();
    }
    let mut edits = EditBatch::new();
    for node in nodes {
        edits.set_attribute(node, "data-profile", "value");
    }
    (document, edits)
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
