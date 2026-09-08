use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use markdown_it_benchmarks::{corpus, parser};

fn benchmark(c: &mut Criterion) {
    for configuration in parser::document_parse_configurations() {
        let configuration_name = configuration.name;
        let md = configuration.parser;
        for corpus in corpus::parser_emphasis_checkpoint() {
            let source = corpus.source();
            let expected = md.parse(source).render();
            let bridged = md.parse_document(source);
            let direct = md.parse_document_direct(source).unwrap();
            assert_eq!(md.render_document(&bridged).unwrap(), expected);
            assert_eq!(md.render_document(&direct).unwrap(), expected);
            assert_eq!(direct.len(), bridged.len());

            let mut group = c.benchmark_group(format!(
                "document-parse/{configuration_name}/{}",
                corpus.name
            ));
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
