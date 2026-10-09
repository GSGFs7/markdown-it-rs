//! Extension sets
//!
//! These things allow you to put custom data inside internal markdown-it structures.
//!

use std::fmt::Debug;

use downcast_rs::{Downcast, impl_downcast};

/// A value that can be stored in an extension set.
///
/// This trait is implemented automatically for every `Debug + Send + Sync + 'static` type.
pub trait Extension: Debug + Downcast + Send + Sync {}
impl<T: Debug + Downcast + Send + Sync> Extension for T {}
impl_downcast!(Extension);

// see https://github.com/malobre/erased_set for inspiration and API
// see https://lucumr.pocoo.org/2022/1/7/as-any-hack/ for additional impl details
macro_rules! extension_set {
    ($(#[$meta:meta])* $name: ident) => {
        $(#[$meta])*
        #[derive(Debug, Default)]
        pub struct $name(::std::collections::HashMap<crate::common::TypeKey, Box<dyn Extension>>);

        impl $name {
            #[must_use]
            pub fn new() -> Self {
                Self::default()
            }

            #[must_use]
            pub fn is_empty(&self) -> bool {
                self.0.is_empty()
            }

            #[must_use]
            pub fn len(&self) -> usize {
                self.0.len()
            }

            pub fn clear(&mut self) {
                self.0.clear();
            }

            #[must_use]
            pub fn contains<T: Extension>(&self) -> bool {
                let key = crate::common::TypeKey::of::<T>();
                self.0.contains_key(&key)
            }

            #[must_use]
            pub fn get<T: Extension>(&self) -> Option<&T> {
                let key = crate::common::TypeKey::of::<T>();
                let result = self.0.get(&key)?;
                result.downcast_ref::<T>()
            }

            #[must_use]
            pub fn get_mut<T: Extension>(&mut self) -> Option<&mut T> {
                let key = crate::common::TypeKey::of::<T>();
                let result = self.0.get_mut(&key)?;
                result.downcast_mut::<T>()
            }

            pub fn get_or_insert<T: Extension>(&mut self, value: T) -> &mut T {
                let key = crate::common::TypeKey::of::<T>();
                let result = self.0.entry(key).or_insert_with(|| Box::new(value));
                result.downcast_mut::<T>().unwrap()
            }

            pub fn get_or_insert_with<T: Extension>(&mut self, f: impl FnOnce() -> T) -> &mut T {
                let key = crate::common::TypeKey::of::<T>();
                let result = self.0.entry(key).or_insert_with(|| Box::new(f()));
                result.downcast_mut::<T>().unwrap()
            }

            pub fn get_or_insert_default<T: Extension + Default>(&mut self) -> &mut T {
                let key = crate::common::TypeKey::of::<T>();
                let result = self.0.entry(key).or_insert_with(|| Box::<T>::default());
                result.downcast_mut::<T>().unwrap()
            }

            pub fn insert<T: Extension>(&mut self, value: T) -> Option<T> {
                let key = crate::common::TypeKey::of::<T>();
                let result = self.0.insert(key, Box::new(value))?;
                Some(*result.downcast::<T>().unwrap())
            }

            pub fn remove<T: Extension>(&mut self) -> Option<T> {
                let key = crate::common::TypeKey::of::<T>();
                let result = self.0.remove(&key)?;
                Some(*result.downcast::<T>().unwrap())
            }
        }
    };
}

extension_set!(
    /// Extension storage for the entire parser (only writable at init).
    MarkdownItExtSet
);

extension_set!(
    /// Extension storage for an arbitrary AST node.
    NodeExtSet
);

extension_set!(
    /// Extension storage for an inline context.
    InlineRootExtSet
);

extension_set!(
    /// Extension storage for a block context.
    RootExtSet
);

extension_set!(
    /// Extension storage for a renderer context.
    RenderExtSet
);

#[cfg(test)]
mod tests {
    use super::{
        Extension,
        InlineRootExtSet,
        MarkdownItExtSet,
        NodeExtSet,
        RenderExtSet,
        RootExtSet,
    };

    extension_set!(TestExtSet);

    #[test]
    fn extension_types_do_not_require_marker_impls() {
        #[derive(Debug, PartialEq, Eq)]
        struct State(u8);

        let mut markdown_it = MarkdownItExtSet::new();
        let mut node = NodeExtSet::new();
        let mut inline_root = InlineRootExtSet::new();
        let mut root = RootExtSet::new();
        let mut render = RenderExtSet::new();

        markdown_it.insert(State(1));
        node.insert(State(2));
        inline_root.insert(State(3));
        root.insert(State(4));
        render.insert(State(5));

        assert_eq!(markdown_it.get::<State>(), Some(&State(1)));
        assert_eq!(node.get::<State>(), Some(&State(2)));
        assert_eq!(inline_root.get::<State>(), Some(&State(3)));
        assert_eq!(root.get::<State>(), Some(&State(4)));
        assert_eq!(render.get::<State>(), Some(&State(5)));
    }

    #[test]
    fn empty_set() {
        let set = TestExtSet::new();
        assert_eq!(set.len(), 0);
        assert!(set.is_empty());
    }

    #[test]
    fn insert_elements() {
        let mut set = TestExtSet::new();
        set.insert(42u8);
        assert_eq!(set.len(), 1);
        assert!(!set.is_empty());
        set.insert(42u16);
        assert_eq!(set.len(), 2);
        assert!(!set.is_empty());
    }

    #[test]
    fn contains() {
        let mut set = TestExtSet::new();
        set.insert(42u8);
        assert!(!set.contains::<u16>());
        set.insert(42u16);
        assert!(set.contains::<u16>());
        set.remove::<u16>();
        assert!(!set.contains::<u16>());
    }

    #[test]
    fn get() {
        let mut set = TestExtSet::new();
        set.insert(42u8);
        assert_eq!(set.get::<u16>(), None);
        set.insert(42u16);
        set.insert(123u16);
        assert_eq!(set.get::<u16>(), Some(&123u16));
    }

    #[test]
    fn get_mut() {
        let mut set = TestExtSet::new();
        set.insert(42u16);
        *set.get_mut::<u16>().unwrap() = 123u16;
        assert_eq!(set.get::<u16>(), Some(&123u16));
    }

    #[test]
    fn or_insert() {
        let mut set = TestExtSet::new();
        set.insert(123u8);
        assert_eq!(set.get_or_insert(0u8), &mut 123u8);
        assert_eq!(set.get_or_insert_default::<u8>(), &mut 123u8);
        assert_eq!(set.get_or_insert_with(|| 0u8), &mut 123u8);
        set.clear();
        assert_eq!(set.get_or_insert(10u8), &mut 10u8);
        set.clear();
        assert_eq!(set.get_or_insert_with(|| 20u8), &mut 20u8);
        set.clear();
        assert_eq!(set.get_or_insert_default::<u8>(), &mut 0u8);
    }

    #[test]
    fn different_types_stored_once() {
        let mut set = TestExtSet::new();
        set.insert("foo");
        set.insert("bar");
        set.insert("quux");
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn zero_sized_types() {
        #[derive(Debug, PartialEq, Eq)]
        struct A;
        #[derive(Debug, PartialEq, Eq)]
        struct B;
        let mut set = TestExtSet::new();
        set.insert(A);
        set.insert(B);
        assert_eq!(set.len(), 2);
        assert_eq!(set.get::<A>(), Some(&A));
    }

    #[test]
    fn clear() {
        let mut set = TestExtSet::new();
        set.insert(42u8);
        set.insert(42u16);
        assert_eq!(set.len(), 2);
        set.clear();
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn debug() {
        let mut set = TestExtSet::new();
        set.insert(42);
        set.insert("test");
        let str = format!("{:?}", set);
        // there are no guarantees about field order, so check both
        assert!(
            str == "TestExtSet({i32: 42, &str: \"test\"})"
                || str == "TestExtSet({&str: \"test\", i32: 42})"
        );
    }
}
