//! Ordered transforms for documents.

use std::fmt;
use std::sync::Arc;

use crate::common::RuleMark;
use crate::common::ruler::{RuleItem, Ruler};
use crate::document::Document;
use crate::document::edit::EditBatch;

trait ErasedTransform: Send + Sync {
    fn run(&self, document: &Document) -> EditBatch;
}

impl<T: Transform> ErasedTransform for T {
    fn run(&self, document: &Document) -> EditBatch {
        Transform::run(self, document)
    }
}

#[derive(Clone)]
struct RegisteredTransform {
    key: &'static str,
    transform: Arc<dyn ErasedTransform>,
}

impl fmt::Debug for RegisteredTransform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegisteredTransform")
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

/// A read-only analysis that produces a batch of document edits.
///
/// `KEY` is the stable, user-facing identity used for registration and ordering.
/// It must be non-empty and unique within a registry.
pub trait Transform: Send + Sync + 'static {
    const KEY: &'static str;
    const ALIASES: &'static [&'static str] = &[];

    fn run(&self, document: &Document) -> EditBatch;
}

/// An ordered registry of [`Transform`] implementations.
///
/// Each transform observes the document left by the preceding transform.
/// Plugins must produce valid edit batches. Panics are not caught here;
/// preceding changes are not rolled back. Invalid batches provide no recovery guarantee.
#[derive(Debug, Default)]
pub struct TransformRegistry {
    ruler: Ruler<RuleMark, RegisteredTransform>,
}

impl TransformRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register the default instance of `T`, returning a builder for
    /// ruler-style ordering constraints.
    pub fn add<T: Transform + Default>(&mut self) -> TransformRuleBuilder<'_> {
        self.add_instance(T::default())
    }

    /// Register an owned transform instance, returning a builder for
    /// ruler-style ordering constraints.
    pub fn add_instance<T: Transform>(&mut self, transform: T) -> TransformRuleBuilder<'_> {
        assert!(!T::KEY.is_empty(), "transform key must not be empty");
        assert!(
            !self.ruler.contains(RuleMark::named(T::KEY)),
            "duplicate transform key: {:?}",
            T::KEY
        );
        assert!(
            !self.ruler.contains(RuleMark::of::<T>()),
            "transform type is already registered: {:?}",
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

    pub fn contains<T: Transform>(&self) -> bool {
        self.ruler.contains(RuleMark::of::<T>())
    }

    pub fn remove<T: Transform>(&mut self) {
        self.ruler.remove(RuleMark::of::<T>());
    }

    /// Run all transforms in resolved order.
    pub fn run(&self, document: &mut Document) {
        for transform in self.ruler.iter() {
            transform.transform.run(document).commit(document);
        }
    }
}

/// Adjust the position and aliases of a newly registered transform.
pub struct TransformRuleBuilder<'a> {
    item: &'a mut RuleItem<RuleMark, RegisteredTransform>,
}

impl<'a> TransformRuleBuilder<'a> {
    fn new(item: &'a mut RuleItem<RuleMark, RegisteredTransform>) -> Self {
        Self { item }
    }

    pub fn before<T: Transform>(self) -> Self {
        self.item.before(RuleMark::of::<T>());
        self
    }

    pub fn before_named(self, key: impl Into<std::sync::Arc<str>>) -> Self {
        self.item.before(RuleMark::named(key));
        self
    }

    pub fn after<T: Transform>(self) -> Self {
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

    pub fn alias<T: Transform>(self) -> Self {
        self.item.alias(RuleMark::of::<T>());
        self
    }

    pub fn alias_named(self, key: impl Into<std::sync::Arc<str>>) -> Self {
        self.item.alias(RuleMark::named(key));
        self
    }

    pub fn require<T: Transform>(self) -> Self {
        self.item.require(RuleMark::of::<T>());
        self
    }

    pub fn require_named(self, key: impl Into<std::sync::Arc<str>>) -> Self {
        self.item.require(RuleMark::named(key));
        self
    }
}

#[cfg(test)]
mod tests;
