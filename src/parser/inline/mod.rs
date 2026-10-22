//! Inline syntax rules and check sessions.
#[doc(hidden)]
pub mod builtin;
mod document;
pub mod helpers;
mod rule;
mod state;

use std::collections::HashMap;
use std::sync::OnceLock;

pub(crate) use document::parse_document;

use self::builtin::skip_text::TextScannerImpl;
pub use self::rule::*;
pub(crate) use self::state::InlinePostprocessRanges;
pub use self::state::InlineState;
use crate::common::RuleMark;
use crate::common::ruler::Ruler;

pub(crate) type InlineCheckFn = fn(&mut InlineState<'_>) -> Option<usize>;
// TODO: some nested some??
pub(crate) type InlineRuleFn =
    fn(&mut crate::InlineState<'_>) -> Option<(Option<crate::NodeId>, usize)>;

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
    fns: InlineRuleFns,
}

#[derive(Debug)]
pub(crate) struct RuleSet {
    pub(crate) runs: Vec<InlineRuleFns>,
    pub(crate) checks: Vec<InlineRuleFns>,
}

/// Inline-level tokenizer.
#[derive(Debug, Default)]
pub struct InlineParser {
    ruler: Ruler<RuleMark, RuleEntry>,
    text_charmap: HashMap<char, Vec<RuleMark>>,
    text_impl: OnceLock<TextScannerImpl>,
    rules: OnceLock<RuleSet>,
}

impl InlineParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn rules(&self) -> &RuleSet {
        self.rules.get_or_init(|| {
            let entries: Vec<_> = self.ruler.iter().map(|entry| entry.fns).collect();
            RuleSet {
                runs: entries.clone(),
                checks: entries,
            }
        })
    }

    pub(crate) fn has_only_text_rule(&self) -> bool {
        self.ruler.len() == 1 && self.has_rule::<builtin::TextScanner>()
    }

    fn add_rule_entry<T: InlineRule>(
        &mut self,
    ) -> &mut crate::common::ruler::RuleItem<RuleMark, RuleEntry> {
        self.rules.take();
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
                fns: inline_rule_fns::<T>(),
            },
        );
        for name in T::NAMES {
            item.alias(RuleMark::named(*name));
        }
        item
    }

    /// Register an inline syntax rule.
    pub fn add_rule<T: InlineRule>(&mut self) -> RuleBuilder<'_, RuleEntry> {
        RuleBuilder::new(self.add_rule_entry::<T>())
    }

    pub fn has_rule<T: InlineRule>(&self) -> bool {
        self.ruler.contains(RuleMark::of::<T>())
    }

    pub fn remove_rule<T: InlineRule>(&mut self) {
        self.rules.take();
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

    pub(crate) fn text_length(&self, source: &str, pos: usize, pos_max: usize) -> usize {
        self.text_impl
            .get_or_init(|| TextScannerImpl::compile(self.text_charmap.keys().copied().collect()))
            .find(&source[pos..pos_max])
    }
}
