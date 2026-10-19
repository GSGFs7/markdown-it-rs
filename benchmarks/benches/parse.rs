use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use markdown_it_benchmarks::{corpus, parser};

fn benchmark(c: &mut Criterion) {
    for configuration in parser::parse_configurations() {
        let configuration_name = configuration.name;
        let md = configuration.parser;
        for corpus in corpus::parser_emphasis_checkpoint().into_iter().chain(
            corpus::standard()
                .into_iter()
                .filter(|input| input.kind == corpus::CorpusKind::PathologicalSmoke),
        ) {
            let source = corpus.source();
            let mut group =
                c.benchmark_group(format!("parse/{configuration_name}/{}", corpus.name));
            group.throughput(Throughput::Bytes(corpus.len() as u64));
            group.bench_function("markdown-it-rs", |b| {
                b.iter(|| black_box(md.parse(black_box(source))))
            });
            group.finish();
        }
    }
}

fn deferred_inline_siblings(c: &mut Criterion) {
    let mut md = markdown_it::MarkdownIt::empty();
    markdown_it::plugins::cmark::inline::emphasis::add(&mut md);
    let mut group = c.benchmark_group("parse/deferred-inline-siblings");
    for lines in [4_000, 8_000, 16_000] {
        let source = "*a*\n".repeat(lines);
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(lines), &source, |b, source| {
            b.iter(|| black_box(md.parse(black_box(source))))
        });
    }
    group.finish();
}

// Keep construction, cold first parse, and warmed repeated parse separate.
fn parser_lifecycle(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse/lifecycle");
    group.bench_function("construct-default", |b| {
        b.iter(|| black_box(markdown_it::MarkdownIt::new()))
    });
    for (name, source) in [
        ("short", "Hello **world** and [link](/url)."),
        ("plain", "A short plain text paragraph."),
    ] {
        // Construction is outside timing; the consumed parser is dropped inside it.
        group.bench_function(format!("first-parse/{name}"), |b| {
            b.iter_batched(
                markdown_it::MarkdownIt::new,
                |md| black_box(md.parse(black_box(source))),
                BatchSize::SmallInput,
            )
        });
        let md = markdown_it::MarkdownIt::new();
        black_box(md.parse(source));
        group.bench_function(format!("repeated-parse/{name}"), |b| {
            b.iter(|| black_box(md.parse(black_box(source))))
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    benchmark,
    deferred_inline_siblings,
    parser_lifecycle
);
criterion_main!(benches);
