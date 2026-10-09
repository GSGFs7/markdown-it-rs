use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::document::Text;
use crate::{MarkdownIt, StructuralEvent};

fn append_step(document: &Document, step: &str) -> EditBatch {
    let root = document.node(document.root());
    let previous = root
        .attrs()
        .iter()
        .find(|(name, _)| name == "steps")
        .map_or("", |(_, value)| value.as_str());
    let mut edits = EditBatch::new();
    edits.set_attribute(document.root(), "steps", format!("{previous}{step}"));
    edits
}

#[derive(Default)]
struct First;
impl DocumentTransform for First {
    const KEY: &'static str = "first";
    const ALIASES: &'static [&'static str] = &["opening"];

    fn run(&self, document: &Document) -> EditBatch {
        append_step(document, "1")
    }
}

#[derive(Default)]
struct Second;
impl DocumentTransform for Second {
    const KEY: &'static str = "second";

    fn run(&self, document: &Document) -> EditBatch {
        append_step(document, "2")
    }
}

#[derive(Default)]
struct Third;
impl DocumentTransform for Third {
    const KEY: &'static str = "third";

    fn run(&self, document: &Document) -> EditBatch {
        append_step(document, "3")
    }
}

#[cfg(debug_assertions)]
#[derive(Default)]
struct Failing;
#[cfg(debug_assertions)]
impl DocumentTransform for Failing {
    const KEY: &'static str = "failing";

    fn run(&self, document: &Document) -> EditBatch {
        let mut edits = EditBatch::new();
        edits.remove_node(document.root());
        edits
    }
}

fn steps(document: &Document) -> Option<&str> {
    document
        .node(document.root())
        .attrs()
        .iter()
        .find(|(name, _)| name == "steps")
        .map(|(_, value)| value.as_str())
}

#[test]
fn runs_in_resolved_type_and_named_order() {
    let mut registry = DocumentTransformRegistry::new();
    registry.add::<Second>().after::<First>();
    registry.add::<Third>().after_named("opening");
    registry.add::<First>().before_all();

    let mut document = MarkdownIt::empty().parse_document("");
    registry.run(&mut document);

    assert_eq!(steps(&document), Some("123"));
}

#[cfg(debug_assertions)]
#[test]
fn invalid_transform_panics_during_debug_validation() {
    let mut registry = DocumentTransformRegistry::new();
    registry.add::<First>();
    registry.add::<Failing>();
    registry.add::<Third>();
    let mut document = MarkdownIt::empty().parse_document("");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        registry.run(&mut document);
    }));
    let panic = result.expect_err("invalid transform must panic in debug builds");
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(message.contains("invalid edit batch"));
    assert_eq!(steps(&document), Some("1"));
}

#[test]
fn empty_registry_is_a_noop() {
    let registry = DocumentTransformRegistry::new();
    let mut document = MarkdownIt::empty().parse_document("hello");
    let root = document.root();
    let len = document.len();

    registry.run(&mut document);

    assert_eq!(document.root(), root);
    assert_eq!(document.len(), len);
}

#[test]
fn remove_invalidates_resolved_order() {
    let mut registry = DocumentTransformRegistry::new();
    registry.add::<First>();
    registry.add::<Second>();
    let mut first_run = MarkdownIt::empty().parse_document("");
    registry.run(&mut first_run);
    assert_eq!(steps(&first_run), Some("12"));

    registry.remove::<First>();
    assert!(!registry.contains::<First>());
    let mut second_run = MarkdownIt::empty().parse_document("");
    registry.run(&mut second_run);
    assert_eq!(steps(&second_run), Some("2"));
}

#[test]
fn parser_runs_transforms_once_and_explicit_reruns_are_opt_in() {
    let mut md = MarkdownIt::empty();
    md.add_document_transform::<First>();

    let mut document = md.parse_document("");
    assert_eq!(steps(&document), Some("1"));

    md.run_document_transforms(&mut document);
    assert_eq!(steps(&document), Some("11"));
    crate::plugins::cmark::add(&mut md);
    assert_eq!(steps(&md.parse_document("text")), Some("1"));
}

#[derive(Default)]
struct RewriteText;
impl DocumentTransform for RewriteText {
    const KEY: &'static str = "rewrite-text";

    fn run(&self, document: &Document) -> EditBatch {
        let mut edits = EditBatch::new();
        for event in document.events(document.root()) {
            if let StructuralEvent::Leaf(node) = event
                && let Some(text) = node.cast::<Text>()
            {
                edits.replace_text(node.id(), 0..text.content.len(), "rewritten");
            }
        }
        edits
    }
}

#[test]
fn transform_can_build_edits_from_an_immutable_document() {
    let mut md = MarkdownIt::empty();
    crate::plugins::cmark::add(&mut md);
    md.add_document_transform::<RewriteText>();
    let mut document = md.parse_document("original");

    md.run_document_transforms(&mut document);

    assert_eq!(md.render_document(&document), "<p>rewritten</p>\n");
}

#[derive(Default)]
struct DuplicateKey;
impl DocumentTransform for DuplicateKey {
    const KEY: &'static str = "first";

    fn run(&self, _: &Document) -> EditBatch {
        EditBatch::new()
    }
}

#[test]
#[should_panic(expected = "duplicate document transform key")]
fn rejects_duplicate_stable_keys() {
    let mut registry = DocumentTransformRegistry::new();
    registry.add::<First>();
    registry.add::<DuplicateKey>();
}

struct ConfiguredTransform {
    value: String,
}

impl DocumentTransform for ConfiguredTransform {
    const KEY: &'static str = "configured";

    fn run(&self, document: &Document) -> EditBatch {
        let mut edits = EditBatch::new();
        edits.set_attribute(document.root(), "configured", self.value.clone());
        edits
    }
}

#[test]
fn markdown_it_registers_an_owned_configured_instance() {
    let mut md = MarkdownIt::empty();
    md.add_document_transform_instance(ConfiguredTransform {
        value: "runtime value".into(),
    });
    let mut document = md.parse_document("");

    md.run_document_transforms(&mut document);

    assert_eq!(
        document.node(document.root()).attrs(),
        &[("configured".into(), "runtime value".into())]
    );
}

struct DropProbe {
    drops: Arc<AtomicUsize>,
}

impl Drop for DropProbe {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::Relaxed);
    }
}

impl DocumentTransform for DropProbe {
    const KEY: &'static str = "drop-probe";

    fn run(&self, _: &Document) -> EditBatch {
        EditBatch::new()
    }
}

#[test]
fn remove_invalidates_the_cache_and_drops_the_instance_once() {
    let drops = Arc::new(AtomicUsize::new(0));
    let mut registry = DocumentTransformRegistry::new();
    registry.add_instance(DropProbe {
        drops: Arc::clone(&drops),
    });
    let mut document = MarkdownIt::empty().parse_document("");
    registry.run(&mut document);

    registry.remove::<DropProbe>();

    assert_eq!(drops.load(Ordering::Relaxed), 1);
}

struct TypeAliasCollision;

impl DocumentTransform for TypeAliasCollision {
    const KEY: &'static str = "type-alias-collision";

    fn run(&self, _: &Document) -> EditBatch {
        EditBatch::new()
    }
}

#[test]
#[should_panic(expected = "document transform type is already registered")]
fn rejects_a_type_already_used_as_an_alias() {
    let mut registry = DocumentTransformRegistry::new();
    registry.add::<First>().alias::<TypeAliasCollision>();
    registry.add_instance(TypeAliasCollision);
}

#[test]
fn registry_debug_does_not_require_transform_debug() {
    let mut registry = DocumentTransformRegistry::new();
    registry.add_instance(ConfiguredTransform {
        value: "secret configuration".into(),
    });

    let debug = format!("{registry:?}");

    assert!(debug.contains("configured"));
    assert!(!debug.contains("secret configuration"));
}

#[test]
fn markdown_it_remains_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<MarkdownIt>();
}
