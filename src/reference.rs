//! Document-level link reference storage.
//!
//! This is the Rust equivalent of markdown-it.js's `env.references`: a
//! well-known document extension that link rules read and the reference
//! definition scanner writes. It deliberately has no plugin dependency.
//! Insert one into the document root extensions to supply external references.
//!
//! ```rust
//! use markdown_it::document::Root;
//! use markdown_it::parser::block::BlockParserRule;
//! use markdown_it::parser::core::{CoreRule, CoreState};
//! use markdown_it::reference::{CustomReferenceMap, DefaultReferenceMap, ReferenceMap};
//! use markdown_it::MarkdownIt;
//!
//! let md = &mut MarkdownIt::empty();
//! markdown_it::plugins::cmark::add(md);
//!
//! #[derive(Debug, Default)]
//! struct RefMapOverride(DefaultReferenceMap);
//! impl CustomReferenceMap for RefMapOverride {
//!     fn get(&self, label: &str) -> Option<(&str, Option<&str>)> {
//!         // override a specific link
//!         if label == "rust" {
//!             return Some((
//!                 "https://www.rust-lang.org/",
//!                 Some("The Rust Language"),
//!             ));
//!         }
//!
//!         self.0.get(label)
//!     }
//!
//!     fn insert(&mut self, label: String, destination: String, title: Option<String>) -> bool {
//!         self.0.insert(label, destination, title)
//!     }
//! }
//!
//! #[derive(Default)]
//! struct AddCustomReferences;
//! impl CoreRule for AddCustomReferences {
//!     fn run(&self, state: &mut CoreState<'_>) {
//!         state.root_ext_mut().insert(ReferenceMap::new(RefMapOverride::default()));
//!     }
//! }
//!
//! md.add_rule::<AddCustomReferences>()
//!     .before::<BlockParserRule>();
//!
//! let html = md.render("[rust]");
//! assert_eq!(
//!     html.trim(),
//!     r#"<p><a href="https://www.rust-lang.org/" title="The Rust Language">rust</a></p>"#
//! );
//! ```
//!
//! You can also view all references that user created by adding the following rule:
//!
//! ```rust
//! use markdown_it::document::Root;
//! use markdown_it::reference::{DefaultReferenceMap, ReferenceMap};
//! use markdown_it::MarkdownIt;
//!
//! let md = &mut MarkdownIt::empty();
//! markdown_it::plugins::cmark::add(md);
//!
//! let ast = md.parse("[hello]: world");
//! let root = ast.node(ast.root()).cast::<Root>().unwrap();
//! let refmap = root.ext.get::<ReferenceMap>()
//!     .map(|m| m.downcast_ref::<DefaultReferenceMap>().expect("expect references to be handled by default map"));
//!
//! let mut labels = vec![];
//! if let Some(refmap) = refmap {
//!     for (label, _dest, _title) in refmap.iter() {
//!         labels.push(label);
//!     }
//! }
//!
//! assert_eq!(labels, ["hello"]);
//! ```

use std::collections::HashMap;
use std::fmt::Debug;

use downcast_rs::{Downcast, impl_downcast};

use crate::common::utils::normalize_reference;

/// Storage for parsed references.
///
/// Insert this into the document root extensions to make reference definitions
/// and external references available to link rules.
#[derive(Debug)]
pub struct ReferenceMap(Box<dyn CustomReferenceMap>);

impl ReferenceMap {
    pub fn new(custom_map: impl CustomReferenceMap + 'static) -> Self {
        Self(Box::new(custom_map))
    }
}

impl Default for ReferenceMap {
    fn default() -> Self {
        Self::new(DefaultReferenceMap::new())
    }
}

impl std::ops::Deref for ReferenceMap {
    type Target = dyn CustomReferenceMap;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl std::ops::DerefMut for ReferenceMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.as_mut()
    }
}

/// A reference table that can replace the default [`DefaultReferenceMap`].
pub trait CustomReferenceMap: Debug + Downcast + Send + Sync {
    /// Insert new element to the reference map. You may return false if it's not a valid label to stop parsing.
    fn insert(&mut self, label: String, destination: String, title: Option<String>) -> bool;

    /// Get an element referenced by `label` from the map, returns destination and optional title.
    fn get(&self, label: &str) -> Option<(&str, Option<&str>)>;

    /// Whether the map is known to be empty (defaults to `false`: a custom map
    /// may not be enumerable, so only return `true` when emptiness is certain).
    fn is_empty(&self) -> bool {
        false
    }
}

impl_downcast!(CustomReferenceMap);

/// Default CommonMark reference storage.
#[derive(Default, Debug)]
pub struct DefaultReferenceMap {
    entries: HashMap<ReferenceMapKey, ReferenceMapEntry>,
    max_normalized_len: usize,
}

impl DefaultReferenceMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str, Option<&str>)> {
        Box::new(
            self.entries
                .iter()
                .map(|(a, b)| (a.label.as_str(), b.destination.as_str(), b.title.as_deref())),
        )
    }
}

impl CustomReferenceMap for DefaultReferenceMap {
    fn insert(&mut self, label: String, destination: String, title: Option<String>) -> bool {
        let Some(key) = ReferenceMapKey::new(label) else {
            return false;
        };
        self.max_normalized_len = self.max_normalized_len.max(key.normalized.len());
        self.entries
            .entry(key)
            .or_insert(ReferenceMapEntry::new(destination, title));
        true
    }

    fn get(&self, label: &str) -> Option<(&str, Option<&str>)> {
        if label
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .take(self.max_normalized_len.saturating_add(1))
            .count()
            > self.max_normalized_len
        {
            return None;
        }

        let key = ReferenceMapKey::new(label.to_owned())?;
        self.entries
            .get(&key)
            .map(|r| (r.destination.as_str(), r.title.as_deref()))
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Reference label
#[derive(Debug, Default)]
struct ReferenceMapKey {
    pub label: String,
    normalized: String,
}

impl PartialEq for ReferenceMapKey {
    fn eq(&self, other: &Self) -> bool {
        self.normalized == other.normalized
    }
}

impl Eq for ReferenceMapKey {}

impl std::hash::Hash for ReferenceMapKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.normalized.hash(state);
    }
}

impl ReferenceMapKey {
    fn new(label: String) -> Option<Self> {
        let normalized = normalize_reference(&label);

        if normalized.is_empty() {
            // CommonMark 0.20 disallows empty labels
            return None;
        }

        Some(Self { label, normalized })
    }
}

/// Reference value
#[derive(Debug, Default)]
struct ReferenceMapEntry {
    pub destination: String,
    pub title: Option<String>,
}

impl ReferenceMapEntry {
    fn new(destination: String, title: Option<String>) -> Self {
        Self { destination, title }
    }
}

#[cfg(test)]
mod tests {
    use super::{CustomReferenceMap, DefaultReferenceMap};

    #[test]
    fn lookup_length_filter_preserves_whitespace_and_unicode_normalization() {
        let mut map = DefaultReferenceMap::new();
        for (stored, query) in [("r", "R"), ("ss", "ß"), ("k", "K"), ("a b", "A\u{2003} B")] {
            assert!(map.insert(stored.to_owned(), stored.to_owned(), None));
            assert_eq!(map.get(query), Some((stored, None)), "{query}");
        }
        let padded = format!("{}R{}", " ".repeat(4096), "\t".repeat(4096));
        assert_eq!(map.get(&padded), Some(("r", None)));
        assert_eq!(map.get(&"[".repeat(50_000)), None);

        // Publicly inserted maps can contain labels longer than parsed definitions.
        let long = "a".repeat(2048);
        assert!(map.insert(long.clone(), "long".to_owned(), None));
        assert_eq!(map.get(&long), Some(("long", None)));
    }
}
