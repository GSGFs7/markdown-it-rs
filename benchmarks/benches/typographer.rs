use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use markdown_it::parser::core::{CoreRule, CoreState};
use markdown_it::plugins::extra::typographer::TypographerRule;
use markdown_it::MarkdownIt;
use markdown_it_benchmarks::corpus;

fn parser() -> MarkdownIt {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::html::add(&mut md);
    md
}

fn benchmark(c: &mut Criterion) {
    let parser = parser();
    let rule = TypographerRule;

    for corpus in corpus::standard() {
        let source = corpus.source();
        let mut group = c.benchmark_group(format!("typographer-transform/corpus/{}", corpus.name));
        group.throughput(Throughput::Bytes(corpus.len() as u64));
        group.bench_function("core-rule", |b| {
            b.iter_batched(
                || parser.parse(source),
                |mut document| {
                    rule.run(&mut CoreState {
                        source,
                        md: &parser,
                        document: black_box(&mut document),
                    });
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
