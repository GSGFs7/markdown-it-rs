use crate::{MarkdownIt, Node};

/// Each member of core rule chain must implement this trait
pub trait CoreRule: 'static {
    const NAMES: &'static [&'static str] = &[];

    fn run(root: &mut Node, md: &MarkdownIt);
}

macro_rules! rule_builder {
    ($var: ident) => {
        /// Adjust positioning of a newly added rule in the chain.
        pub struct RuleBuilder<'a, T> {
            item: &'a mut crate::common::ruler::RuleItem<crate::common::RuleMark, T>,
        }

        impl<'a, T> RuleBuilder<'a, T> {
            pub(crate) fn new(
                item: &'a mut crate::common::ruler::RuleItem<crate::common::RuleMark, T>,
            ) -> Self {
                Self { item }
            }

            pub fn before<U: $var>(self) -> Self {
                self.item.before(crate::common::RuleMark::of::<U>());
                self
            }

            pub fn before_mark(self, mark: crate::common::RuleMark) -> Self {
                self.item.before(mark);
                self
            }

            pub fn before_named(self, name: impl Into<std::sync::Arc<str>>) -> Self {
                self.before_mark(crate::common::RuleMark::named(name))
            }

            pub fn after<U: $var>(self) -> Self {
                self.item.after(crate::common::RuleMark::of::<U>());
                self
            }

            pub fn after_mark(self, mark: crate::common::RuleMark) -> Self {
                self.item.after(mark);
                self
            }

            pub fn after_named(self, name: impl Into<std::sync::Arc<str>>) -> Self {
                self.after_mark(crate::common::RuleMark::named(name))
            }

            pub fn before_all(self) -> Self {
                self.item.before_all();
                self
            }

            pub fn after_all(self) -> Self {
                self.item.after_all();
                self
            }

            pub fn alias<U: $var>(self) -> Self {
                self.item.alias(crate::common::RuleMark::of::<U>());
                self
            }

            pub fn alias_mark(self, mark: crate::common::RuleMark) -> Self {
                self.item.alias(mark);
                self
            }

            pub fn alias_named(self, name: impl Into<std::sync::Arc<str>>) -> Self {
                self.alias_mark(crate::common::RuleMark::named(name))
            }

            pub fn require<U: $var>(self) -> Self {
                self.item.require(crate::common::RuleMark::of::<U>());
                self
            }

            pub fn require_mark(self, mark: crate::common::RuleMark) -> Self {
                self.item.require(mark);
                self
            }

            pub fn require_named(self, name: impl Into<std::sync::Arc<str>>) -> Self {
                self.require_mark(crate::common::RuleMark::named(name))
            }
        }
    };
}

rule_builder!(CoreRule);

pub(crate) use rule_builder;
