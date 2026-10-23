//! Block rule chain
mod document;
mod lines;
mod parser;
mod rule;
mod state;
pub(crate) use document::parse_document;
pub use lines::*;
pub use parser::BlockParserRule;
pub use rule::*;
pub use state::BlockState;

use crate::common::RuleMark;
use crate::common::ruler::Ruler;
use crate::{MarkdownIt, NodeId};

/// Register the core rule that drives block parsing.
pub fn add(md: &mut MarkdownIt) {
    parser::add(md);
}

pub(crate) type BlockRuleFns = (
    fn(&mut BlockState<'_>) -> Option<()>,
    fn(&mut BlockState<'_>) -> Option<(Option<NodeId>, usize)>,
);

#[derive(Debug, Default)]
pub struct BlockParser {
    ruler: Ruler<RuleMark, BlockRuleFns>,
}

impl BlockParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn rules(&self) -> &[BlockRuleFns] {
        self.ruler.as_slice()
    }

    pub fn add_rule<T: BlockRule>(&mut self) -> RuleBuilder<'_, BlockRuleFns> {
        let item = self.ruler.add(RuleMark::of::<T>(), (T::check, T::run));
        for name in T::NAMES {
            item.alias(RuleMark::named(*name));
        }
        RuleBuilder::new(item)
    }

    pub fn has_rule<T: BlockRule>(&self) -> bool {
        self.ruler.contains(RuleMark::of::<T>())
    }

    pub fn remove_rule<T: BlockRule>(&mut self) {
        self.ruler.remove(RuleMark::of::<T>());
    }
}
