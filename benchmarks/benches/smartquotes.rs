use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use markdown_it::Document;
use markdown_it::parser::core::CoreRule;
use markdown_it::parser::inline::Text;
use markdown_it::plugins::extra::smartquotes::{self, SmartQuotesRule};
use markdown_it::{MarkdownIt, Node};
use markdown_it_benchmarks::corpus;

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

fn transform_registered(document: &mut Document, md: &MarkdownIt) {
    md.run_document_transforms(document).unwrap();
}

fn assert_transformed(mut root: Node, md: &MarkdownIt) {
    transform(&mut root, md);
    root.walk(|node, _| {
        if let Some(text) = node.cast::<Text>() {
            assert!(!text.content.contains('"'));
        }
    });
}

fn assert_document_has_no_double_quotes(document: Document) {
    document.into_legacy().walk(|node, _| {
        if let Some(text) = node.cast::<Text>() {
            assert!(!text.content.contains('"'));
        }
    });
}

fn parser() -> MarkdownIt {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::html::add(&mut md);
    md
}

fn benchmark(c: &mut Criterion) {
    let md = MarkdownIt::empty();
    let mut registry_md = MarkdownIt::empty();
    smartquotes::add_document(&mut registry_md);
    let mut group = c.benchmark_group("smartquotes-transform/one-text");
    for quote_count in [70_000, 140_000, 280_000] {
        assert_transformed(one_large_text(quote_count), &md);
        let mut registered = Document::from_legacy("", one_large_text(quote_count));
        transform_registered(&mut registered, &registry_md);
        assert_document_has_no_double_quotes(registered);
        group.throughput(Throughput::Elements(quote_count as u64));
        group.bench_function(format!("legacy/{quote_count}"), |b| {
            b.iter_batched(
                || one_large_text(quote_count),
                |mut root| {
                    transform(black_box(&mut root), black_box(&md));
                    black_box(root);
                },
                BatchSize::SmallInput,
            )
        });
        group.bench_function(format!("document-registry/{quote_count}"), |b| {
            b.iter_batched(
                || Document::from_legacy("", one_large_text(quote_count)),
                |mut document| {
                    transform_registered(black_box(&mut document), black_box(&registry_md));
                    black_box(document);
                },
                BatchSize::SmallInput,
            )
        });
    }
    group.finish();

    let mut small = c.benchmark_group("smartquotes-transform/many-text-nodes");
    for text_count in [17_500, 35_000, 70_000] {
        assert_transformed(many_small_texts(text_count), &md);
        let mut registered = Document::from_legacy("", many_small_texts(text_count));
        transform_registered(&mut registered, &registry_md);
        assert_document_has_no_double_quotes(registered);
        small.throughput(Throughput::Elements(text_count as u64));
        small.bench_function(format!("legacy/{text_count}"), |b| {
            b.iter_batched(
                || many_small_texts(text_count),
                |mut root| {
                    transform(black_box(&mut root), black_box(&md));
                    black_box(root);
                },
                BatchSize::SmallInput,
            )
        });
        small.bench_function(format!("document-registry/{text_count}"), |b| {
            b.iter_batched(
                || Document::from_legacy("", many_small_texts(text_count)),
                |mut document| {
                    transform_registered(black_box(&mut document), black_box(&registry_md));
                    black_box(document);
                },
                BatchSize::SmallInput,
            )
        });
    }
    small.finish();

    let parser = parser();
    for corpus in corpus::standard() {
        let source = corpus.source();
        let mut legacy = parser.parse(source);
        transform(&mut legacy, &parser);
        let legacy_html = legacy.render();
        let mut registered = parser.parse_document(source);
        transform_registered(&mut registered, &registry_md);
        assert_eq!(legacy_html, registered.into_legacy().render(), "{}", corpus.name);

        let mut group = c.benchmark_group(format!("smartquotes-transform/corpus/{}", corpus.name));
        group.throughput(Throughput::Bytes(corpus.len() as u64));
        group.bench_function("legacy", |b| {
            b.iter_batched(
                || parser.parse(source),
                |mut root| {
                    transform(black_box(&mut root), black_box(&parser));
                    black_box(root);
                },
                BatchSize::SmallInput,
            )
        });
        group.bench_function("document-registry", |b| {
            b.iter_batched(
                || parser.parse_document(source),
                |mut document| {
                    transform_registered(black_box(&mut document), black_box(&registry_md));
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
