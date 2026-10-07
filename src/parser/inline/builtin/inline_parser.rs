use crate::MarkdownIt;
use crate::parser::core::{CoreRule, DocumentCoreRule};

pub fn add(md: &mut MarkdownIt) {
    md.add_rule::<InlineParserRule>()
        .after::<crate::parser::block::builtin::BlockParserRule>()
        .before_all();
}

pub struct InlineParserRule;
impl CoreRule for InlineParserRule {
    const NAMES: &'static [&'static str] = &["inline"];
    fn document_rule() -> DocumentCoreRule {
        DocumentCoreRule::ParseInlines
    }
}
