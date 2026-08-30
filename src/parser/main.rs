use crate::common::RuleMark;
use crate::common::ruler::Ruler;
use crate::common::sourcemap::SourcePos;
use crate::parser::block::{self, BlockParser};
use crate::parser::core::{Root, *};
use crate::parser::extset::MarkdownItExtSet;
use crate::parser::inline::{self, InlineParser};
use crate::parser::linkfmt::{LinkFormatter, MDLinkFormatter};
use crate::parser::node::Node;
use crate::parser::render_options::RenderOptions;
use crate::plugins::presets::{Preset, PresetConfig};

type RuleFn = fn(&mut Node, &MarkdownIt);

/// Main parser struct, created once and reused for parsing multiple documents.
pub struct MarkdownIt {
    /// Block-level tokenizer.
    pub block: BlockParser,

    /// Inline-level tokenizer.
    pub inline: InlineParser,

    /// Link validator and formatter.
    pub link_formatter: Box<dyn LinkFormatter>,

    /// Storage for custom data used in plugins.
    pub ext: MarkdownItExtSet,

    /// Maximum depth of the generated AST, exists to prevent recursion
    /// (if markdown source reaches this depth, deeply nested structures
    /// will be parsed as plain text).
    #[doc(hidden)]
    pub max_nesting: u32,

    /// Maximum allowed indentation for syntax blocks
    /// default i32::MAX, indented code blocks will set this to 4
    pub max_indent: i32,

    /// Default rendering options.
    pub render_options: RenderOptions,

    ruler: Ruler<RuleMark, RuleFn>,
}

impl std::fmt::Debug for MarkdownIt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MarkdownIt")
            .field("block", &self.block)
            .field("inline", &self.inline)
            .field("link_formatter", &self.link_formatter)
            .field("ext", &self.ext)
            .field("max_nesting", &self.max_nesting)
            .field("max_indent", &self.max_indent)
            .field("ruler", &self.ruler)
            .field("render_options", &self.render_options)
            .finish()
    }
}

impl MarkdownIt {
    /// Create a parser using the markdown-it.js default preset.
    pub fn new() -> Self {
        Self::with_preset(Preset::MarkdownItDefault)
    }

    pub fn empty() -> Self {
        let mut md = Self {
            block: BlockParser::new(),
            inline: InlineParser::new(),
            link_formatter: Box::new(MDLinkFormatter),
            ext: MarkdownItExtSet::new(),
            max_nesting: 100,
            max_indent: i32::MAX,
            render_options: RenderOptions::default(),
            ruler: Ruler::new(),
        };

        // infrastructure
        block::builtin::add(&mut md);
        inline::builtin::add(&mut md);

        md
    }

    /// Parse a markdown source string into an AST ([`Node`]).
    ///
    /// The default [`MarkdownIt::render_options`] are stored in the node,
    /// so calling [`Node::render`] will use them.
    pub fn parse(&self, src: &str) -> Node {
        let mut node = Node::new(Root::new(src.to_owned()));
        node.ext.insert(self.render_options.clone());
        node.srcmap = Some(SourcePos::new(0, src.len()));

        for rule in self.ruler.iter() {
            rule(&mut node, self);
            debug_assert!(
                node.is::<Root>(),
                "root node of the AST must always be Root"
            );
        }
        node
    }

    /// Parse `src` and render it to HTML, using the options stored in the
    /// AST (see [`MarkdownIt::render_options`]).
    pub fn render(&self, src: &str) -> String {
        self.parse(src).render()
    }

    /// Register a new core rule for type `T`, returning a builder to
    /// position it relative to other rules (before/after/alias/...).
    pub fn add_rule<T: CoreRule>(&mut self) -> RuleBuilder<'_, RuleFn> {
        let item = self.ruler.add(RuleMark::of::<T>(), T::run);
        for name in T::NAMES {
            item.alias(RuleMark::named(*name));
        }
        RuleBuilder::new(item)
    }

    /// Check whether a rule of type `T` is registered.
    pub fn has_rule<T: CoreRule>(&self) -> bool {
        self.ruler.contains(RuleMark::of::<T>())
    }

    /// Remove the rule of type `T` from the ruler.
    pub fn remove_rule<T: CoreRule>(&mut self) {
        self.ruler.remove(RuleMark::of::<T>());
    }

    /// Create a parser configured with a preset (e.g. `Preset::CommonMark`)
    /// or a custom closure `|md| { ... }`.
    pub fn with_preset(preset: impl PresetConfig) -> Self {
        let mut md = Self::empty();
        preset.configure(&mut md);
        md
    }
}

impl Default for MarkdownIt {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::MarkdownIt;
    use crate::parser::block::builtin::BlockParserRule;
    use crate::parser::inline::builtin::TextScanner;
    use crate::plugins::cmark;
    use crate::plugins::cmark::block::paragraph::ParagraphScanner;

    #[test]
    fn new_uses_markdown_it_default_preset() {
        let md = MarkdownIt::new();

        assert_eq!(md.render("~~deleted~~"), "<p><s>deleted</s></p>\n");
        assert_eq!(
            md.render("| a |\n| - |"),
            "<table>\n<thead>\n<tr>\n<th>a</th>\n</tr>\n</thead>\n</table>\n"
        );
        assert_eq!(
            md.render("<em>escaped</em>"),
            "<p>&lt;em&gt;escaped&lt;/em&gt;</p>\n"
        );
    }

    #[test]
    fn default_matches_new() {
        let src = "Hello **world**!";

        assert_eq!(
            MarkdownIt::default().render(src),
            MarkdownIt::new().render(src)
        );
    }

    #[test]
    fn empty_does_not_install_markdown_syntax() {
        let md = MarkdownIt::empty();

        assert_eq!(md.render("# **plain**"), "# **plain**\n");
    }

    #[test]
    fn with_preset_starts_from_empty_parser() {
        let md = MarkdownIt::with_preset(|md: &mut MarkdownIt| cmark::add(md));

        assert_eq!(md.render("~~plain~~"), "<p>~~plain~~</p>\n");
    }

    #[test]
    fn replaces_nul_with_replacement_character() {
        let md = MarkdownIt::with_preset(|md: &mut MarkdownIt| cmark::add(md));

        let html = md.render("abc\0de\0");

        assert_eq!(html, "<p>abc\u{FFFD}de\u{FFFD}</p>\n");
        assert!(!html.contains('\0'));
    }

    #[test]
    fn rule_presence_can_be_checked_through_a_shared_reference() {
        let md = MarkdownIt::new();
        let md = &md;

        assert!(md.has_rule::<BlockParserRule>());
        assert!(md.block.has_rule::<ParagraphScanner>());
        assert!(md.inline.has_rule::<TextScanner>());
    }
}
