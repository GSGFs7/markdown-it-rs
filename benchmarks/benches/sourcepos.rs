use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use markdown_it::parser::core::CoreRule;
use markdown_it::plugins::sourcepos::{self, SourcePosDocumentTransform, SyntaxPosRule};
use markdown_it::{Document, DocumentTransform, EditBatch, MarkdownIt, Node, StructuralEvent};
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
    let transform = SourcePosDocumentTransform;
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
        group.bench_function("document-build-edits", |b| {
            let document = parser.parse_document(source);
            // Diagnostic phase only: its timing includes dropping the
            // uncommitted batch, so it is not additive with document-commit.
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
                    edits.commit(black_box(&mut document)).unwrap();
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
                    edits.commit(black_box(&mut document)).unwrap();
                    black_box(document);
                },
                BatchSize::SmallInput,
            )
        });
        group.bench_function("document-commit-reversed", |b| {
            b.iter_batched(
                || attribute_commit_input(&parser, source, true),
                |(mut document, edits)| {
                    edits.commit(black_box(&mut document)).unwrap();
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
    let document = parser.parse_document(source);
    let mut nodes: Vec<_> = document
        .events(document.root())
        .unwrap()
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
