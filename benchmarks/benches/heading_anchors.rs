use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use markdown_it::parser::core::{CoreRule, CoreState};
use markdown_it::plugins::extra::heading_anchors::HeadingAnchorsRule;
use markdown_it::MarkdownIt;
use markdown_it_benchmarks::corpus;

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
    let parser = parser();
    let rule = HeadingAnchorsRule::default();
    let standard = corpus::standard();
    let heading_heavy = heading_heavy();

    for (name, source) in standard
        .iter()
        .map(|corpus| (corpus.name, corpus.source()))
        .chain(std::iter::once(("heading-heavy", heading_heavy.as_str())))
    {
        let mut group = c.benchmark_group(format!("heading-anchors-transform/corpus/{name}"));
        group.throughput(Throughput::Bytes(source.len() as u64));
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
