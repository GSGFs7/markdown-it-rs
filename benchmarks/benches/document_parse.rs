use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use markdown_it::MarkdownIt;
use markdown_it_benchmarks::corpus;

fn benchmark(c: &mut Criterion) {
    let empty = MarkdownIt::empty();
    let mut paragraph = MarkdownIt::empty();
    markdown_it::plugins::cmark::block::paragraph::add(&mut paragraph);

    for (configuration, md) in [("text-fallback", empty), ("paragraph-text", paragraph)] {
        for corpus in corpus::standard() {
            let source = corpus.source();
            let expected = md.parse(source).render();
            let bridged = md.parse_document(source);
            let direct = md.parse_document_direct(source).unwrap();
            assert_eq!(md.render_document(&bridged).unwrap(), expected);
            assert_eq!(md.render_document(&direct).unwrap(), expected);

            let mut group =
                c.benchmark_group(format!("document-parse/{configuration}/{}", corpus.name));
            group.throughput(Throughput::Bytes(corpus.len() as u64));
            group.bench_function("legacy-tree", |b| {
                b.iter(|| black_box(md.parse(black_box(source))))
            });
            group.bench_function("legacy-arena-bridge", |b| {
                b.iter(|| black_box(md.parse_document(black_box(source))))
            });
            group.bench_function("arena-direct", |b| {
                b.iter(|| black_box(md.parse_document_direct(black_box(source)).unwrap()))
            });
            group.finish();
        }
    }
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
