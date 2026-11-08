//! Inline syntax rules and check sessions.
mod document;
pub mod helpers;
mod mapping;
mod parser;
mod rule;
mod state;
mod text_scanner;
#[cfg(debug_assertions)]
mod validation;

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

pub(crate) use self::document::parse_document;
pub use self::mapping::InlineMapping;
pub use self::parser::InlineParserRule;
pub use self::rule::*;
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

/// A plugin hook for source-wide scan side effects.
///
/// Returns true when `[start..end)` must be checked even if the label may not
/// close, so the hook's side effects are preserved.
pub(crate) type ScanHook = fn(&InlineState<'_>, usize, usize) -> bool;

/// Returns false to skip a scan hook for the entire inline source.
pub(crate) type SourceFilter = fn(&str) -> bool;

#[derive(Debug, Clone, Copy)]
struct ScanHookEntry {
    hook: ScanHook,
    source_filter: Option<SourceFilter>,
}

#[derive(Debug, Default)]
pub(crate) struct ScanHooks(Vec<ScanHookEntry>);

impl ScanHooks {
    pub(crate) fn register(&mut self, hook: ScanHook, source_filter: Option<SourceFilter>) {
        self.0.push(ScanHookEntry {
            hook,
            source_filter,
        });
    }

    fn select(&self, source: &str) -> Vec<ScanHook> {
        self.0
            .iter()
            .filter(|entry| entry.source_filter.is_none_or(|filter| filter(source)))
            .map(|entry| entry.hook)
            .collect()
    }
}

/// Register a hook that runs on every probe.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn add_scan_hook(md: &mut crate::MarkdownIt, hook: ScanHook) {
    md.ext
        .get_or_insert_default::<ScanHooks>()
        .register(hook, None);
}

/// Register a hook gated by a pure source predicate.
///
/// The filter runs lazily once per source and is shared by nested sessions.
pub(crate) fn add_filtered_scan_hook(
    md: &mut crate::MarkdownIt,
    hook: ScanHook,
    source_filter: SourceFilter,
) {
    md.ext
        .get_or_insert_default::<ScanHooks>()
        .register(hook, Some(source_filter));
}

/// Select the hooks that may apply to a full inline source.
pub(crate) fn select_scan_hooks(md: &crate::MarkdownIt, source: &str) -> Vec<ScanHook> {
    md.ext
        .get::<ScanHooks>()
        .map_or_else(Vec::new, |hooks| hooks.select(source))
}

/// Whether any registered hook still needs `[start..end)` checked.
pub(crate) fn scan_hooks_require_scan(state: &InlineState<'_>, start: usize, end: usize) -> bool {
    state
        .scan_hooks()
        .iter()
        .any(|&hook| hook(state, start, end))
}

pub(crate) type InlineCheckFn = fn(&mut InlineState<'_>) -> Option<usize>;
// TODO: some nested some??
pub(crate) type InlineRunFn =
    fn(&mut InlineState<'_>) -> Option<(Option<crate::document::NodeId>, usize)>;

// functions that cause heap allocation
type DynamicCheckFn = dyn Fn(&mut InlineState<'_>) -> Option<usize> + Send + Sync;
type DynamicRunFn =
    dyn Fn(&mut InlineState<'_>) -> Option<(Option<crate::document::NodeId>, usize)> + Send + Sync;

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

#[cfg(test)]
mod scan_hook_tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::document::{Document, Root};

    static FILTERS: AtomicUsize = AtomicUsize::new(0);

    #[derive(Debug, Default)]
    struct Calls(Vec<&'static str>);

    #[derive(Debug, Default)]
    struct Disabled(bool);

    fn source_filter(source: &str) -> bool {
        FILTERS.fetch_add(1, Ordering::Relaxed);
        source.contains("<!")
    }

    fn first(state: &InlineState<'_>, _: usize, _: usize) -> bool {
        state
            .shared_ext()
            .get_or_insert_default::<Calls>()
            .0
            .push("first");
        false
    }

    fn second(state: &InlineState<'_>, _: usize, _: usize) -> bool {
        let mut shared = state.shared_ext();
        shared.get_or_insert_default::<Calls>().0.push("second");
        !shared.get_or_insert_default::<Disabled>().0
    }

    #[test]
    fn hooks_registered_after_rule_compilation_are_used() {
        let mut md = crate::MarkdownIt::empty();
        let _ = md.inline.rules();
        add_scan_hook(&mut md, |_, _, _| true);
        let mut document = Document::new("", Root::new(""));
        let state = InlineState::new(&mut document, "x", 0, 1, &md, md.inline.rules(), 0, 0);
        assert!(scan_hooks_require_scan(&state, 0, 1));
    }

    #[test]
    fn filters_are_lazy_and_shared_while_hooks_stay_live() {
        FILTERS.store(0, Ordering::Relaxed);

        let mut md = crate::MarkdownIt::empty();
        add_scan_hook(&mut md, first);
        add_filtered_scan_hook(&mut md, second, source_filter);
        let mut document = Document::new("", Root::new(""));
        let rules = md.inline.rules();
        let source = "ab<!x";
        let mut parent = InlineState::new(&mut document, source, 0, source.len(), &md, rules, 0, 0);
        assert_eq!(FILTERS.load(Ordering::Relaxed), 0);
        // Selection uses the full shared source, not the child window.
        {
            let child = parent.child_state(0..2).unwrap();
            assert!(scan_hooks_require_scan(&child, 0, 2));
            child.shared_ext().get_or_insert_default::<Disabled>().0 = true;
            assert!(!scan_hooks_require_scan(&child, 0, 2));
        }
        assert!(!scan_hooks_require_scan(&parent, 0, source.len()));
        assert_eq!(FILTERS.load(Ordering::Relaxed), 1);
        assert_eq!(
            parent.shared_ext().get::<Calls>().unwrap().0,
            ["first", "second", "first", "second", "first", "second"]
        );

        let other = InlineState::new(&mut document, "plain", 0, 5, &md, rules, 0, 0);
        assert!(!scan_hooks_require_scan(&other, 0, 5));
        assert!(!scan_hooks_require_scan(&other, 0, 5));
        assert_eq!(FILTERS.load(Ordering::Relaxed), 2);
        assert_eq!(
            other.shared_ext().get::<Calls>().unwrap().0,
            ["first", "first"]
        );
    }
}
