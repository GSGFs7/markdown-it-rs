//! Ordered transforms for arena-backed documents.

use std::fmt;
use std::sync::Arc;

use crate::common::RuleMark;
use crate::common::ruler::{RuleItem, Ruler};
use crate::parser::document::Document;
use crate::parser::document_edit::{EditBatch, EditError};

trait ErasedDocumentTransform: Send + Sync {
    fn run(&self, document: &Document) -> EditBatch;
}

impl<T: DocumentTransform> ErasedDocumentTransform for T {
    fn run(&self, document: &Document) -> EditBatch {
        DocumentTransform::run(self, document)
    }
}

#[derive(Clone)]
struct RegisteredTransform {
    key: &'static str,
    transform: Arc<dyn ErasedDocumentTransform>,
}

impl fmt::Debug for RegisteredTransform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegisteredTransform")
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

/// A read-only analysis that produces one atomic batch of document edits.
///
/// `KEY` is the stable, user-facing identity used for ordering and error
/// reporting. It must be non-empty and unique within a registry.
pub trait DocumentTransform: Send + Sync + 'static {
    const KEY: &'static str;
    const ALIASES: &'static [&'static str] = &[];

    fn run(&self, document: &Document) -> EditBatch;
}

/// The failure produced when a registered transform's edit batch is invalid.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentTransformError {
    transform: &'static str,
    source: EditError,
}

impl DocumentTransformError {
    pub fn transform(&self) -> &'static str {
        self.transform
    }

    pub fn edit_error(&self) -> &EditError {
        &self.source
    }

    pub fn into_edit_error(self) -> EditError {
        self.source
    }
}

impl fmt::Display for DocumentTransformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "document transform {:?} failed: {}",
            self.transform, self.source
        )
    }
}

impl std::error::Error for DocumentTransformError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// An ordered registry of [`DocumentTransform`] implementations.
///
/// Each transform observes the document left by the preceding transform and
/// commits its own batch atomically. If a transform fails, later transforms
/// are skipped; batches committed by earlier transforms remain applied.
#[derive(Debug, Default)]
pub struct DocumentTransformRegistry {
    ruler: Ruler<RuleMark, RegisteredTransform>,
}

impl DocumentTransformRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register the default instance of `T`, returning a builder for
    /// ruler-style ordering constraints.
    pub fn add<T: DocumentTransform + Default>(&mut self) -> TransformRuleBuilder<'_> {
        self.add_instance(T::default())
    }

    /// Register an owned transform instance, returning a builder for
    /// ruler-style ordering constraints.
    pub fn add_instance<T: DocumentTransform>(&mut self, transform: T) -> TransformRuleBuilder<'_> {
        assert!(
            !T::KEY.is_empty(),
            "document transform key must not be empty"
        );
        assert!(
            !self.ruler.contains(RuleMark::named(T::KEY)),
            "duplicate document transform key: {:?}",
            T::KEY
        );
        assert!(
            !self.ruler.contains(RuleMark::of::<T>()),
            "document transform type is already registered: {:?}",
            std::any::type_name::<T>()
        );

        let item = self.ruler.add(
            RuleMark::named(T::KEY),
            RegisteredTransform {
                key: T::KEY,
                transform: Arc::new(transform),
            },
        );
        item.alias(RuleMark::of::<T>());
        for alias in T::ALIASES {
            item.alias(RuleMark::named(*alias));
        }
        TransformRuleBuilder::new(item)
    }

    pub fn contains<T: DocumentTransform>(&self) -> bool {
        self.ruler.contains(RuleMark::of::<T>())
    }

    pub fn remove<T: DocumentTransform>(&mut self) {
        self.ruler.remove(RuleMark::of::<T>());
    }

    /// Run all transforms in resolved order.
    pub fn run(&self, document: &mut Document) -> Result<(), DocumentTransformError> {
        for transform in self.ruler.iter() {
            let edits = transform.transform.run(document);
            edits
                .commit(document)
                .map_err(|source| DocumentTransformError {
                    transform: transform.key,
                    source,
                })?;
        }
        Ok(())
    }
}

/// Adjust the position and aliases of a newly registered document transform.
pub struct TransformRuleBuilder<'a> {
    item: &'a mut RuleItem<RuleMark, RegisteredTransform>,
}

impl<'a> TransformRuleBuilder<'a> {
    fn new(item: &'a mut RuleItem<RuleMark, RegisteredTransform>) -> Self {
        Self { item }
    }

    pub fn before<T: DocumentTransform>(self) -> Self {
        self.item.before(RuleMark::of::<T>());
        self
    }

    pub fn before_named(self, key: impl Into<std::sync::Arc<str>>) -> Self {
        self.item.before(RuleMark::named(key));
        self
    }

    pub fn after<T: DocumentTransform>(self) -> Self {
        self.item.after(RuleMark::of::<T>());
        self
    }

    pub fn after_named(self, key: impl Into<std::sync::Arc<str>>) -> Self {
        self.item.after(RuleMark::named(key));
        self
    }

    pub fn before_all(self) -> Self {
        self.item.before_all();
        self
    }

    pub fn after_all(self) -> Self {
        self.item.after_all();
        self
    }

    pub fn alias<T: DocumentTransform>(self) -> Self {
        self.item.alias(RuleMark::of::<T>());
        self
    }

    pub fn alias_named(self, key: impl Into<std::sync::Arc<str>>) -> Self {
        self.item.alias(RuleMark::named(key));
        self
    }

    pub fn require<T: DocumentTransform>(self) -> Self {
        self.item.require(RuleMark::of::<T>());
        self
    }

    pub fn require_named(self, key: impl Into<std::sync::Arc<str>>) -> Self {
        self.item.require(RuleMark::named(key));
        self
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::parser::inline::Text;
    use crate::{MarkdownIt, StructuralEvent};

    fn append_step(document: &Document, step: &str) -> EditBatch {
        let root = document.node(document.root()).unwrap();
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

    fn steps(document: &Document) -> Option<&str> {
        document
            .node(document.root())
            .unwrap()
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
        registry.run(&mut document).unwrap();

        assert_eq!(steps(&document), Some("123"));
    }

    #[test]
    fn empty_registry_is_a_noop() {
        let registry = DocumentTransformRegistry::new();
        let mut document = MarkdownIt::empty().parse_document("hello");
        let root = document.root();
        let len = document.len();

        registry.run(&mut document).unwrap();

        assert_eq!(document.root(), root);
        assert_eq!(document.len(), len);
    }

    #[derive(Default)]
    struct Failing;
    impl DocumentTransform for Failing {
        const KEY: &'static str = "failing";

        fn run(&self, document: &Document) -> EditBatch {
            let mut edits = EditBatch::new();
            edits.remove_node(document.root());
            edits
        }
    }

    #[test]
    fn reports_key_stops_and_keeps_prior_commits() {
        let mut registry = DocumentTransformRegistry::new();
        registry.add::<First>();
        registry.add::<Failing>();
        registry.add::<Third>();
        let mut document = MarkdownIt::empty().parse_document("");

        let error = registry.run(&mut document).unwrap_err();

        assert_eq!(error.transform(), "failing");
        assert_eq!(
            error.edit_error(),
            &EditError::CannotRemoveRoot(document.root())
        );
        assert_eq!(steps(&document), Some("1"));
        assert_eq!(
            std::error::Error::source(&error).unwrap().to_string(),
            error.edit_error().to_string()
        );
    }

    #[test]
    fn remove_invalidates_resolved_order() {
        let mut registry = DocumentTransformRegistry::new();
        registry.add::<First>();
        registry.add::<Second>();
        let mut first_run = MarkdownIt::empty().parse_document("");
        registry.run(&mut first_run).unwrap();
        assert_eq!(steps(&first_run), Some("12"));

        registry.remove::<First>();
        assert!(!registry.contains::<First>());
        let mut second_run = MarkdownIt::empty().parse_document("");
        registry.run(&mut second_run).unwrap();
        assert_eq!(steps(&second_run), Some("2"));
    }

    #[test]
    fn markdown_it_runner_is_explicit() {
        let mut md = MarkdownIt::empty();
        md.add_document_transform::<First>();

        let mut document = md.parse_document("");
        assert_eq!(steps(&document), None);

        md.run_document_transforms(&mut document).unwrap();
        assert_eq!(steps(&document), Some("1"));
    }

    #[derive(Default)]
    struct RewriteText;
    impl DocumentTransform for RewriteText {
        const KEY: &'static str = "rewrite-text";

        fn run(&self, document: &Document) -> EditBatch {
            let mut edits = EditBatch::new();
            for event in document.events(document.root()).unwrap() {
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

        md.run_document_transforms(&mut document).unwrap();

        assert_eq!(document.into_legacy().render(), "<p>rewritten</p>\n");
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

        md.run_document_transforms(&mut document).unwrap();

        assert_eq!(
            document.node(document.root()).unwrap().attrs(),
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
        registry.run(&mut document).unwrap();

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
}
