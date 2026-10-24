use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use markdown_it::parser::core::{CoreRule, CoreState};
use markdown_it::plugins::cmark::block::paragraph::Paragraph;
use markdown_it::plugins::cmark::inline::newline::{Hardbreak, Softbreak};
use markdown_it::plugins::html::html_inline::HtmlInline;
use markdown_it::{
    Document,
    EditBatch,
    MarkdownIt,
    NodeDraft,
    NodeRef,
    StructuralEvent,
    Text,
    TextBoundary,
    TextEvent,
    TextProjectionKind,
};
use markdown_it_benchmarks::corpus;

macro_rules! empty_core_rule {
    ($type:ident) => {
        #[derive(Default)]
        struct $type;

        impl CoreRule for $type {
            fn run(&self, state: &mut CoreState<'_>) {
                EditBatch::new().commit(state.document);
            }
        }
    };
}

empty_core_rule!(EmptyCoreRule1);
empty_core_rule!(EmptyCoreRule2);
empty_core_rule!(EmptyCoreRule3);
empty_core_rule!(EmptyCoreRule4);
empty_core_rule!(EmptyCoreRule5);

fn text_projection<'a>(document: &'a Document, node: NodeRef<'a>) -> TextProjectionKind<'a> {
    if let Some(text) = document.text(node.id()) {
        TextProjectionKind::Writable(text)
    } else if let Some(html) = node.cast::<HtmlInline>() {
        TextProjectionKind::ReadOnly(&html.content)
    } else if node.is::<Paragraph>() || node.is::<Hardbreak>() || node.is::<Softbreak>() {
        TextProjectionKind::Boundary(TextBoundary::Space)
    } else {
        TextProjectionKind::Transparent
    }
}

fn one_edit_per_text_node(document: &markdown_it::Document) -> EditBatch {
    let mut batch = EditBatch::new();
    let mut previous = None;
    for event in document.text_events(text_projection) {
        match event {
            TextEvent::Char {
                node,
                byte_offset,
                ch,
                writable: true,
                ..
            } if previous != Some(node) => {
                batch.replace_char(node, byte_offset..byte_offset + ch.len_utf8(), ch);
                previous = Some(node);
            }
            _ => {}
        }
    }
    batch
}

fn one_attribute_per_node(document: &markdown_it::Document) -> EditBatch {
    let mut batch = EditBatch::new();
    for event in document.events(document.root()) {
        match event {
            StructuralEvent::Enter(node) | StructuralEvent::Leaf(node) => {
                batch.set_attribute(node.id(), "data-benchmark", "edited");
            }
            StructuralEvent::Exit(_) => {}
        }
    }
    batch
}

fn remove_top_level_subtrees(document: &markdown_it::Document) -> EditBatch {
    let mut batch = EditBatch::new();
    for &child in document.children(document.root()) {
        batch.remove_node(child);
    }
    batch
}

fn insert_before_top_level_nodes(document: &markdown_it::Document) -> EditBatch {
    let mut batch = EditBatch::new();
    for &child in document.children(document.root()) {
        batch.insert_before(
            child,
            NodeDraft::new(Text {
                content: "generated".to_owned(),
            }),
        );
    }
    batch
}

fn replace_top_level_subtrees(document: &markdown_it::Document) -> EditBatch {
    let mut batch = EditBatch::new();
    for &child in document.children(document.root()) {
        batch.replace_node(
            child,
            NodeDraft::new(Text {
                content: "generated".to_owned(),
            }),
        );
    }
    batch
}

fn wrap_top_level_range(document: &markdown_it::Document) -> EditBatch {
    let mut batch = EditBatch::new();
    let children = document.children(document.root());
    if let (Some(&first), Some(&last)) = (children.first(), children.last()) {
        batch.wrap_range(first, last, NodeDraft::new(Paragraph));
    }
    batch
}

fn parser() -> markdown_it::MarkdownIt {
    let mut md = markdown_it::MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::html::add(&mut md);
    md
}

fn benchmark(c: &mut Criterion) {
    let md = parser();

    let zero_rules = MarkdownIt::empty();
    let mut one_rule = MarkdownIt::empty();
    one_rule.add_rule::<EmptyCoreRule1>();
    let mut five_rules = MarkdownIt::empty();
    five_rules.add_rule::<EmptyCoreRule1>();
    five_rules.add_rule::<EmptyCoreRule2>();
    five_rules.add_rule::<EmptyCoreRule3>();
    five_rules.add_rule::<EmptyCoreRule4>();
    five_rules.add_rule::<EmptyCoreRule5>();
    zero_rules.parse("");
    one_rule.parse("");
    five_rules.parse("");

    let mut group = c.benchmark_group("core-chain/empty-input");
    group.bench_function("zero-custom-rules", |b| {
        b.iter(|| zero_rules.parse(black_box("")))
    });
    group.bench_function("one-empty-rule", |b| {
        b.iter(|| one_rule.parse(black_box("")))
    });
    group.bench_function("five-empty-rules", |b| {
        b.iter(|| five_rules.parse(black_box("")))
    });
    group.finish();

    for corpus in corpus::standard() {
        let source = corpus.source();
        let mut group = c.benchmark_group(format!("document-build/{}", corpus.name));
        group.throughput(Throughput::Bytes(corpus.len() as u64));
        group.bench_function("arena-document", |b| {
            b.iter(|| black_box(md.parse(black_box(source))))
        });
        group.finish();
        let document = md.parse(source);
        let mut group = c.benchmark_group(format!("document-events/{}", corpus.name));
        group.throughput(Throughput::Elements(document.len() as u64));
        group.bench_function("arena-structural-events", |b| {
            b.iter(|| {
                for event in document.events(document.root()) {
                    black_box(event.node().name());
                }
            })
        });
        group.finish();

        let projection = text_projection;
        let event_count = document.text_events(projection).count();
        let mut group = c.benchmark_group(format!("document-text-events/{}", corpus.name));
        group.throughput(Throughput::Elements(event_count as u64));
        group.bench_function("arena-text-projection", |b| {
            b.iter(|| {
                for event in document.text_events(projection) {
                    black_box(event);
                }
            })
        });
        group.finish();

        let edit_count = one_edit_per_text_node(&document).len();
        let mut group = c.benchmark_group(format!("document-text-commit/{}", corpus.name));
        group.throughput(Throughput::Elements(edit_count as u64));
        group.bench_function("validate-and-commit", |b| {
            b.iter_batched(
                || {
                    let document = md.parse(source);
                    let batch = one_edit_per_text_node(&document);
                    (document, batch)
                },
                |(mut document, batch)| {
                    batch.commit(&mut document);
                    black_box(document)
                },
                BatchSize::SmallInput,
            )
        });
        group.finish();

        let attribute_count = document.len();
        let mut edited = md.parse(source);
        one_attribute_per_node(&edited).commit(&mut edited);
        for event in edited.events(edited.root()) {
            if let StructuralEvent::Enter(node) | StructuralEvent::Leaf(node) = event {
                assert!(node
                    .attrs()
                    .iter()
                    .any(|(name, value)| name == "data-benchmark" && value == "edited"));
            }
        }
        let mut group = c.benchmark_group(format!("document-attribute-commit/{}", corpus.name));
        group.throughput(Throughput::Elements(attribute_count as u64));
        group.bench_function("validate-and-commit", |b| {
            b.iter_batched(
                || {
                    let document = md.parse(source);
                    let batch = one_attribute_per_node(&document);
                    (document, batch)
                },
                |(mut document, batch)| {
                    batch.commit(&mut document);
                    black_box(document)
                },
                BatchSize::SmallInput,
            )
        });
        group.finish();

        let removed_count = document.len() - 1;
        let mut edited = md.parse(source);
        remove_top_level_subtrees(&edited).commit(&mut edited);
        assert_eq!(edited.len(), 1);
        assert!(edited.children(edited.root()).is_empty());
        let mut group = c.benchmark_group(format!("document-subtree-remove/{}", corpus.name));
        group.throughput(Throughput::Elements(removed_count as u64));
        group.bench_function("validate-and-commit", |b| {
            b.iter_batched(
                || {
                    let document = md.parse(source);
                    let batch = remove_top_level_subtrees(&document);
                    (document, batch)
                },
                |(mut document, batch)| {
                    batch.commit(&mut document);
                    black_box(document)
                },
                BatchSize::SmallInput,
            )
        });
        group.finish();

        let inserted_count = document.children(document.root()).len();
        let mut edited = md.parse(source);
        let original_children = edited.children(edited.root()).to_vec();
        insert_before_top_level_nodes(&edited).commit(&mut edited);
        let edited_children = edited.children(edited.root());
        assert_eq!(edited_children.len(), original_children.len() * 2);
        let (pairs, remainder) = edited_children.as_chunks::<2>();
        assert!(remainder.is_empty());
        for (pair, original) in pairs.iter().zip(original_children) {
            assert_eq!(pair[1], original);
            assert_eq!(
                edited.node(pair[0]).cast::<Text>().unwrap().content,
                "generated"
            );
        }
        let mut group = c.benchmark_group(format!("document-sibling-insert/{}", corpus.name));
        group.throughput(Throughput::Elements(inserted_count as u64));
        group.bench_function("validate-and-commit", |b| {
            b.iter_batched(
                || {
                    let document = md.parse(source);
                    let batch = insert_before_top_level_nodes(&document);
                    (document, batch)
                },
                |(mut document, batch)| {
                    batch.commit(&mut document);
                    black_box(document)
                },
                BatchSize::SmallInput,
            )
        });
        group.finish();

        let replaced_node_count = document.len() - 1;
        let mut edited = md.parse(source);
        let old_roots = edited.children(edited.root()).to_vec();
        replace_top_level_subtrees(&edited).commit(&mut edited);
        assert_eq!(edited.len(), old_roots.len() + 1);
        assert!(old_roots
            .iter()
            .all(|&node| edited.get_node(node).is_none()));
        for &node in edited.children(edited.root()) {
            assert_eq!(
                edited.node(node).cast::<Text>().unwrap().content,
                "generated"
            );
        }
        let mut group = c.benchmark_group(format!("document-subtree-replace/{}", corpus.name));
        group.throughput(Throughput::Elements(replaced_node_count as u64));
        group.bench_function("validate-and-commit", |b| {
            b.iter_batched(
                || {
                    let document = md.parse(source);
                    let batch = replace_top_level_subtrees(&document);
                    (document, batch)
                },
                |(mut document, batch)| {
                    batch.commit(&mut document);
                    black_box(document)
                },
                BatchSize::SmallInput,
            )
        });
        group.finish();

        let wrapped_count = document.children(document.root()).len();
        let mut edited = md.parse(source);
        let original_children = edited.children(edited.root()).to_vec();
        wrap_top_level_range(&edited).commit(&mut edited);
        assert_eq!(edited.len(), document.len() + 1);
        let wrapper = edited.children(edited.root())[0];
        assert_eq!(edited.children(wrapper), original_children);
        assert!(edited.children(wrapper).iter().all(|&node| {
            edited.get_node(node).is_some() && edited.parent(node) == Some(wrapper)
        }));
        let mut group = c.benchmark_group(format!("document-sibling-wrap/{}", corpus.name));
        group.throughput(Throughput::Elements(wrapped_count as u64));
        group.bench_function("validate-and-commit", |b| {
            b.iter_batched(
                || {
                    let document = md.parse(source);
                    let batch = wrap_top_level_range(&document);
                    (document, batch)
                },
                |(mut document, batch)| {
                    batch.commit(&mut document);
                    black_box(document)
                },
                BatchSize::SmallInput,
            )
        });
        group.finish();
    }
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
