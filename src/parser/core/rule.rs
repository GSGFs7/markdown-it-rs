use crate::document::NodeDraft;
use crate::parser::extset::RootExtSet;
use crate::parser::main::MarkdownIt;
use crate::parser::node::Node;

/// Prepare shared state from the source. Runs before or after block parsing,
/// as placed in the core ruler.
pub(crate) type DocumentPrepareStateFn = fn(&str, &MarkdownIt, &mut RootExtSet);

/// Finalize the resolved draft tree from shared state. Runs after inline
/// parsing, before arena conversion.
pub(crate) type DocumentFinalizeDraftFn = fn(&mut NodeDraft, &RootExtSet);

/// Experimental direct counterpart of a core rule.
///
/// Registered through [`CoreRule::document_rule`] and sharing the legacy rule's
/// ruler position. Stages run in this order:
///
/// ```text
/// PrepareState (before blocks) -> ParseBlocks -> PrepareState (after blocks)
///     -> ParseInlines -> FinalizeDraft -> persist RootExtSet -> Document
/// ```
///
/// Exactly one `ParseBlocks` and one `ParseInlines` are required, in that order;
/// callbacks outside their stage are rejected and preserve core-ruler order.
/// The pure-text fast path still runs preparations and finalizers. Arena-backed
/// document transforms remain a separate, explicitly executed pipeline.
#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub enum DocumentCoreRule {
    /// Build the block draft tree, deferring inline content until all block
    /// definitions are available. Marks the built-in block parser.
    ParseBlocks,
    /// Resolve deferred inline content from block-pass state. Must follow
    /// `ParseBlocks`; marks the built-in inline parser.
    ParseInlines,
    /// Prepare [`RootExtSet`] from the source. Must precede `ParseInlines`;
    /// placed before `ParseBlocks` it runs before block parsing, otherwise
    /// after (with block state available).
    PrepareState(DocumentPrepareStateFn),
    /// Modify the resolved [`NodeDraft`], e.g. to move footnote definitions to
    /// the end. Must follow `ParseInlines`; state is read-only and persisted to
    /// `Root.ext` after all finalizers.
    FinalizeDraft(DocumentFinalizeDraftFn),
}

/// Each member of core rule chain must implement this trait
pub trait CoreRule: 'static {
    const NAMES: &'static [&'static str] = &[];

    fn run(root: &mut Node, md: &MarkdownIt);

    /// Optional direct implementation. Defaults to legacy-only support; see
    /// [`DocumentCoreRule`] for stages and ordering.
    #[doc(hidden)]
    fn document_rule() -> Option<DocumentCoreRule> {
        None
    }
}

/// Both implementations share one ruler entry and the same ordering/lifetime.
#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct CoreRuleEntry {
    pub(crate) legacy: fn(&mut Node, &MarkdownIt),
    pub(crate) document: Option<DocumentCoreRule>,
}

impl CoreRuleEntry {
    pub(crate) fn new<T: CoreRule>() -> Self {
        Self {
            legacy: T::run,
            document: T::document_rule(),
        }
    }
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
