//! Equivalent leaf and container syntax through low-level and high-level APIs.
use std::hint::black_box;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use markdown_it::document::{Node, NodeId};
use markdown_it::parser::inline::{InlineRule, InlineState};
use markdown_it::plugin::{Html, InlinePluginState, PluginSpec};
use markdown_it::render::{NodeRenderer, RenderContext, Writer};
use markdown_it::MarkdownIt;

#[derive(Debug)]
struct Emoji;

impl Emoji {
    fn parse(cx: &mut InlinePluginState<'_>) -> Option<Self> {
        cx.consume(":rocket:").then_some(Self)
    }

    fn render_html(&self, html: &mut Html) {
        html.element("span")
            .class("emoji")
            .attr("role", "img")
            .attr("aria-label", "rocket")
            .text("🚀");
    }
}

struct LowRule;
impl InlineRule for LowRule {
    const MARKER: char = ':';

    fn check(state: &mut InlineState<'_>) -> Option<usize> {
        state.remaining().starts_with(":rocket:").then_some(8)
    }

    fn run(state: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        let len = Self::check(state)?;
        let (_, document) = state.remaining_and_document();
        Some((Some(document.create_node(Emoji)), len))
    }
}

struct LowRenderer;
impl NodeRenderer<Emoji> for LowRenderer {
    fn render(&self, _: &Node, _: &Emoji, _: &mut RenderContext<'_>, out: &mut Writer) {
        // Same tag/attribute/text escaping work as the Html builder, without
        // allocating a temporary tree. Avoid a precomputed markup shortcut.
        out.write_str("<");
        out.write_str("span");
        for (name, value) in [
            ("class", "emoji"),
            ("role", "img"),
            ("aria-label", "rocket"),
        ] {
            out.write_str(" ");
            out.write_str(name);
            out.write_str("=\"");
            out.write_str(&markdown_it::common::utils::escape_html(value));
            out.write_str("\"");
        }
        out.write_str(">");
        out.write_str(&markdown_it::common::utils::escape_html("🚀"));
        out.write_str("</");
        out.write_str("span");
        out.write_str(">");
    }
}

fn parser(kind: &str) -> MarkdownIt {
    match kind {
        "low" => {
            let mut md = MarkdownIt::new();
            md.inline
                .add_rule::<LowRule>()
                .after_named("text")
                .before_all();
            md.add_renderer::<Emoji, _>("html", LowRenderer);
            md
        }
        "dynamic" | "wildcard" => MarkdownIt::builder()
            .plugin(
                PluginSpec::new("emoji")
                    .inline_leaf::<Emoji>("emoji")
                    .marker(if kind == "wildcard" { '\0' } else { ':' })
                    .parse(Emoji::parse)
                    .render_html(Emoji::render_html)
                    .finish(),
            )
            .build(),
        _ => unreachable!(),
    }
}

fn benchmark(c: &mut Criterion) {
    let kinds = ["low", "dynamic", "wildcard"];
    let corpora = [
        ("short", "Ready :rocket:!".to_owned()),
        (
            "sparse",
            format!(
                "{} :rocket:\n\n",
                "Ordinary prose without markup. ".repeat(32)
            )
            .repeat(64),
        ),
        ("dense", "Ready :rocket: :rocket: :rocket:!\n\n".repeat(512)),
        (
            "miss",
            "Some prose :unknown: and :missing: markers.\n\n".repeat(512),
        ),
    ];
    for (name, source) in &corpora {
        let expected = parser("low").render(source);
        for kind in kinds {
            let md = parser(kind);
            // Warm every parser, including the lazy low-level rule tables.
            assert_eq!(md.render(source), expected, "{name}/{kind}");
        }
        for phase in ["parse", "render", "total"] {
            let mut group = c.benchmark_group(format!("plugin_api/{name}/{phase}"));
            for kind in kinds {
                let md = parser(kind);
                let document = md.parse(source);
                group.bench_function(kind, |b| match phase {
                    "parse" => b.iter(|| {
                        black_box(md.parse(black_box(source)));
                    }),
                    "render" => b.iter(|| {
                        black_box(md.render_document(black_box(&document), "html"));
                    }),
                    "total" => b.iter(|| {
                        black_box(md.render(black_box(source)));
                    }),
                    _ => unreachable!(),
                });
            }
            group.finish();
        }
    }
    let mut group = c.benchmark_group("plugin_api/build");
    for kind in kinds {
        group.bench_function(kind, |b| b.iter(|| black_box(parser(kind))));
    }
    group.finish();

    // The builder eagerly prepares rule tables; low-level construction is
    // lazy. Include first use to compare the complete initialization cost.
    let mut group = c.benchmark_group("plugin_api/build-first-render");
    for kind in kinds {
        group.bench_function(kind, |b| {
            b.iter(|| black_box(parser(kind).render(black_box("Ready :rocket:!"))))
        });
    }
    group.finish();
}

#[derive(Debug)]
struct Span;

struct ContainerRule;
impl InlineRule for ContainerRule {
    const MARKER: char = '{';

    fn check(state: &mut InlineState<'_>) -> Option<usize> {
        let source = state.remaining();
        if !source.starts_with('{') {
            return None;
        }
        Some(source.find('}')? + 1)
    }

    fn run(state: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        let consumed = Self::check(state)?;
        let children = state.parse_subrange(1..consumed - 1)?;
        let node = state.document.create_node(Span);
        for child in children {
            state.document.push_child(node, child);
        }
        Some((Some(node), consumed))
    }
}

struct SpanRenderer;
impl NodeRenderer<Span> for SpanRenderer {
    fn render(&self, node: &Node, _: &Span, cx: &mut RenderContext<'_>, out: &mut Writer) {
        out.write_str("<span>");
        cx.render_children(node.id(), out);
        out.write_str("</span>");
    }
}

fn container_parser(kind: &str) -> MarkdownIt {
    if kind == "low" {
        let mut md = MarkdownIt::new();
        md.inline
            .add_rule::<ContainerRule>()
            .after_named("text")
            .before_all();
        md.add_renderer::<Span, _>("html", SpanRenderer);
        return md;
    }
    let builder = PluginSpec::new("span")
        .inline_container::<Span>("span")
        .marker('{')
        .parse(|cx| {
            let source = cx.source();
            if !source.starts_with('{') {
                return None;
            }
            let end = source.find('}')?;
            cx.advance(end + 1);
            Some((Span, 1..end))
        });
    let plugin = match kind {
        "tag" => builder.html_tag("span").finish(),
        "callback" => builder
            .render_html(|_, html| {
                let children = html.render_children();
                html.element("span").raw(children);
            })
            .finish(),
        "render_with" => builder.render_with("html", SpanRenderer).finish(),
        _ => unreachable!(),
    };
    MarkdownIt::builder().plugin(plugin).build()
}

fn benchmark_containers(c: &mut Criterion) {
    let kinds = ["low", "tag", "callback", "render_with"];
    let corpora = [
        ("short", "前{*雪* `x`}后".to_owned()),
        ("dense", "前{*雪* `x`}后 {plain} {}\n\n".repeat(256)),
        (
            "sparse",
            format!("{} {{*雪*}}\n\n", "Ordinary prose. ".repeat(32)).repeat(64),
        ),
        ("miss", "Ordinary {unterminated *雪* `x`\n\n".repeat(256)),
    ];
    for (name, source) in &corpora {
        let expected = container_parser("low").render(source);
        for kind in kinds {
            let md = container_parser(kind);
            assert_eq!(md.render(source), expected, "container/{name}/{kind}");
        }
        for phase in ["parse", "render", "total"] {
            let mut group = c.benchmark_group(format!("container_api/{name}/{phase}"));
            for kind in kinds {
                let md = container_parser(kind);
                let document = md.parse(source);
                // Initialize lazy rule tables before timing, for both API levels.
                assert_eq!(md.render_document(&document, "html"), expected);
                group.bench_function(kind, |b| match phase {
                    "parse" => b.iter(|| {
                        black_box(md.parse(black_box(source)));
                    }),
                    "render" => b.iter(|| {
                        black_box(md.render_document(black_box(&document), "html"));
                    }),
                    "total" => b.iter(|| {
                        black_box(md.render(black_box(source)));
                    }),
                    _ => unreachable!(),
                });
            }
            group.finish();
        }
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(40)
        .warm_up_time(Duration::from_millis(300))
        .measurement_time(Duration::from_secs(1));
    targets = benchmark, benchmark_containers
}
criterion_main!(benches);
