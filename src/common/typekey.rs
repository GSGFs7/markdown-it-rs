use std::any::{TypeId, type_name};
use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Lightweight hashing for maps whose keys are program-defined TypeIds.
/// Do not use for arbitrary input-controlled keys.
#[derive(Default)]
pub(crate) struct TypeIdHasher(u64);

impl Hasher for TypeIdHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        // Fallback for a future TypeId Hash implementation that does not use
        // one of the integer-specific Hasher methods.
        let mut hash = self.0 ^ 0xcbf2_9ce4_8422_2325;
        for &byte in bytes {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        self.0 = hash;
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = self.0.rotate_left(5) ^ value;
    }

    fn write_u128(&mut self, value: u128) {
        self.write_u64(value as u64);
        self.write_u64((value >> 64) as u64);
    }
}

/// A [`TypeId`] paired with its [`type_name`] for debugging.
///
/// Equality and hashing use only the type ID. Debug output uses only the type name.
///
/// ```
/// # use markdown_it::common::TypeKey;
/// struct A;
/// struct B;
///
/// let mut set = std::collections::HashSet::new();
///
/// set.insert(TypeKey::of::<A>());
/// set.insert(TypeKey::of::<B>());
///
/// assert!(set.contains(&TypeKey::of::<A>()));
/// dbg!(set);
/// ```
#[derive(Clone, Copy)]
pub struct TypeKey {
    /// The type's identity.
    id: TypeId,
    /// The type's name for debugging.
    name: &'static str,
}

impl TypeKey {
    /// Returns the type ID and debugging name of `T`.
    #[must_use]
    pub fn of<T: ?Sized + 'static>() -> Self {
        Self {
            id: TypeId::of::<T>(),
            name: type_name::<T>(),
        }
    }

    #[must_use]
    pub const fn id(&self) -> TypeId {
        self.id
    }

    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }
}

impl Hash for TypeKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl PartialEq for TypeKey {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for TypeKey {}

impl Debug for TypeKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str(self.name)
    }
}

/// A rule identifier based on either a Rust type or a dynamic name.
///
/// Type-based and name-based identifiers are distinct, even when their debug
/// names match.
#[derive(Clone, Eq, PartialEq, Hash)]
pub enum RuleMark {
    /// Identifies a rule by its Rust type.
    Type(TypeKey),
    /// Identifies a rule by a shared name, such as one supplied by a binding.
    Name(Arc<str>),
}

impl RuleMark {
    /// Returns a type-based rule identifier for `T`.
    #[must_use]
    pub fn of<T: 'static>() -> Self {
        Self::Type(TypeKey::of::<T>())
    }

    /// Returns a name-based rule identifier.
    #[must_use]
    pub fn named(name: impl Into<Arc<str>>) -> Self {
        Self::Name(name.into())
    }
}

impl Debug for RuleMark {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            RuleMark::Type(key) => key.fmt(f),
            RuleMark::Name(name) => write!(f, "{name:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::any::{TypeId, type_name};
    use std::collections::HashSet;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{BuildHasher, BuildHasherDefault};

    use super::{RuleMark, TypeIdHasher, TypeKey};

    #[test]
    fn typekey_eq() {
        struct A;
        struct B;
        assert_eq!(
            TypeKey {
                id: std::any::TypeId::of::<A>(),
                name: "foo"
            },
            TypeKey {
                id: std::any::TypeId::of::<A>(),
                name: "bar"
            }
        );
        assert_ne!(
            TypeKey {
                id: std::any::TypeId::of::<A>(),
                name: "foo"
            },
            TypeKey {
                id: std::any::TypeId::of::<B>(),
                name: "foo"
            }
        );
    }

    #[test]
    fn typekey_of() {
        struct A;
        struct B;
        assert_eq!(TypeKey::of::<A>(), TypeKey::of::<A>());
        assert_ne!(TypeKey::of::<A>(), TypeKey::of::<B>());
    }

    #[test]
    fn typekey_hash_uses_only_id() {
        let hasher = BuildHasherDefault::<DefaultHasher>::default();
        let first = TypeKey {
            id: TypeId::of::<str>(),
            name: "first",
        };
        let second = TypeKey {
            id: first.id,
            name: "second",
        };

        assert_eq!(hasher.hash_one(first), hasher.hash_one(first.id));
        assert_eq!(hasher.hash_one(first), hasher.hash_one(second));

        let mut keys = HashSet::new();
        assert!(keys.insert(first));
        assert!(!keys.insert(second));
        assert_eq!(keys.len(), 1);

        let mut keys = HashSet::<_, BuildHasherDefault<TypeIdHasher>>::default();
        assert!(keys.insert(first));
        assert!(!keys.insert(second));
        assert_eq!(keys.len(), 1);
    }

    #[test]
    fn typekey_debug_uses_name() {
        let key = TypeKey::of::<str>();
        assert_eq!(key.id, TypeId::of::<str>());
        assert_eq!(key.name, type_name::<str>());
        assert_eq!(format!("{key:?}"), key.name);
    }

    #[test]
    fn rulemark_identity() {
        assert_eq!(RuleMark::of::<u32>(), RuleMark::of::<u32>());
        assert_ne!(RuleMark::of::<u32>(), RuleMark::of::<u64>());
        assert_eq!(
            RuleMark::named("rule"),
            RuleMark::named(String::from("rule"))
        );
        assert_ne!(RuleMark::named("first"), RuleMark::named("second"));
        assert_ne!(RuleMark::of::<u32>(), RuleMark::named(type_name::<u32>()));

        let mut marks = HashSet::new();
        assert!(marks.insert(RuleMark::of::<u32>()));
        assert!(!marks.insert(RuleMark::of::<u32>()));
        assert!(marks.insert(RuleMark::named("rule")));
        assert!(!marks.insert(RuleMark::named(String::from("rule"))));
        assert_eq!(marks.len(), 2);
    }

    #[test]
    fn rulemark_debug() {
        let typed = RuleMark::of::<u32>();
        let named = RuleMark::named("rule");

        assert_eq!(format!("{typed:?}"), type_name::<u32>());
        assert_eq!(format!("{named:?}"), "\"rule\"");
    }
}
