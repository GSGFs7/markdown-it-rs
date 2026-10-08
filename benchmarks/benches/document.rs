use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use markdown_it::parser::inline::Text;
use markdown_it::plugins::cmark::block::paragraph::Paragraph;
use markdown_it::plugins::cmark::inline::newline::{Hardbreak, Softbreak};
use markdown_it::plugins::html::html_inline::HtmlInline;
use markdown_it::{
    Document,
    DocumentTransform,
    DocumentTransformRegistry,
    EditBatch,
    MarkdownIt,
    NodeDraft,
    NodeRef,
    StructuralEvent,
    TextBoundary,
    TextEvent,
    TextProjection,
    TextProjectionKind,
};
use markdown_it_benchmarks::corpus;

macro_rules! empty_transform {
    ($type:ident, $key:literal) => {
        #[derive(Default)]
        struct $type;

        impl DocumentTransform for $type {
            const KEY: &'static str = $key;

            fn run(&self, _: &Document) -> EditBatch {
                EditBatch::new()
            }
        }
    };
}

empty_transform!(EmptyTransform1, "empty-1");
empty_transform!(EmptyTransform2, "empty-2");
empty_transform!(EmptyTransform3, "empty-3");
empty_transform!(EmptyTransform4, "empty-4");
empty_transform!(EmptyTransform5, "empty-5");

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

fn one_edit_per_text_node(document: &markdown_it::Document) -> EditBatch {
    let mut batch = EditBatch::new();
    let mut previous = None;
    for event in document.text_events(TextProjection::new(text_projection)) {
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

    let empty_registry = DocumentTransformRegistry::new();
    let mut one_transform = DocumentTransformRegistry::new();
    one_transform.add::<EmptyTransform1>();
    let mut five_transforms = DocumentTransformRegistry::new();
    five_transforms.add::<EmptyTransform1>();
    five_transforms.add::<EmptyTransform2>();
    five_transforms.add::<EmptyTransform3>();
    five_transforms.add::<EmptyTransform4>();
    five_transforms.add::<EmptyTransform5>();
    let mut empty_document = MarkdownIt::empty().parse_document("");
    empty_registry.run(&mut empty_document);
    one_transform.run(&mut empty_document);
    five_transforms.run(&mut empty_document);

    let mut group = c.benchmark_group("document-transform-runner/empty-document");
    group.bench_function("zero-transforms", |b| {
        b.iter(|| empty_registry.run(black_box(&mut empty_document)))
    });
    group.bench_function("one-empty-transform", |b| {
        b.iter(|| one_transform.run(black_box(&mut empty_document)))
    });
    group.bench_function("five-empty-transforms", |b| {
        b.iter(|| five_transforms.run(black_box(&mut empty_document)))
    });
    group.finish();

    for corpus in corpus::standard() {
        let source = corpus.source();
        let mut group = c.benchmark_group(format!("document-build/{}", corpus.name));
        group.throughput(Throughput::Bytes(corpus.len() as u64));
        group.bench_function("arena-document", |b| {
            b.iter(|| black_box(md.parse_document(black_box(source))))
        });
        group.finish();
        let document = md.parse_document(source);
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

        let projection = TextProjection::new(text_projection);
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
                    let document = md.parse_document(source);
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
        let mut edited = md.parse_document(source);
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
                    let document = md.parse_document(source);
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
        let mut edited = md.parse_document(source);
        remove_top_level_subtrees(&edited).commit(&mut edited);
        assert_eq!(edited.len(), 1);
        assert!(edited.children(edited.root()).is_empty());
        let mut group = c.benchmark_group(format!("document-subtree-remove/{}", corpus.name));
        group.throughput(Throughput::Elements(removed_count as u64));
        group.bench_function("validate-and-commit", |b| {
            b.iter_batched(
                || {
                    let document = md.parse_document(source);
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
        let mut edited = md.parse_document(source);
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
                    let document = md.parse_document(source);
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
        let mut edited = md.parse_document(source);
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
                    let document = md.parse_document(source);
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
        let mut edited = md.parse_document(source);
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
                    let document = md.parse_document(source);
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
