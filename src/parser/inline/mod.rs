//! Inline syntax rules and check sessions.
mod document;
pub mod helpers;
mod parser;
mod rule;
mod state;
mod text_scanner;

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

pub(crate) use document::parse_document;

pub use self::parser::InlineParserRule;
pub use self::rule::*;
pub(crate) use self::state::InlinePostprocessRanges;
pub use self::state::InlineState;
pub use self::text_scanner::TextScanner;
use self::text_scanner::TextScannerImpl;
use crate::common::RuleMark;
use crate::common::ruler::Ruler;

/// Register the core rule and text scanner that drive inline parsing.
pub fn add(md: &mut crate::MarkdownIt) {
    text_scanner::add(md);
    parser::add(md);
}

pub(crate) type InlineCheckFn = fn(&mut InlineState<'_>) -> Option<usize>;
// TODO: some nested some??
pub(crate) type InlineRunFn =
    fn(&mut crate::InlineState<'_>) -> Option<(Option<crate::NodeId>, usize)>;

// functions that cause heap allocation
type DynamicCheckFn = dyn Fn(&mut InlineState<'_>) -> Option<usize> + Send + Sync;
type DynamicRunFn =
    dyn Fn(&mut InlineState<'_>) -> Option<(Option<crate::NodeId>, usize)> + Send + Sync;

#[derive(Clone)]
pub(crate) enum InlineCheckRunner {
    Static(InlineCheckFn),
    Dynamic(Arc<DynamicCheckFn>),
}

impl InlineCheckRunner {
    #[inline]
    pub(crate) fn call(&self, state: &mut InlineState<'_>) -> Option<usize> {
        match self {
            Self::Static(check) => check(state),
            Self::Dynamic(check) => check(state),
        }
    }
}

impl std::fmt::Debug for InlineCheckRunner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Static(_) => "Static",
            Self::Dynamic(_) => "Dynamic",
        })
    }
}

#[derive(Clone)]
pub(crate) enum InlineRunRunner {
    Static(InlineRunFn),
    Dynamic(Arc<DynamicRunFn>),
}

impl std::fmt::Debug for InlineRunRunner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Static(_) => "Static",
            Self::Dynamic(_) => "Dynamic",
        })
    }
}

struct DynamicRuleTag;

#[derive(Debug, Clone)]
pub(crate) struct InlineRuleFns {
    pub(crate) run: InlineRunRunner,
    pub(crate) check: InlineCheckRunner,
    pub(crate) marker: char,
    pub(crate) type_id: std::any::TypeId,
}

impl InlineRuleFns {
    /// Whether this rule can be considered for the given current character.
    #[inline]
    pub(crate) fn matches_marker(&self, ch: char) -> bool {
        self.marker == '\0' || self.marker == ch
    }
}

fn inline_rule_fns<T: InlineRule>() -> InlineRuleFns {
    InlineRuleFns {
        run: InlineRunRunner::Static(T::run),
        check: InlineCheckRunner::Static(T::check),
        marker: T::MARKER,
        type_id: std::any::TypeId::of::<T>(),
    }
}

#[derive(Clone)]
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
            let entries: Vec<_> = self.ruler.iter().map(|entry| entry.fns.clone()).collect();
            RuleSet {
                runs: entries.clone(),
                checks: entries,
            }
        })
    }

    pub(crate) fn has_only_text_rule(&self) -> bool {
        self.ruler.len() == 1 && self.has_rule::<TextScanner>()
    }

    fn add_rule_entry<T: InlineRule>(
        &mut self,
    ) -> &mut crate::common::ruler::RuleItem<RuleMark, RuleEntry> {
        self.rules.take();
        if std::any::TypeId::of::<T>() != std::any::TypeId::of::<TextScanner>() {
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
        if std::any::TypeId::of::<T>() != std::any::TypeId::of::<TextScanner>() {
            self.text_impl = OnceLock::new();
            let mut marks = self.text_charmap.remove(&T::MARKER).unwrap_or_default();
            marks.retain(|mark| *mark != RuleMark::of::<T>());
            if !marks.is_empty() {
                self.text_charmap.insert(T::MARKER, marks);
            }
        }
        self.ruler.remove(RuleMark::of::<T>());
    }

    /// Register a runtime inline rule produced by the high-level plugin API.
    pub(crate) fn add_dynamic_rule(
        &mut self,
        mark: RuleMark,
        marker: char,
        run: Arc<DynamicRunFn>,
        check: Arc<DynamicCheckFn>,
    ) -> RuleBuilder<'_, RuleEntry> {
        self.rules.take();
        self.text_impl = OnceLock::new();

        self.text_charmap
            .entry(marker)
            .or_default()
            .push(mark.clone());

        let item = self.ruler.add(
            mark,
            RuleEntry {
                fns: InlineRuleFns {
                    run: InlineRunRunner::Dynamic(run),
                    check: InlineCheckRunner::Dynamic(check),
                    marker,
                    type_id: std::any::TypeId::of::<DynamicRuleTag>(),
                },
            },
        );
        RuleBuilder::new(item)
    }

    // Keep short text scans inside the calling rule's hot path.
    #[inline]
    pub(crate) fn text_length(&self, source: &str, pos: usize, pos_max: usize) -> usize {
        self.text_impl
            .get_or_init(|| TextScannerImpl::compile(self.text_charmap.keys().copied().collect()))
            .find(&source[pos..pos_max])
    }
}
