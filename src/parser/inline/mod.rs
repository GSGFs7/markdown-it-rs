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
pub use self::state::*;
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
        rules: impl Iterator<Item = (char, &'a RuleFns)>,
        markers: impl Iterator<Item = char>,
    ) -> Self {
        let ordered: Vec<(char, RuleFns)> = rules.map(|(marker, rule)| (marker, *rule)).collect();
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
    ruler: Ruler<RuleMark, RuleFns>,
    text_charmap: HashMap<char, Vec<RuleMark>>,
    text_impl: OnceLock<TextScannerImpl>,
    dispatch: OnceLock<InlineDispatch>,
}

impl InlineParser {
    pub fn new() -> Self {
        Self::default()
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

    pub fn add_rule<T: InlineRule>(&mut self) -> RuleBuilder<'_, RuleFns> {
        self.dispatch = OnceLock::new();
        if T::MARKER != '\0' {
            self.text_impl = OnceLock::new();
            let charvec = self.text_charmap.entry(T::MARKER).or_default();
            charvec.push(RuleMark::of::<T>());
        }

        let item = self.ruler.add(RuleMark::of::<T>(), (T::check, T::run));
        for name in T::NAMES {
            item.alias(RuleMark::named(*name));
        }
        RuleBuilder::new(item)
    }

    pub fn has_rule<T: InlineRule>(&self) -> bool {
        self.ruler.contains(RuleMark::of::<T>())
    }

    pub fn remove_rule<T: InlineRule>(&mut self) {
        self.dispatch = OnceLock::new();
        if T::MARKER != '\0' {
            self.text_impl = OnceLock::new();
            let mut charvec = self.text_charmap.remove(&T::MARKER).unwrap_or_default();
            charvec.retain(|x| *x != RuleMark::of::<T>());
            if !charvec.is_empty() {
                self.text_charmap.insert(T::MARKER, charvec);
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

    macro_rules! empty_rule {
        ($rule:ty, $marker:expr) => {
            impl InlineRule for $rule {
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

    fn check_id<T: InlineRule>() -> usize {
        T::check as fn(&mut InlineState) -> Option<usize> as usize
    }

    fn check_ids(rules: &[RuleFns]) -> Vec<usize> {
        rules.iter().map(|rule| rule.0 as usize).collect()
    }

    #[test]
    fn dispatch_filters_rules_without_changing_order() {
        let mut parser = InlineParser::new();
        parser.add_rule::<HashRule>();
        parser.add_rule::<AtRule>().alias_named("at");
        parser.add_rule::<WildcardRule>().after::<AtRule>();

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
        parser.add_rule::<AtRule>();
        parser.add_rule::<WildcardRule>().after_all();

        // Compile the first dispatch table before mutating the ruler.
        assert_eq!(
            check_ids(parser.rules_for('雪')),
            vec![check_id::<WildcardRule>()]
        );

        parser.add_rule::<SnowRule>().before::<WildcardRule>();
        assert_eq!(
            check_ids(parser.rules_for('雪')),
            vec![check_id::<SnowRule>(), check_id::<WildcardRule>()]
        );

        parser.remove_rule::<AtRule>();
        assert_eq!(
            check_ids(parser.rules_for('@')),
            vec![check_id::<WildcardRule>()]
        );
    }
}
