use crate::MarkdownIt;
use crate::parser::core::{CoreRule, CoreStage};

pub fn add(md: &mut MarkdownIt) {
    md.add_rule::<BlockParserRule>().before_all();
}

pub struct BlockParserRule;
impl CoreRule for BlockParserRule {
    const NAMES: &'static [&'static str] = &["block"];

    fn stage() -> CoreStage {
        CoreStage::ParseBlocks
    }
}
