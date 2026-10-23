use crate::MarkdownIt;
use crate::parser::core::{CoreRule, CoreState};

pub fn add(md: &mut MarkdownIt) {
    md.add_rule::<InlineParserRule>()
        .after::<crate::parser::block::BlockParserRule>()
        .before_all();
}

#[derive(Default)]
pub struct InlineParserRule;
impl CoreRule for InlineParserRule {
    const NAMES: &'static [&'static str] = &["inline"];

    fn run(&self, state: &mut CoreState<'_>) {
        let md = state.md;
        state.with_root_ext(|document, root_ext| {
            crate::parser::inline::parse_document(md, document, root_ext);
        });
    }
}
