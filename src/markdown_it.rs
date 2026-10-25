use std::borrow::Cow;
use std::sync::Arc;

use crate::common::RuleMark;
use crate::common::extset::MarkdownItExtSet;
use crate::common::ruler::Ruler;
#[cfg(feature = "sourcemap")]
use crate::common::sourcemap::SourcePos;
use crate::document::{Document, NodeValue, Root, Text, TextSpecial};
use crate::links::{LinkFormatter, MDLinkFormatter};
use crate::parser::block::{self, BlockParser};
use crate::parser::core::*;
use crate::parser::inline::{self, InlineParser};
use crate::plugins::presets::{Preset, PresetConfig};
use crate::render::{
    DebugTreeRenderer,
    HtmlTextRenderer,
    NodeRenderer,
    PlainTextRenderer,
    RenderOptions,
    RendererRegistry,
    TransparentRenderer,
};

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

    /// Format-specific renderers for documents.
    pub renderers: RendererRegistry,

    ruler: Ruler<RuleMark, CoreRuleRunner>,
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
            .field("renderers", &self.renderers)
            .finish()
    }
}

impl MarkdownIt {
    /// Create a parser using the markdown-it.js default preset.
    pub fn new() -> Self {
        Self::with_preset(Preset::MarkdownItDefault)
    }

    pub fn empty() -> Self {
        let mut renderers = RendererRegistry::new();
        renderers.add::<Root, _>("html", TransparentRenderer);
        renderers.add::<Text, _>("html", HtmlTextRenderer);
        renderers.add::<TextSpecial, _>("html", HtmlTextRenderer);
        renderers.add::<Root, _>("text", TransparentRenderer);
        renderers.add::<Text, _>("text", PlainTextRenderer);
        renderers.add::<TextSpecial, _>("text", PlainTextRenderer);
        renderers.add::<Root, _>("debug", DebugTreeRenderer);

        let mut md = Self {
            block: BlockParser::new(),
            inline: InlineParser::new(),
            link_formatter: Box::new(MDLinkFormatter),
            ext: MarkdownItExtSet::new(),
            max_nesting: 100,
            max_indent: i32::MAX,
            render_options: RenderOptions::default(),
            renderers,
            ruler: Ruler::new(),
        };

        // infrastructure
        block::add(&mut md);
        inline::add(&mut md);

        md
    }

    /// Parse Markdown into a document by executing core rules in ruler order.
    ///
    /// NUL is replaced with U+FFFD first, so [`Document::source`] and byte source
    /// maps refer to the normalized text.
    ///
    /// At low `max_nesting` values links and images may remain literal; a zero
    /// limit stops block parsing.
    pub fn parse(&self, src: &str) -> Document {
        let source: Arc<str> = Arc::from(normalize_input(src).as_ref());
        let mut document = Document::new(Arc::clone(&source), Root::new(Arc::clone(&source)));
        document
            .node_mut(document.root())
            .ext_mut()
            .insert(self.render_options.clone());

        #[cfg(feature = "sourcemap")]
        document
            .node_mut(document.root())
            .set_srcmap(Some(SourcePos::new(0, source.len())));

        let mut state = CoreState::new(&source, self, &mut document);
        for rule in self.ruler.iter() {
            rule.run(&mut state);
        }

        document.trim_unused_tail();
        document
    }

    /// Register or replace a renderer for one payload type and output format.
    pub fn add_renderer<T, R>(&mut self, format: impl Into<String>, renderer: R) -> bool
    where
        T: NodeValue,
        R: NodeRenderer<T>,
    {
        self.renderers.add::<T, R>(format, renderer)
    }

    /// Render a document with the renderers registered
    /// for `format`.
    ///
    /// The built-in format keys are `"html"`, `"text"`, and `"debug"`.
    /// Standard syntax plugins register HTML and plain-text behavior; plugins
    /// that add leaf payloads must register both explicitly. The diagnostic
    /// debug tree traverses all descendants without per-payload registration.
    ///
    /// # Panics
    ///
    /// Panics if a leaf node has no renderer registered for `format`.
    pub fn render_as(&self, document: &Document, format: &str) -> String {
        self.renderers
            .render(document, format, &self.render_options)
    }

    /// Render a document as HTML.
    ///
    /// # Panics
    ///
    /// Panics if a leaf node has no `"html"` renderer.
    pub fn render_html(&self, document: &Document) -> String {
        self.render_as(document, "html")
    }

    /// Parse `src` and render it as HTML using [`MarkdownIt::render_options`].
    pub fn render(&self, src: &str) -> String {
        self.render_html(&self.parse(src))
    }

    /// Register a new core rule for type `T`, returning a builder to position
    /// it relative to other rules.
    ///
    /// # Panics
    ///
    /// Panics if `T` is already registered, directly or via an alias.
    pub fn add_rule<T: CoreRule + Default>(&mut self) -> RuleBuilder<'_, CoreRuleRunner> {
        self.add_rule_instance(T::default())
    }

    /// Register an owned core rule instance, e.g. for rules with runtime
    /// configuration. To replace a rule, remove it first. Named aliases may be
    /// shared for group ordering.
    ///
    /// # Panics
    ///
    /// Panics if `T` is already registered, directly or via an alias.
    pub fn add_rule_instance<T: CoreRule>(&mut self, rule: T) -> RuleBuilder<'_, CoreRuleRunner> {
        assert!(
            !self.has_rule::<T>(),
            "core rule type is already registered: {:?}",
            std::any::type_name::<T>()
        );
        let item = self
            .ruler
            .add(RuleMark::of::<T>(), CoreRuleRunner::new(rule));
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

    pub(crate) fn validate_rules(&self) {
        let _ = self.ruler.as_slice();
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

/// Replace NUL with U+FFFD before parsing, matching markdown-it's `normalize`.
fn normalize_input(src: &str) -> Cow<'_, str> {
    if src.contains('\0') {
        Cow::Owned(src.replace('\0', "\u{FFFD}"))
    } else {
        Cow::Borrowed(src)
    }
}

#[cfg(test)]
mod tests {
    use super::MarkdownIt;
    use crate::parser::block::BlockParserRule;
    use crate::parser::inline::TextScanner;
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
