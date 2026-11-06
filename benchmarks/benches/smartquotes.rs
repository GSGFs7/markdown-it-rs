use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use markdown_it::document::Document;
use markdown_it::parser::core::{CoreRule, CoreState};
use markdown_it::plugins::extra::smartquotes::ClassicSmartQuotesRule;
use markdown_it::MarkdownIt;

fn synthetic(count: usize, split: bool) -> Document {
    let mut document = Document::new("", markdown_it::document::Root::new(""));
    let root = document.root();
    if split {
        for _ in 0..count {
            let text = document.create_node(markdown_it::document::Text {
                content: "\"word\" ".into(),
            });
            document.push_child(root, text);
        }
    } else {
        let text = document.create_node(markdown_it::document::Text {
            content: "\"word\" ".repeat(count),
        });
        document.push_child(root, text);
    }
    document
}

fn benchmark(c: &mut Criterion) {
    let md = MarkdownIt::empty();
    let rule = ClassicSmartQuotesRule::default();
    for split in [false, true] {
        for count in [100, 1_000, 10_000] {
            let mut group = c.benchmark_group(format!("smartquotes/split-{split}/{count}"));
            group.throughput(Throughput::Elements(count as u64));
            group.bench_function("core-rule", |b| {
                b.iter_batched(
                    || synthetic(count, split),
                    |mut doc| {
                        rule.run(&mut CoreState {
                            source: "",
                            md: &md,
                            document: black_box(&mut doc),
                        });
                        black_box(doc);
                    },
                    BatchSize::SmallInput,
                )
            });
            group.finish();
        }
    }
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
