use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use markdown_it::MarkdownIt;
use markdown_it_benchmarks::corpus;

fn parser() -> MarkdownIt {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::html::add(&mut md);
    md
}

fn benchmark(c: &mut Criterion) {
    let md = parser();

    for corpus in corpus::standard() {
        let source = corpus.source();
        let legacy = md.parse(source);
        let document = md.parse_document(source);

        let legacy_html = legacy.render();
        let direct_html = md.render_document(&document).unwrap();
        let bridge_html = md.parse_document(source).into_legacy().render();
        assert_eq!(direct_html, legacy_html, "{} direct render", corpus.name);
        assert_eq!(bridge_html, legacy_html, "{} bridge render", corpus.name);

        let mut render = c.benchmark_group(format!("document-render/{}", corpus.name));
        render.throughput(Throughput::Bytes(corpus.len() as u64));
        render.bench_function("arena-direct", |b| {
            b.iter(|| black_box(md.render_document(black_box(&document)).unwrap()))
        });
        render.bench_function("arena-bridge", |b| {
            b.iter_batched(
                || md.parse_document(source),
                |document| black_box(document.into_legacy().render()),
                BatchSize::SmallInput,
            )
        });
        render.bench_function("legacy-tree", |b| {
            b.iter(|| black_box(legacy.render()))
        });
        render.finish();

        let mut end_to_end =
            c.benchmark_group(format!("document-end-to-end/{}", corpus.name));
        end_to_end.throughput(Throughput::Bytes(corpus.len() as u64));
        end_to_end.bench_function("arena-direct", |b| {
            b.iter(|| {
                let document = md.parse_document(black_box(source));
                black_box(md.render_document(&document).unwrap())
            })
        });
        end_to_end.bench_function("arena-bridge", |b| {
            b.iter(|| {
                black_box(
                    md.parse_document(black_box(source))
                        .into_legacy()
                        .render(),
                )
            })
        });
        end_to_end.bench_function("legacy-tree", |b| {
            b.iter(|| black_box(md.parse(black_box(source)).render()))
        });
        end_to_end.finish();
    }
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
