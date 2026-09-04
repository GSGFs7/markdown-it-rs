use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use markdown_it::{Document, HtmlAttribute, MarkdownIt};

fn parser() -> MarkdownIt {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut md);
    md
}

fn document_with_attrs(md: &MarkdownIt, attrs: &[HtmlAttribute]) -> Document {
    let mut legacy = md.parse("text");
    legacy.children[0].attrs = attrs.to_vec();
    let expected = legacy.render();
    let document = Document::from_legacy("text", legacy);
    assert_eq!(md.render_document(&document).unwrap(), expected);
    document
}

fn benchmark(c: &mut Criterion) {
    let md = parser();
    let cases = [
        ("empty", Vec::new()),
        ("single", vec![("id".into(), "value".into())]),
        (
            "repeated",
            vec![
                ("class".into(), "first".into()),
                ("id".into(), "one".into()),
                ("class".into(), "second".into()),
                ("style".into(), "color:red".into()),
                ("title".into(), "<&>".into()),
                ("style".into(), "display:block".into()),
                ("id".into(), "two".into()),
            ],
        ),
        (
            "large",
            (0..32)
                .flat_map(|index| {
                    [
                        (format!("data-{index}"), format!("value-{index}")),
                        ("class".into(), format!("class-{index}")),
                    ]
                })
                .collect(),
        ),
    ];

    for (name, attrs) in cases {
        let document = document_with_attrs(&md, &attrs);
        let mut group = c.benchmark_group(format!("document-attrs/{name}"));
        group.throughput(Throughput::Elements(attrs.len() as u64));
        group.bench_function("arena-direct", |b| {
            b.iter(|| black_box(md.render_document(black_box(&document)).unwrap()))
        });
        group.finish();
    }
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
