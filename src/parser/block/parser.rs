use crate::MarkdownIt;
use crate::parser::core::{CoreRule, CoreState};

pub fn add(md: &mut MarkdownIt) {
    md.add_rule::<BlockParserRule>().before_all();
}

#[derive(Default)]
pub struct BlockParserRule;
impl CoreRule for BlockParserRule {
    const NAMES: &'static [&'static str] = &["block"];

    fn run(&self, state: &mut CoreState<'_>) {
        let source = state.source;
        let md = state.md;
        state.with_root_ext(|document, root_ext| {
            crate::parser::block::parse_document(source, md, document, root_ext);
        });
    }
}
