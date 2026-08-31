use std::hint::black_box;

use comrak::{format_html, parse_document, Arena, Options};
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use markdown_it_benchmarks::corpus::{self, CorpusKind};
use pulldown_cmark::{html, Parser as PulldownParser};

const MARKDOWN_IT_RS: &str = "markdown-it-rs";
const MARKDOWN_IT_V6: &str = "markdown-it-0.6";
const COMRAK: &str = "comrak-0.52";
const PULLDOWN_CMARK: &str = "pulldown-cmark-0.13";
const MARKDOWN_RS: &str = "markdown-rs-1.0";

fn markdown_it_rs() -> markdown_it::MarkdownIt {
    let mut md = markdown_it::MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::html::add(&mut md);
    md
}

fn markdown_it_v6() -> markdown_it_v6::MarkdownIt {
    let mut md = markdown_it_v6::MarkdownIt::new();
    markdown_it_v6::plugins::cmark::add(&mut md);
    markdown_it_v6::plugins::html::add(&mut md);
    md
}

fn comrak_options() -> Options<'static> {
    let mut options = Options::default();
    // Match the markdown-it configurations, which preserve raw HTML.
    options.render.r#unsafe = true;
    options
}

fn render_comrak(source: &str, options: &Options<'_>) -> String {
    let arena = Arena::new();
    let root = parse_document(&arena, source, options);
    let mut output = String::new();
    format_html(root, options, &mut output).unwrap();
    output
}

fn markdown_rs_options() -> markdown::Options {
    markdown::Options {
        compile: markdown::CompileOptions {
            // Match the other engines, which preserve raw HTML.
            allow_dangerous_html: true,
            ..markdown::CompileOptions::default()
        },
        ..markdown::Options::default()
    }
}

fn render_pulldown_cmark(source: &str) -> String {
    let mut output = String::new();
    html::push_html(&mut output, PulldownParser::new(source));
    output
}

fn assert_stable_output(corpus_name: &str, engine_name: &str, mut render: impl FnMut() -> String) {
    let expected = render();
    let repeated = render();

    assert!(
        !expected.is_empty(),
        "{engine_name} rendered an empty {corpus_name}"
    );
    assert_eq!(
        expected, repeated,
        "{engine_name} produced unstable output for {corpus_name}"
    );
}

pub fn benchmark(c: &mut Criterion) {
    let current = markdown_it_rs();
    let legacy = markdown_it_v6();
    let comrak_options = comrak_options();
    let markdown_rs_options = markdown_rs_options();

    for corpus in corpus::standard() {
        let source = corpus.source();
        let compare_engines = corpus.kind != CorpusKind::PathologicalSmoke;

        // Correctness checks intentionally happen before Criterion starts
        // timing. The engines are checked independently because harmless HTML
        // formatting differences make byte-for-byte cross-engine comparison
        // misleading.
        assert_stable_output(corpus.name, MARKDOWN_IT_RS, || {
            current.parse(source).render()
        });
        if compare_engines {
            assert_stable_output(corpus.name, MARKDOWN_IT_V6, || {
                legacy.parse(source).render()
            });
            assert_stable_output(corpus.name, COMRAK, || {
                render_comrak(source, &comrak_options)
            });
            assert_stable_output(corpus.name, PULLDOWN_CMARK, || {
                render_pulldown_cmark(source)
            });
            assert_stable_output(corpus.name, MARKDOWN_RS, || {
                markdown::to_html_with_options(source, &markdown_rs_options).unwrap()
            });
        }

        let mut parse = c.benchmark_group(format!("parse/{}", corpus.name));
        parse.throughput(Throughput::Bytes(corpus.len() as u64));
        parse.bench_function(MARKDOWN_IT_RS, |b| {
            b.iter(|| black_box(current.parse(black_box(source))))
        });
        if compare_engines {
            parse.bench_function(MARKDOWN_IT_V6, |b| {
                b.iter(|| black_box(legacy.parse(black_box(source))))
            });
            parse.bench_function(COMRAK, |b| {
                b.iter(|| {
                    let arena = Arena::new();
                    let root = parse_document(&arena, black_box(source), &comrak_options);
                    black_box(root);
                })
            });
            parse.bench_function(PULLDOWN_CMARK, |b| {
                b.iter(|| black_box(PulldownParser::new(black_box(source)).collect::<Vec<_>>()))
            });
            parse.bench_function(MARKDOWN_RS, |b| {
                b.iter(|| {
                    black_box(
                        markdown::to_mdast(black_box(source), &markdown_rs_options.parse).unwrap(),
                    )
                })
            });
        }
        parse.finish();

        let current_ast = current.parse(source);
        let legacy_ast = compare_engines.then(|| legacy.parse(source));
        let comrak_arena = Arena::new();
        let comrak_ast =
            compare_engines.then(|| parse_document(&comrak_arena, source, &comrak_options));
        let pulldown_events =
            compare_engines.then(|| PulldownParser::new(source).collect::<Vec<_>>());

        let mut render = c.benchmark_group(format!("render/{}", corpus.name));
        render.throughput(Throughput::Bytes(corpus.len() as u64));
        render.bench_function(MARKDOWN_IT_RS, |b| {
            b.iter(|| black_box(current_ast.render()))
        });
        if let (Some(legacy_ast), Some(comrak_ast)) = (legacy_ast.as_ref(), comrak_ast) {
            render.bench_function(MARKDOWN_IT_V6, |b| {
                b.iter(|| black_box(legacy_ast.render()))
            });
            render.bench_function(COMRAK, |b| {
                b.iter(|| {
                    let mut output = String::new();
                    format_html(comrak_ast, &comrak_options, &mut output).unwrap();
                    black_box(output);
                })
            });
            render.bench_function(PULLDOWN_CMARK, |b| {
                let events = pulldown_events.as_ref().unwrap();
                b.iter(|| {
                    let mut output = String::new();
                    // pulldown-cmark's renderer consumes an event iterator;
                    // replaying a saved event stream requires cheap event clones.
                    html::push_html(&mut output, events.iter().cloned());
                    black_box(output);
                })
            });
        }
        render.finish();

        let mut end_to_end = c.benchmark_group(format!("parse-render/{}", corpus.name));
        end_to_end.throughput(Throughput::Bytes(corpus.len() as u64));
        end_to_end.bench_function(MARKDOWN_IT_RS, |b| {
            b.iter(|| black_box(current.parse(black_box(source)).render()))
        });
        if compare_engines {
            end_to_end.bench_function(MARKDOWN_IT_V6, |b| {
                b.iter(|| black_box(legacy.parse(black_box(source)).render()))
            });
            end_to_end.bench_function(COMRAK, |b| {
                b.iter(|| {
                    let arena = Arena::new();
                    let root = parse_document(&arena, black_box(source), &comrak_options);
                    let mut output = String::new();
                    format_html(root, &comrak_options, &mut output).unwrap();
                    black_box(output);
                })
            });
            end_to_end.bench_function(PULLDOWN_CMARK, |b| {
                b.iter(|| {
                    let mut output = String::new();
                    html::push_html(&mut output, PulldownParser::new(black_box(source)));
                    black_box(output);
                })
            });
            end_to_end.bench_function(MARKDOWN_RS, |b| {
                b.iter(|| {
                    black_box(
                        markdown::to_html_with_options(black_box(source), &markdown_rs_options)
                            .unwrap(),
                    )
                })
            });
        }
        end_to_end.finish();
    }
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
