use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use markdown_it::parser::core::{CoreRule, CoreState};
use markdown_it::plugins::extra::smartquotes::ClassicSmartQuotesRule;
use markdown_it::{Document, MarkdownIt, NodeDraft};

fn synthetic(count: usize, split: bool) -> Document {
    let mut root = NodeDraft::new(markdown_it::Root::new(String::new()));
    if split {
        for _ in 0..count {
            root.push_child(NodeDraft::new(markdown_it::Text {
                content: "\"word\" ".into(),
            }));
        }
    } else {
        root.push_child(NodeDraft::new(markdown_it::Text {
            content: "\"word\" ".repeat(count),
        }));
    }
    Document::from_draft("", root)
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
