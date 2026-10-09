use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use markdown_it::{
    Document,
    DocumentNodeRenderer,
    DocumentRenderContext,
    DocumentWriter,
    MarkdownIt,
    NodeDraft,
    NodeRef,
    Text,
};

// Preserve the previous rendering operation as a same-process control.
struct LegacyHtmlTextRenderer;

impl DocumentNodeRenderer<Text> for LegacyHtmlTextRenderer {
    fn render(
        &self,
        _: NodeRef<'_>,
        value: &Text,
        _: &mut DocumentRenderContext<'_>,
        output: &mut DocumentWriter,
    ) {
        output.write_str(&markdown_it::common::utils::escape_html(&value.content));
    }
}

fn benchmark(c: &mut Criterion) {
    let md = MarkdownIt::empty();
    let mut legacy = MarkdownIt::empty();
    legacy.add_document_renderer::<Text, _>("html", LegacyHtmlTextRenderer);
    for (name, pattern) in [
        ("ascii-clean", "ordinary words without HTML punctuation "),
        ("unicode-clean", "中文日本語한글 café e\u{301} 🦀🚀 "),
        ("mixed-escaping", "文字 <tag key=\"value\"> & text </tag> "),
        ("dense-escaping", "<&>\""),
    ] {
        let source = pattern.repeat((64_usize * 1024).div_ceil(pattern.len()));
        let document = Document::from_draft("", NodeDraft::new(Text { content: source }));
        let text = &document
            .node(document.root())
            .cast::<Text>()
            .unwrap()
            .content;
        // Verify the full output independently before timing document rendering.
        let expected = markdown_it::common::utils::escape_html(text);
        assert_eq!(md.render_document(&document), expected);
        assert_eq!(legacy.render_document(&document), expected);
        let mut group = c.benchmark_group(format!("html-escape/{name}"));
        group.throughput(Throughput::Bytes(text.len() as u64));
        group.bench_function("legacy-html", |b| {
            b.iter(|| black_box(legacy.render_document(black_box(&document))))
        });
        group.bench_function("document-html", |b| {
            b.iter(|| black_box(md.render_document(black_box(&document))))
        });
        group.finish();
    }
}

fn benchmark_sizes(c: &mut Criterion) {
    let md = MarkdownIt::empty();
    let mut legacy = MarkdownIt::empty();
    legacy.add_document_renderer::<Text, _>("html", LegacyHtmlTextRenderer);
    for name in [
        "ascii-clean",
        "unicode-clean",
        "quote-last",
        "amp-last",
        "dense-escaping",
    ] {
        let mut group = c.benchmark_group(format!("html-escape-sizes/{name}"));
        for len in [8, 16, 24, 31, 32, 33, 48, 64, 128, 256, 1024, 4096] {
            // Keep each input exactly `len` bytes, including Unicode and markers.
            let source = match name {
                "ascii-clean" => "a".repeat(len),
                "unicode-clean" => format!("{}{}", "雪".repeat(len / 3), "a".repeat(len % 3)),
                "quote-last" => format!("{}\"", "a".repeat(len - 1)),
                "amp-last" => format!("{}&", "a".repeat(len - 1)),
                "dense-escaping" => "<&>\"".repeat(len.div_ceil(4))[..len].to_owned(),
                _ => unreachable!(),
            };
            assert_eq!(source.len(), len);
            let expected = markdown_it::common::utils::escape_html(&source).into_owned();
            let document = Document::from_draft("", NodeDraft::new(Text { content: source }));
            assert_eq!(md.render_document(&document), expected);
            assert_eq!(legacy.render_document(&document), expected);
            group.throughput(Throughput::Bytes(len as u64));
            group.bench_with_input(
                BenchmarkId::new("legacy-html", len),
                &document,
                |b, document| b.iter(|| black_box(legacy.render_document(black_box(document)))),
            );
            group.bench_with_input(
                BenchmarkId::new("document-html", len),
                &document,
                |b, document| b.iter(|| black_box(md.render_document(black_box(document)))),
            );
        }
        group.finish();
    }
}

criterion_group!(benches, benchmark, benchmark_sizes);
criterion_main!(benches);
