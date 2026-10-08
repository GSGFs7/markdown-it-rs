use crate::MarkdownIt;
use crate::parser::core::{CoreRule, DocumentCoreRule};

pub fn add(md: &mut MarkdownIt) {
    md.add_rule::<BlockParserRule>().before_all();
}

pub struct BlockParserRule;
impl CoreRule for BlockParserRule {
    const NAMES: &'static [&'static str] = &["block"];

    fn document_rule() -> DocumentCoreRule {
        DocumentCoreRule::ParseBlocks
    }
}
