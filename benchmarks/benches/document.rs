use std::hint::black_box;

use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use markdown_it_benchmarks::corpus;
use markdown_it::parser::inline::Text;
use markdown_it::plugins::cmark::block::paragraph::Paragraph;
use markdown_it::plugins::cmark::inline::newline::{Hardbreak, Softbreak};
use markdown_it::plugins::html::html_inline::HtmlInline;
use markdown_it::{NodeRef, TextBoundary, TextProjection, TextProjectionKind};

fn consume_legacy_events(node: &markdown_it::Node) {
    if node.children.is_empty() {
        black_box(node.name());
        return;
    }

    black_box(node.name());
    for child in &node.children {
        consume_legacy_events(child);
    }
    black_box(node.name());
}

fn text_projection(node: NodeRef<'_>) -> TextProjectionKind<'_> {
    if let Some(text) = node.cast::<Text>() {
        TextProjectionKind::Writable(&text.content)
    } else if let Some(html) = node.cast::<HtmlInline>() {
        TextProjectionKind::ReadOnly(&html.content)
    } else if node.is::<Paragraph>() || node.is::<Hardbreak>() || node.is::<Softbreak>() {
        TextProjectionKind::Boundary(TextBoundary::Space)
    } else {
        TextProjectionKind::Transparent
    }
}

fn consume_legacy_text_events(node: &markdown_it::Node, nesting_level: u32) {
    black_box(("enter", node.name(), nesting_level));
    if let Some(text) = node.cast::<Text>() {
        for (byte_offset, ch) in text.content.char_indices() {
            black_box((byte_offset, ch, true, nesting_level));
        }
    } else if let Some(html) = node.cast::<HtmlInline>() {
        for (byte_offset, ch) in html.content.char_indices() {
            black_box((byte_offset, ch, false, nesting_level));
        }
    } else if node.is::<Paragraph>() || node.is::<Hardbreak>() || node.is::<Softbreak>() {
        black_box(TextBoundary::Space);
    }
    for child in &node.children {
        consume_legacy_text_events(child, nesting_level + 1);
    }
    black_box(("exit", node.name(), nesting_level));
}

fn parser() -> markdown_it::MarkdownIt {
    let mut md = markdown_it::MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::html::add(&mut md);
    md
}

fn benchmark(c: &mut Criterion) {
    let md = parser();

    for corpus in corpus::standard() {
        let source = corpus.source();
        let legacy_html = md.parse(source).render();
        let document_html = md.parse_document(source).into_legacy().render();
        assert_eq!(legacy_html, document_html, "{} roundtrip", corpus.name);

        let mut group = c.benchmark_group(format!("document-build/{}", corpus.name));
        group.throughput(Throughput::Bytes(corpus.len() as u64));
        group.bench_function("legacy-tree", |b| {
            b.iter(|| black_box(md.parse(black_box(source))))
        });
        group.bench_function("arena-document", |b| {
            b.iter(|| black_box(md.parse_document(black_box(source))))
        });
        group.bench_function("legacy-to-arena", |b| {
            b.iter_batched(
                || md.parse(source),
                |root| black_box(markdown_it::Document::from_legacy(black_box(source), root)),
                BatchSize::SmallInput,
            )
        });
        group.finish();

        let legacy = md.parse(source);
        let document = md.parse_document(source);
        let mut group = c.benchmark_group(format!("document-events/{}", corpus.name));
        group.throughput(Throughput::Elements(document.len() as u64));
        group.bench_function("legacy-structural-dfs", |b| {
            b.iter(|| consume_legacy_events(black_box(&legacy)))
        });
        group.bench_function("arena-structural-events", |b| {
            b.iter(|| {
                for event in document.events(document.root()).unwrap() {
                    black_box(event.node().name());
                }
            })
        });
        group.finish();

        let projection = TextProjection::new(text_projection);
        let event_count = document.text_events(projection).count();
        let mut group = c.benchmark_group(format!("document-text-events/{}", corpus.name));
        group.throughput(Throughput::Elements(event_count as u64));
        group.bench_function("legacy-text-projection", |b| {
            b.iter(|| consume_legacy_text_events(black_box(&legacy), 0))
        });
        group.bench_function("arena-text-projection", |b| {
            b.iter(|| {
                for event in document.text_events(projection) {
                    black_box(event);
                }
            })
        });
        group.finish();
    }
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
