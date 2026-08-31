//! Block rule chain

#[doc(hidden)]
pub mod builtin;
mod rule;
mod state;

pub use self::rule::*;
pub use self::state::*;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use crate::common::RuleMark;
use crate::common::ruler::Ruler;
use crate::parser::extset::RootExtSet;
use crate::parser::inline::InlineRoot;
use crate::parser::node::NodeEmpty;
use crate::{MarkdownIt, Node};

type RuleFns = (
    fn(&mut BlockState) -> Option<()>,
    fn(&mut BlockState) -> Option<(Node, usize)>,
);
type RuleEntry = (&'static [char], RuleFns);

#[derive(Debug)]
struct BlockDispatch {
    ascii: Box<[Option<Vec<RuleFns>>; 128]>,
    unicode: HashMap<char, Vec<RuleFns>>,
    wildcard: Vec<RuleFns>,
}

impl BlockDispatch {
    fn compile<'a>(rules: impl Iterator<Item = &'a RuleEntry>) -> Self {
        let ordered: Vec<RuleEntry> = rules.copied().collect();
        let wildcard: Vec<RuleFns> = ordered
            .iter()
            .filter(|rule| rule.0.is_empty())
            .map(|rule| rule.1)
            .collect();
        let markers: HashSet<char> = ordered
            .iter()
            .flat_map(|rule| rule.0.iter().copied())
            .collect();
        let mut ascii = Box::new(std::array::from_fn(|_| None));
        let mut unicode = HashMap::new();

        for marker in markers {
            let candidates = ordered
                .iter()
                .filter(|rule| rule.0.is_empty() || rule.0.contains(&marker))
                .map(|rule| rule.1)
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

    #[inline]
    fn get_at(&self, src: &str, pos: usize) -> &[RuleFns] {
        let first = src.as_bytes()[pos];
        if first.is_ascii() {
            self.ascii[first as usize]
                .as_deref()
                .unwrap_or(&self.wildcard)
        } else {
            // Multi-byte UTF-8 lead byte: decode the full char for the
            // unicode marker table. `pos` is a char boundary and the caller
            // guarantees `pos < line_end <= len`, so this can't be empty.
            let marker = src[pos..].chars().next().unwrap_or('\0');
            self.get(marker)
        }
    }
}

#[derive(Debug, Default)]
/// Block-level tokenizer.
pub struct BlockParser {
    ruler: Ruler<RuleMark, RuleEntry>,
    dispatch: OnceLock<BlockDispatch>,
}

impl BlockParser {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    fn rules_for(&self, marker: char) -> &[RuleFns] {
        self.dispatch
            .get_or_init(|| BlockDispatch::compile(self.ruler.iter()))
            .get(marker)
    }

    #[inline]
    fn rules_for_line(&self, state: &BlockState) -> &[RuleFns] {
        let offsets = &state.line_offsets[state.line];
        if offsets.first_nonspace >= offsets.line_end {
            // Empty line — no block rule can match it.
            return &[];
        }
        let dispatch = self
            .dispatch
            .get_or_init(|| BlockDispatch::compile(self.ruler.iter()));
        dispatch.get_at(state.src, offsets.first_nonspace)
    }

    /// Generate tokens for input range
    ///
    fn tokenize(&self, state: &mut BlockState) {
        stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
            let mut has_empty_lines = false;

            while state.line < state.line_max {
                state.line = state.skip_empty_lines(state.line);
                if state.line >= state.line_max {
                    break;
                }

                // Termination condition for nested calls.
                // Nested calls currently used for blockquotes & lists
                if state.line_indent(state.line) < 0 {
                    break;
                }

                // If nesting level exceeded - skip tail to the end. That's not ordinary
                // situation and we should not care about content.
                if state.level >= state.md.max_nesting {
                    state.line = state.line_max;
                    break;
                }

                // Try all possible rules.
                // On success, rule should:
                //
                // - update `state.line`
                // - update `state.tokens`
                // - return true
                let mut ok = None;

                for rule in self.rules_for_line(state) {
                    ok = (rule.1)(state);
                    if ok.is_some() {
                        break;
                    }
                }

                if let Some((mut node, len)) = ok {
                    state.line += len;
                    if !node.is::<NodeEmpty>() {
                        node.srcmap = state.get_map(state.line - len, state.line - 1);
                        state.node.children.push(node);
                    }
                } else {
                    // this can only happen if user disables paragraph rule
                    // push text as is, this behavior can change in the future;
                    // users should always have some kind of default block rule
                    let mut content = state.get_line(state.line).to_owned();
                    content.push('\n');
                    let node = Node::new(InlineRoot::new(
                        content,
                        vec![(0, state.line_offsets[state.line].first_nonspace)],
                    ));
                    state.node.children.push(node);
                    state.line += 1;
                }

                // set state.tight if we had an empty line before current tag
                // i.e. latest empty line should not count
                state.tight = !has_empty_lines;

                // paragraph might "eat" one newline after it in nested lists
                if state.is_empty(state.line - 1) {
                    has_empty_lines = true;
                }

                if state.line < state.line_max && state.is_empty(state.line) {
                    has_empty_lines = true;
                    state.line += 1;
                }
            }
        });
    }

    /// Tokenize the contents of a nested block container.
    ///
    /// Block rules that recursively invoke the block parser must use this
    /// method so [`MarkdownIt::max_nesting`] can stop excessively deep input.
    pub fn tokenize_nested(&self, state: &mut BlockState) {
        let old_level = state.level;
        state.level = state.level.saturating_add(1);
        self.tokenize(state);
        state.level = old_level;
    }

    /// Process input string and push block tokens into `out_tokens`
    ///
    pub fn parse(&self, src: &str, node: Node, md: &MarkdownIt, root_ext: &mut RootExtSet) -> Node {
        let mut state = BlockState::new(src, md, root_ext, node);
        self.tokenize(&mut state);
        state.node
    }

    pub fn add_rule<T: BlockRule>(&mut self) -> RuleBuilder<'_, RuleEntry> {
        self.dispatch = OnceLock::new();
        let item = self
            .ruler
            .add(RuleMark::of::<T>(), (T::MARKERS, (T::check, T::run)));
        for name in T::NAMES {
            item.alias(RuleMark::named(*name));
        }
        RuleBuilder::new(item)
    }

    pub fn has_rule<T: BlockRule>(&self) -> bool {
        self.ruler.contains(RuleMark::of::<T>())
    }

    pub fn remove_rule<T: BlockRule>(&mut self) {
        self.dispatch = OnceLock::new();
        self.ruler.remove(RuleMark::of::<T>());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AtRule;
    struct HashRule;
    struct DigitRule;
    struct SnowRule;
    struct WildcardRule;
    struct InterruptRule;

    macro_rules! empty_rule {
        ($rule:ty, $markers:expr) => {
            impl BlockRule for $rule {
                const MARKERS: &'static [char] = $markers;

                fn run(_: &mut BlockState) -> Option<(Node, usize)> {
                    None
                }
            }
        };
    }

    empty_rule!(AtRule, &['@']);
    empty_rule!(HashRule, &['#']);
    empty_rule!(
        DigitRule,
        &['0', '1', '2', '3', '4', '5', '6', '7', '8', '9']
    );
    empty_rule!(SnowRule, &['雪']);
    empty_rule!(WildcardRule, &[]);

    impl BlockRule for InterruptRule {
        const MARKERS: &'static [char] = &['!'];

        fn check(_: &mut BlockState) -> Option<()> {
            Some(())
        }

        fn run(_: &mut BlockState) -> Option<(Node, usize)> {
            Some((Node::new(NodeEmpty), 1))
        }
    }

    fn check_id<T: BlockRule>() -> usize {
        T::check as fn(&mut BlockState) -> Option<()> as usize
    }

    fn check_ids(rules: &[RuleFns]) -> Vec<usize> {
        rules.iter().map(|rule| rule.0 as usize).collect()
    }

    #[test]
    fn dispatch_filters_rules_without_changing_order() {
        let mut parser = BlockParser::new();
        parser.add_rule::<AtRule>().alias_named("at");
        parser.add_rule::<WildcardRule>().before_named("at");
        parser.add_rule::<HashRule>().after_named("at");

        assert_eq!(
            check_ids(parser.rules_for('@')),
            vec![check_id::<WildcardRule>(), check_id::<AtRule>()]
        );
        assert_eq!(
            check_ids(parser.rules_for('#')),
            vec![check_id::<WildcardRule>(), check_id::<HashRule>()]
        );
        assert_eq!(
            check_ids(parser.rules_for('!')),
            vec![check_id::<WildcardRule>()]
        );
    }

    #[test]
    fn dispatch_supports_multiple_and_unicode_markers_and_invalidates() {
        let mut parser = BlockParser::new();
        parser.add_rule::<DigitRule>();
        parser.add_rule::<WildcardRule>().after_all();

        for marker in '0'..='9' {
            assert_eq!(
                check_ids(parser.rules_for(marker)),
                vec![check_id::<DigitRule>(), check_id::<WildcardRule>()]
            );
        }

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

        parser.remove_rule::<DigitRule>();
        assert_eq!(
            check_ids(parser.rules_for('1')),
            vec![check_id::<WildcardRule>()]
        );
    }

    #[test]
    fn rules_for_line_skips_empty_lines_and_handles_unicode() {
        let mut md = MarkdownIt::empty();
        md.block.add_rule::<AtRule>();
        md.block.add_rule::<SnowRule>();

        let mut root_ext = RootExtSet::default();
        let mut state =
            BlockState::new("   \n@at\n雪", &md, &mut root_ext, Node::new(NodeEmpty));

        // Line 0 is all spaces: empty, must yield no rules (and not panic).
        assert!(md.block.rules_for_line(&state).is_empty());

        state.line = 1;
        assert_eq!(
            check_ids(md.block.rules_for_line(&state)),
            vec![check_id::<AtRule>()]
        );

        // Multi-byte char: goes through the unicode marker table.
        state.line = 2;
        assert_eq!(
            check_ids(md.block.rules_for_line(&state)),
            vec![check_id::<SnowRule>()]
        );
    }

    #[test]
    fn paragraph_interruption_uses_marker_dispatch() {
        let mut md = MarkdownIt::empty();
        crate::plugins::cmark::add(&mut md);
        md.block
            .add_rule::<InterruptRule>()
            .before_named("paragraph");

        assert_eq!(
            md.parse("ordinary\n!interrupt").render(),
            "<p>ordinary</p>\n"
        );
    }
}
