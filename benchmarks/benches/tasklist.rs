use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use markdown_it::plugins::extra::tasklist;
use markdown_it::{Document, MarkdownIt};
use markdown_it_benchmarks::corpus;

fn run_postprocessors(document: &mut Document, md: &MarkdownIt) {
    md.run_postprocessors(document);
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
    let mut postprocessors = MarkdownIt::empty();
    tasklist::add(&mut postprocessors);
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
                || parser.parse(source),
                |mut document| {
                    run_postprocessors(black_box(&mut document), black_box(&postprocessors));
                    black_box(document);
                },
                BatchSize::SmallInput,
            )
        });
        group.bench_function("document-build-edits", |b| {
            let document = parser.parse(source);
            b.iter(|| black_box(tasklist::edits(black_box(&document))))
        });
        group.bench_function("document-commit", |b| {
            b.iter_batched(
                || {
                    let document = parser.parse(source);
                    let edits = tasklist::edits(&document);
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
