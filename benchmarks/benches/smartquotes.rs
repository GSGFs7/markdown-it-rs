use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use markdown_it::parser::core::CoreRule;
use markdown_it::parser::inline::Text;
use markdown_it::plugins::extra::smartquotes::SmartQuotesRule;
use markdown_it::{MarkdownIt, Node};

type ClassicSmartQuotes = SmartQuotesRule<'‘', '’', '“', '”'>;

fn text(content: String) -> Node {
    Node::new(Text { content })
}

fn one_large_text(quote_count: usize) -> Node {
    let mut root = Node::default();
    root.children.push(text("\"".repeat(quote_count)));
    root
}

fn many_small_texts(text_count: usize) -> Node {
    let mut root = Node::default();
    root.children.reserve(text_count);
    for _ in 0..text_count {
        root.children.push(text("\"a\" ".to_owned()));
    }
    root
}

fn transform(root: &mut Node, md: &MarkdownIt) {
    <ClassicSmartQuotes as CoreRule>::run(root, md);
}

fn assert_transformed(mut root: Node, md: &MarkdownIt) {
    transform(&mut root, md);
    root.walk(|node, _| {
        if let Some(text) = node.cast::<Text>() {
            assert!(!text.content.contains('"'));
        }
    });
}

fn benchmark(c: &mut Criterion) {
    let md = MarkdownIt::empty();
    let mut group = c.benchmark_group("smartquotes-transform/one-text");
    for quote_count in [70_000, 140_000, 280_000] {
        assert_transformed(one_large_text(quote_count), &md);
        group.throughput(Throughput::Elements(quote_count as u64));
        group.bench_function(quote_count.to_string(), |b| {
            b.iter_batched(
                || one_large_text(quote_count),
                |mut root| {
                    transform(black_box(&mut root), black_box(&md));
                    black_box(root);
                },
                BatchSize::SmallInput,
            )
        });
    }
    group.finish();

    let mut small = c.benchmark_group("smartquotes-transform/many-text-nodes");
    for text_count in [17_500, 35_000, 70_000] {
        assert_transformed(many_small_texts(text_count), &md);
        small.throughput(Throughput::Elements(text_count as u64));
        small.bench_function(text_count.to_string(), |b| {
            b.iter_batched(
                || many_small_texts(text_count),
                |mut root| {
                    transform(black_box(&mut root), black_box(&md));
                    black_box(root);
                },
                BatchSize::SmallInput,
            )
        });
    }
    small.finish();
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
