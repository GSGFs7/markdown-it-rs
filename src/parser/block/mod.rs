//! Block rule chain
#[doc(hidden)]
pub mod builtin;
mod lines;
mod rule;
mod state;
pub use lines::*;
pub use rule::*;
pub use state::DocumentBlockState;

use crate::NodeDraft;
use crate::common::RuleMark;
use crate::common::ruler::Ruler;

pub(crate) type BlockRuleFns = (
    fn(&mut DocumentBlockState<'_>) -> Option<()>,
    fn(&mut DocumentBlockState<'_>) -> Option<(NodeDraft, usize)>,
);

#[derive(Debug, Default)]
pub struct BlockParser {
    ruler: Ruler<RuleMark, BlockRuleFns>,
}

impl BlockParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn document_rules(&self) -> Vec<BlockRuleFns> {
        self.ruler.iter().copied().collect()
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
