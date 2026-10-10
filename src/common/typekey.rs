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

#[readonly::make]
#[derive(Clone, Copy)]
/// [std::any::TypeId] and [std::any::type_name] fused into one struct.
///
/// It acts as TypeId when hashed or compared, and it acts as type_name when printed.
/// Used to improve debuggability of type ids in hashmaps in particular.
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
pub struct TypeKey {
    /// type id (read only)
    pub id: TypeId,
    /// type name (read only)
    pub name: &'static str,
}

impl TypeKey {
    #[must_use]
    /// Similar to [TypeId::of](TypeId::of), returns `TypeKey`
    /// of the type this generic function has been instantiated with.
    pub fn of<T: ?Sized + 'static>() -> Self {
        Self {
            id: TypeId::of::<T>(),
            name: type_name::<T>(),
        }
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
        write!(f, "{}", self.name)
    }
}

#[derive(Clone, Eq, PartialEq, Hash)]
pub enum RuleMark {
    Type(TypeKey),  // rust or static rule
    Name(Arc<str>), // python or dynamic rule
}

impl RuleMark {
    pub fn of<T: 'static>() -> Self {
        Self::Type(TypeKey::of::<T>())
    }

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
    use super::TypeKey;

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
}
