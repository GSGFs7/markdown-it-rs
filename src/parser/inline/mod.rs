//! Inline rule chain

#[doc(hidden)]
pub mod builtin;
mod rule;
mod state;

use std::collections::HashMap;
use std::sync::OnceLock;

pub use self::builtin::inline_parser::InlineRoot;
pub use self::builtin::skip_text::{Text, TextSpecial};
pub use self::rule::*;
pub(crate) use self::state::{DelimiterRun, InlineState, set_delimiter_scanner};
use crate::common::RuleMark;
use crate::common::ruler::Ruler;
use crate::parser::extset::{InlineRootExtSet, RootExtSet};
use crate::parser::inline::builtin::skip_text::TextScannerImpl;
use crate::parser::main::MarkdownIt;
use crate::parser::node::{Node, NodeEmpty};

type RuleFns = (
    fn(&mut InlineState) -> Option<usize>,
    fn(&mut InlineState) -> Option<(Node, usize)>,
);
pub(crate) type DocumentRuleFn = fn(
    &mut crate::parser::document_parser::DocumentInlineState<'_>,
) -> Option<(Option<crate::NodeDraft>, usize)>;

#[derive(Clone, Copy)]
#[doc(hidden)]
pub struct RuleEntry {
    legacy: Option<RuleFns>,
    document: Option<DocumentRuleFn>,
}

/// dispatcher
///
/// avoid scan entire plugin list when encountered any chars.
#[derive(Debug)]
struct InlineDispatch {
    ascii: Box<[Option<Vec<RuleFns>>; 128]>,
    unicode: HashMap<char, Vec<RuleFns>>,
    wildcard: Vec<RuleFns>,
}

impl InlineDispatch {
    fn compile<'a>(
        rules: impl Iterator<Item = (char, &'a RuleEntry)>,
        markers: impl Iterator<Item = char>,
    ) -> Self {
        let ordered: Vec<(char, RuleFns)> = rules
            .filter_map(|(marker, rule)| rule.legacy.map(|legacy| (marker, legacy)))
            .collect();
        let wildcard: Vec<RuleFns> = ordered
            .iter()
            .filter(|(marker, _)| *marker == '\0')
            .map(|(_, rule)| *rule)
            .collect();
        let mut ascii = Box::new(std::array::from_fn(|_| None));
        let mut unicode = HashMap::new();

        for marker in markers {
            let candidates = ordered
                .iter()
                .filter(|(rule_marker, _)| *rule_marker == '\0' || *rule_marker == marker)
                .map(|(_, rule)| *rule)
                .collect();
            if marker.is_ascii() {
                ascii[marker as usize] = Some(candidates);
            } else {
                unicode.insert(marker, candidates);
            }
        }

        Self {
            ascii,
            unicode,
            wildcard,
        }
    }

    #[inline]
    fn get(&self, marker: char) -> &[RuleFns] {
        if marker.is_ascii() {
            self.ascii[marker as usize]
                .as_deref()
                .unwrap_or(&self.wildcard)
        } else {
            self.unicode
                .get(&marker)
                .map(Vec::as_slice)
                .unwrap_or(&self.wildcard)
        }
    }
}

#[derive(Debug, Default)]
/// Inline-level tokenizer.
pub struct InlineParser {
    ruler: Ruler<RuleMark, RuleEntry>,
    text_charmap: HashMap<char, Vec<RuleMark>>,
    text_impl: OnceLock<TextScannerImpl>,
    dispatch: OnceLock<InlineDispatch>,
}

impl InlineParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn document_rules(&self) -> Option<Vec<DocumentRuleFn>> {
        self.ruler.iter().map(|entry| entry.document).collect()
    }

    pub(crate) fn is_document_marker(&self, marker: char) -> bool {
        self.text_charmap.contains_key(&marker)
    }

    pub(crate) fn has_only_text_rule(&self) -> bool {
        self.ruler.len() == 1 && self.has_rule::<builtin::TextScanner>()
    }

    #[inline]
    fn rules_for(&self, marker: char) -> &[RuleFns] {
        self.dispatch
            .get_or_init(|| {
                InlineDispatch::compile(
                    self.ruler
                        .iter_with_marks()
                        .map(|(mark, rule)| (self.marker_for(mark), rule)),
                    self.text_charmap.keys().copied(),
                )
            })
            .get(marker)
    }

    fn marker_for(&self, mark: &RuleMark) -> char {
        self.text_charmap
            .iter()
            .find_map(|(marker, marks)| marks.contains(mark).then_some(*marker))
            .unwrap_or('\0')
    }

    /// Skip single token by running all rules in validation mode;
    /// returns `true` if any rule reported success
    ///
    pub fn skip_token(&self, state: &mut InlineState) {
        stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
            let mut ok = None;

            if state.level < state.md.max_nesting {
                let marker = state.src[state.pos..state.pos_max].chars().next().unwrap();
                for rule in self.rules_for(marker) {
                    ok = (rule.0)(state);
                    if ok.is_some() {
                        break;
                    }
                }
            } else {
                // Too much nesting, just skip until the end of the paragraph.
                //
                // NOTE: this will cause links to behave incorrectly in the following case,
                //       when an amount of `[` is exactly equal to `maxNesting + 1`:
                //
                //       [[[[[[[[[[[[[[[[[[[[[foo]()
                //
                // TODO: remove this workaround when CM standard will allow nested links
                //       (we can replace it by preventing links from being parsed in
                //       validation mode)
                //
                state.pos = state.pos_max;
            }

            if let Some(len) = ok {
                state.pos += len;
            } else {
                let ch = state.src[state.pos..state.pos_max].chars().next().unwrap();
                state.pos += ch.len_utf8();
            }
        });
    }

    /// Generate tokens for input range
    ///
    pub fn tokenize(&self, state: &mut InlineState) {
        stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
            let end = state.pos_max;

            while state.pos < end {
                // Try all possible rules.
                // On success, rule should:
                //
                // - update `state.pos`
                // - update `state.tokens`
                // - return true
                let mut ok = None;

                if state.level < state.md.max_nesting {
                    let marker = state.src[state.pos..state.pos_max].chars().next().unwrap();
                    for rule in self.rules_for(marker) {
                        ok = (rule.1)(state);
                        if ok.is_some() {
                            break;
                        }
                    }
                }

                if let Some((mut node, len)) = ok {
                    state.pos += len;
                    if !node.is::<NodeEmpty>() {
                        node.srcmap = state.get_map(state.pos - len, state.pos);
                        state.node.children.push(node);
                        if state.pos >= end {
                            break;
                        }
                    }
                    continue;
                }

                let ch = state.src[state.pos..state.pos_max].chars().next().unwrap();
                let len = ch.len_utf8();
                state.trailing_text_push(state.pos, state.pos + len);
                state.pos += len;
            }
        });
    }

    /// Process input string and push inline tokens into `out_tokens`
    ///
    pub fn parse(
        &self,
        src: String,
        srcmap: Vec<(usize, usize)>,
        node: Node,
        md: &MarkdownIt,
        root_ext: &mut RootExtSet,
        inline_ext: &mut InlineRootExtSet,
    ) -> Node {
        let mut state = InlineState::new(src, srcmap, md, root_ext, inline_ext, node);
        self.tokenize(&mut state);
        state.node
    }

    fn add_rule_entry<T: 'static>(
        &mut self,
        marker: char,
        names: &'static [&'static str],
        legacy: Option<RuleFns>,
        document: Option<DocumentRuleFn>,
    ) -> &mut crate::common::ruler::RuleItem<RuleMark, RuleEntry> {
        self.dispatch = OnceLock::new();
        if marker != '\0' {
            self.text_impl = OnceLock::new();
            let charvec = self.text_charmap.entry(marker).or_default();
            charvec.push(RuleMark::of::<T>());
        }

        let item = self
            .ruler
            .add(RuleMark::of::<T>(), RuleEntry { legacy, document });
        for name in names {
            item.alias(RuleMark::named(*name));
        }
        item
    }

    /// Register an arena-backed rule used by [`MarkdownIt::parse_document_direct`].
    ///
    /// The legacy [`MarkdownIt::parse`] path does not execute rules registered through
    /// this API.
    pub fn add_rule<T: InlineRule>(&mut self) -> RuleBuilder<'_, RuleEntry> {
        let item =
            self.add_rule_entry::<T>(T::MARKER, T::NAMES, None, Some(<T as InlineRule>::run));
        RuleBuilder::new(item)
    }

    pub(crate) fn add_legacy_rule<T: LegacyInlineRule>(
        &mut self,
    ) -> LegacyRuleBuilder<'_, RuleEntry> {
        let item = self.add_rule_entry::<T>(T::MARKER, T::NAMES, Some((T::check, T::run)), None);
        LegacyRuleBuilder::new(item)
    }

    pub(crate) fn add_migrated_rule<T: InlineRule + LegacyInlineRule>(
        &mut self,
    ) -> RuleBuilder<'_, RuleEntry> {
        debug_assert_eq!(<T as InlineRule>::MARKER, <T as LegacyInlineRule>::MARKER);
        debug_assert_eq!(<T as InlineRule>::NAMES, <T as LegacyInlineRule>::NAMES);
        let item = self.add_rule_entry::<T>(
            <T as InlineRule>::MARKER,
            <T as InlineRule>::NAMES,
            Some((<T as LegacyInlineRule>::check, <T as LegacyInlineRule>::run)),
            Some(<T as InlineRule>::run),
        );
        RuleBuilder::new(item)
    }

    pub fn has_rule<T: InlineRule>(&self) -> bool {
        self.ruler.contains(RuleMark::of::<T>())
    }

    pub fn remove_rule<T: InlineRule>(&mut self) {
        self.remove_rule_entry::<T>(T::MARKER);
    }

    pub(crate) fn has_legacy_rule<T: LegacyInlineRule>(&self) -> bool {
        self.ruler.contains(RuleMark::of::<T>())
    }

    #[cfg(test)]
    pub(crate) fn remove_legacy_rule<T: LegacyInlineRule>(&mut self) {
        self.remove_rule_entry::<T>(T::MARKER);
    }

    fn remove_rule_entry<T: 'static>(&mut self, marker: char) {
        self.dispatch = OnceLock::new();
        if marker != '\0' {
            self.text_impl = OnceLock::new();
            let mut charvec = self.text_charmap.remove(&marker).unwrap_or_default();
            charvec.retain(|x| *x != RuleMark::of::<T>());
            if !charvec.is_empty() {
                self.text_charmap.insert(marker, charvec);
            }
        }

        self.ruler.remove(RuleMark::of::<T>());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AtRule;
    struct HashRule;
    struct SnowRule;
    struct WildcardRule;
    struct DirectAtRule;
    struct DirectHashRule;

    macro_rules! empty_rule {
        ($rule:ty, $marker:expr) => {
            impl LegacyInlineRule for $rule {
                const MARKER: char = $marker;

                fn run(_: &mut InlineState) -> Option<(Node, usize)> {
                    None
                }
            }
        };
    }

    empty_rule!(AtRule, '@');
    empty_rule!(HashRule, '#');
    empty_rule!(SnowRule, '雪');
    empty_rule!(WildcardRule, '\0');
    empty_rule!(DirectAtRule, '@');
    empty_rule!(DirectHashRule, '#');

    impl InlineRule for DirectAtRule {
        const MARKER: char = '@';

        fn run(
            _: &mut crate::parser::document_parser::DocumentInlineState<'_>,
        ) -> Option<(Option<crate::NodeDraft>, usize)> {
            None
        }
    }

    impl InlineRule for DirectHashRule {
        const MARKER: char = '#';

        fn run(
            _: &mut crate::parser::document_parser::DocumentInlineState<'_>,
        ) -> Option<(Option<crate::NodeDraft>, usize)> {
            None
        }
    }

    fn check_id<T: LegacyInlineRule>() -> usize {
        T::check as fn(&mut InlineState) -> Option<usize> as usize
    }

    fn check_ids(rules: &[RuleFns]) -> Vec<usize> {
        rules.iter().map(|rule| rule.0 as usize).collect()
    }

    fn document_id<T: InlineRule>() -> usize {
        <T as InlineRule>::run as DocumentRuleFn as usize
    }

    #[test]
    fn dispatch_filters_rules_without_changing_order() {
        let mut parser = InlineParser::new();
        parser.add_legacy_rule::<HashRule>();
        parser.add_legacy_rule::<AtRule>().alias_named("at");
        parser.add_legacy_rule::<WildcardRule>().after::<AtRule>();

        assert_eq!(
            check_ids(parser.rules_for('@')),
            vec![check_id::<AtRule>(), check_id::<WildcardRule>()]
        );
        assert_eq!(
            check_ids(parser.rules_for('#')),
            vec![check_id::<HashRule>(), check_id::<WildcardRule>()]
        );
        assert_eq!(
            check_ids(parser.rules_for('!')),
            vec![check_id::<WildcardRule>()]
        );
    }

    #[test]
    fn dispatch_supports_unicode_and_invalidates_on_changes() {
        let mut parser = InlineParser::new();
        parser.add_legacy_rule::<AtRule>();
        parser.add_legacy_rule::<WildcardRule>().after_all();

        // Compile the first dispatch table before mutating the ruler.
        assert_eq!(
            check_ids(parser.rules_for('雪')),
            vec![check_id::<WildcardRule>()]
        );

        parser
            .add_legacy_rule::<SnowRule>()
            .before::<WildcardRule>();
        assert_eq!(
            check_ids(parser.rules_for('雪')),
            vec![check_id::<SnowRule>(), check_id::<WildcardRule>()]
        );

        parser.remove_legacy_rule::<AtRule>();
        assert_eq!(
            check_ids(parser.rules_for('@')),
            vec![check_id::<WildcardRule>()]
        );
    }

    #[test]
    fn document_callbacks_share_rule_order_and_lifecycle() {
        let mut parser = InlineParser::new();
        parser.add_migrated_rule::<DirectAtRule>();
        parser
            .add_migrated_rule::<DirectHashRule>()
            .before::<DirectAtRule>();

        let rules = parser.document_rules().unwrap();
        assert_eq!(
            rules.iter().map(|rule| *rule as usize).collect::<Vec<_>>(),
            vec![
                document_id::<DirectHashRule>(),
                document_id::<DirectAtRule>()
            ]
        );

        parser.add_legacy_rule::<SnowRule>();
        assert!(parser.document_rules().is_none());
        parser.remove_legacy_rule::<SnowRule>();
        assert_eq!(parser.document_rules().unwrap().len(), 2);
    }
}
