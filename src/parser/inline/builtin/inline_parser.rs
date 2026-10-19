use crate::MarkdownIt;
use crate::parser::core::{CoreRule, CoreStage};

pub fn add(md: &mut MarkdownIt) {
    md.add_rule::<InlineParserRule>()
        .after::<crate::parser::block::builtin::BlockParserRule>()
        .before_all();
}

#[derive(Default)]
pub struct InlineParserRule;
impl CoreRule for InlineParserRule {
    const NAMES: &'static [&'static str] = &["inline"];
    fn stage(&self) -> CoreStage {
        CoreStage::ParseInlines
    }
}
