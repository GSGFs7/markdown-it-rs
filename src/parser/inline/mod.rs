//! Inline syntax rules and check sessions.
#[doc(hidden)]
pub mod builtin;
pub mod helpers;
mod rule;
mod state;

use std::collections::HashMap;
use std::sync::OnceLock;

use self::builtin::skip_text::TextScannerImpl;
pub use self::rule::*;
pub use self::state::DocumentInlineState;
use crate::common::RuleMark;
use crate::common::ruler::Ruler;

pub(crate) type InlineCheckFn = fn(&mut DocumentInlineState<'_>) -> Option<usize>;
pub(crate) type InlineRuleFn =
    fn(&mut crate::DocumentInlineState<'_>) -> Option<(Option<crate::NodeDraft>, usize)>;

#[derive(Debug, Clone, Copy)]
pub(crate) struct InlineRuleFns {
    pub(crate) run: InlineRuleFn,
    pub(crate) check: InlineCheckFn,
    pub(crate) marker: char,
    pub(crate) type_id: std::any::TypeId,
}

impl InlineRuleFns {
    /// Whether this rule can be considered for the given current character.
    #[inline]
    pub(crate) fn matches_marker(self, ch: char) -> bool {
        self.marker == '\0' || self.marker == ch
    }
}

fn inline_rule_fns<T: InlineRule>() -> InlineRuleFns {
    InlineRuleFns {
        run: T::run,
        check: T::check,
        marker: T::MARKER,
        type_id: std::any::TypeId::of::<T>(),
    }
}

#[derive(Clone, Copy)]
#[doc(hidden)]
pub struct RuleEntry {
    document: InlineRuleFns,
    document_finalize: Option<DocumentFinalizeFn>,
}

pub(crate) struct DocumentRuleSet {
    pub(crate) runs: Vec<InlineRuleFns>,
    pub(crate) checks: Vec<InlineRuleFns>,
    pub(crate) finalizers: Vec<DocumentFinalizeFn>,
}

/// Inline-level tokenizer.
#[derive(Debug, Default)]
pub struct InlineParser {
    ruler: Ruler<RuleMark, RuleEntry>,
    text_charmap: HashMap<char, Vec<RuleMark>>,
    text_impl: OnceLock<TextScannerImpl>,
}

impl InlineParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn document_rules(&self) -> DocumentRuleSet {
        let entries: Vec<_> = self.ruler.iter().map(|entry| entry.document).collect();
        DocumentRuleSet {
            runs: entries.clone(),
            checks: entries,
            finalizers: self.document_finalizers(),
        }
    }

    pub(crate) fn has_only_text_rule(&self) -> bool {
        self.ruler.len() == 1 && self.has_rule::<builtin::TextScanner>()
    }

    fn add_rule_entry<T: InlineRule>(
        &mut self,
        finalize: Option<DocumentFinalizeFn>,
    ) -> &mut crate::common::ruler::RuleItem<RuleMark, RuleEntry> {
        if T::MARKER != '\0' {
            self.text_impl = OnceLock::new();
            self.text_charmap
                .entry(T::MARKER)
                .or_default()
                .push(RuleMark::of::<T>());
        }
        let item = self.ruler.add(
            RuleMark::of::<T>(),
            RuleEntry {
                document: inline_rule_fns::<T>(),
                document_finalize: finalize,
            },
        );
        for name in T::NAMES {
            item.alias(RuleMark::named(*name));
        }
        item
    }

    /// Register an inline syntax rule.
    pub fn add_rule<T: InlineRule>(&mut self) -> RuleBuilder<'_, RuleEntry> {
        RuleBuilder::new(self.add_rule_entry::<T>(None))
    }

    /// Register a rule and a callback that resolves its deferred drafts.
    /// Shared callbacks execute once, in rule order.
    pub fn add_rule_with_finalize<T: InlineRule>(
        &mut self,
        finalize: DocumentFinalizeFn,
    ) -> RuleBuilder<'_, RuleEntry> {
        RuleBuilder::new(self.add_rule_entry::<T>(Some(finalize)))
    }

    pub fn has_rule<T: InlineRule>(&self) -> bool {
        self.ruler.contains(RuleMark::of::<T>())
    }

    pub fn remove_rule<T: InlineRule>(&mut self) {
        if T::MARKER != '\0' {
            self.text_impl = OnceLock::new();
            let mut marks = self.text_charmap.remove(&T::MARKER).unwrap_or_default();
            marks.retain(|mark| *mark != RuleMark::of::<T>());
            if !marks.is_empty() {
                self.text_charmap.insert(T::MARKER, marks);
            }
        }
        self.ruler.remove(RuleMark::of::<T>());
    }

    fn document_finalizers(&self) -> Vec<DocumentFinalizeFn> {
        let mut result: Vec<DocumentFinalizeFn> = Vec::new();
        for entry in self.ruler.iter() {
            if let Some(finalize) = entry.document_finalize {
                if !result
                    .iter()
                    .any(|existing| *existing as usize == finalize as usize)
                {
                    result.push(finalize);
                }
            }
        }
        result
    }

    pub(crate) fn text_length(&self, source: &str, pos: usize, pos_max: usize) -> usize {
        self.text_impl
            .get_or_init(|| TextScannerImpl::compile(self.text_charmap.keys().copied().collect()))
            .find(&source[pos..pos_max])
    }
}
