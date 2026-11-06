use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use markdown_it::{plugins, MarkdownIt};

fn benchmark(c: &mut Criterion) {
    let mut md = MarkdownIt::empty();
    plugins::cmark::add(&mut md);
    plugins::html::add(&mut md);

    let openers = "[a".repeat(4_096);
    let mut group = c.benchmark_group("scan-hooks");
    for (name, suffix) in [
        ("no-html", ""),
        ("ordinary-html", " <span>x</span>"),
        ("special-html", " <?ok?>"),
        ("disabled-special-html", " <?ok?> (<?unclosed"),
    ] {
        let source = format!("{openers}{suffix}");
        // Keep lazy initialization outside the measured parse-and-drop loop.
        md.render(&source);
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_function(name, |b| b.iter(|| black_box(md.parse(black_box(&source)))));
    }
    group.finish();
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
