//! Inline syntax rules and probe sessions.
#[doc(hidden)]
pub mod builtin;
pub mod probe;
mod rule;
mod state;

use std::collections::HashMap;
use std::sync::OnceLock;

use self::builtin::skip_text::TextScannerImpl;
pub use self::builtin::skip_text::{Text, TextSpecial};
pub use self::probe::*;
pub use self::rule::*;
pub(crate) use self::state::{DelimiterRun, scan_delimiter_run, set_delimiter_scanner};
use crate::common::RuleMark;
use crate::common::ruler::Ruler;

pub(crate) type DocumentProbeFn = fn(&mut InlineProbeContext<'_>) -> InlineProbeResult;
pub(crate) type DocumentRuleFn =
    fn(&mut crate::DocumentInlineState<'_>) -> Option<(Option<crate::NodeDraft>, usize)>;

#[derive(Debug, Clone, Copy)]
pub(crate) struct DocumentRuleFns {
    pub(crate) run: DocumentRuleFn,
    pub(crate) probe: DocumentProbeFn,
    pub(crate) marker: char,
    pub(crate) type_id: std::any::TypeId,
}

impl DocumentRuleFns {
    /// Whether this rule can be considered for the given current character.
    #[inline]
    pub(crate) fn matches_marker(self, ch: char) -> bool {
        self.marker == '\0' || self.marker == ch
    }
}

fn document_rule_fns<T: InlineRule>() -> DocumentRuleFns {
    DocumentRuleFns {
        run: T::run,
        probe: T::probe,
        marker: T::MARKER,
        type_id: std::any::TypeId::of::<T>(),
    }
}

#[derive(Clone, Copy)]
#[doc(hidden)]
pub struct RuleEntry {
    document: DocumentRuleFns,
    document_finalize: Option<DocumentFinalizeFn>,
}

pub(crate) struct DocumentRuleSet {
    pub(crate) runs: Vec<DocumentRuleFns>,
    pub(crate) probes: Vec<DocumentRuleFns>,
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
            probes: entries,
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
                document: document_rule_fns::<T>(),
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
