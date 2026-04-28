use std::any::{TypeId, type_name};
use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

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
