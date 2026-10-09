use std::sync::Arc;

use crate::common::RuleMark;
use crate::common::extset::MarkdownItExtSet;
use crate::common::ruler::Ruler;
use crate::document::transform::{
    DocumentTransform,
    DocumentTransformRegistry,
    TransformRuleBuilder,
};
use crate::document::{Document, NodeDraft, NodeValue, Root, Text, TextSpecial};
use crate::links::{LinkFormatter, MDLinkFormatter};
use crate::parser::block::{self, BlockParser};
use crate::parser::core::*;
use crate::parser::inline::{self, InlineParser};
use crate::parser::pipeline::DocumentParseContext;
use crate::plugins::presets::{Preset, PresetConfig};
use crate::render::{
    DebugTreeDocumentRenderer,
    DocumentNodeRenderer,
    DocumentRendererRegistry,
    HtmlTextDocumentRenderer,
    PlainTextDocumentRenderer,
    RenderOptions,
    TransparentDocumentRenderer,
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

    /// Ordered transforms for arena-backed documents.
    pub document_transforms: DocumentTransformRegistry,

    /// Format-specific renderers for arena-backed documents.
    pub document_renderers: DocumentRendererRegistry,

    ruler: Ruler<RuleMark, DocumentCoreRule>,
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
            .field("document_transforms", &self.document_transforms)
            .field("document_renderers", &self.document_renderers)
            .finish()
    }
}

impl MarkdownIt {
    /// Create a parser using the markdown-it.js default preset.
    pub fn new() -> Self {
        Self::with_preset(Preset::MarkdownItDefault)
    }

    pub fn empty() -> Self {
        let mut document_renderers = DocumentRendererRegistry::new();
        document_renderers.add::<Root, _>("html", TransparentDocumentRenderer);
        document_renderers.add::<Text, _>("html", HtmlTextDocumentRenderer);
        document_renderers.add::<TextSpecial, _>("html", HtmlTextDocumentRenderer);
        document_renderers.add::<Root, _>("text", TransparentDocumentRenderer);
        document_renderers.add::<Text, _>("text", PlainTextDocumentRenderer);
        document_renderers.add::<TextSpecial, _>("text", PlainTextDocumentRenderer);
        document_renderers.add::<Root, _>("debug", DebugTreeDocumentRenderer);

        let mut md = Self {
            block: BlockParser::new(),
            inline: InlineParser::new(),
            link_formatter: Box::new(MDLinkFormatter),
            ext: MarkdownItExtSet::new(),
            max_nesting: 100,
            max_indent: i32::MAX,
            render_options: RenderOptions::default(),
            document_transforms: DocumentTransformRegistry::new(),
            document_renderers,
            ruler: Ruler::new(),
        };

        // infrastructure
        block::builtin::add(&mut md);
        inline::builtin::add(&mut md);

        md
    }

    /// Parse Markdown into an arena-backed document and run registered transforms.
    ///
    /// At low `max_nesting` values links and images may remain literal; a zero
    /// limit stops block parsing.
    ///
    /// # Panics
    ///
    /// Panics if core-rule stages are missing, repeated, or out of order.
    pub fn parse_document(&self, src: &str) -> Document {
        let source: Arc<str> = Arc::from(src);
        let mut root = NodeDraft::new(Root::new(Arc::clone(&source)));
        root.ext_mut().insert(self.render_options.clone());
        DocumentParseContext::new(source, self, root).parse()
    }

    pub(super) fn document_core_rules(&self) -> impl Iterator<Item = DocumentCoreRule> + '_ {
        self.ruler.iter().copied()
    }

    /// Register an arena-backed document transform.
    pub fn add_document_transform<T: DocumentTransform + Default>(
        &mut self,
    ) -> TransformRuleBuilder<'_> {
        self.document_transforms.add::<T>()
    }

    /// Register an owned arena-backed document transform instance.
    pub fn add_document_transform_instance<T: DocumentTransform>(
        &mut self,
        transform: T,
    ) -> TransformRuleBuilder<'_> {
        self.document_transforms.add_instance(transform)
    }

    /// Run registered transforms again after manually editing a document.
    /// Parsing already runs this pipeline once automatically.
    pub fn run_document_transforms(&self, document: &mut Document) {
        self.document_transforms.run(document);
    }

    /// Register or replace a renderer for one payload type and output format.
    pub fn add_document_renderer<T, R>(&mut self, format: impl Into<String>, renderer: R) -> bool
    where
        T: NodeValue,
        R: DocumentNodeRenderer<T>,
    {
        self.document_renderers.add::<T, R>(format, renderer)
    }

    /// Render an arena-backed document directly with the renderers registered
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
    pub fn render_document_as(&self, document: &Document, format: &str) -> String {
        let output = self
            .document_renderers
            .render(document, format, &self.render_options);
        if output.contains('\0') {
            output.replace('\0', "\u{FFFD}")
        } else {
            output
        }
    }

    /// Render an arena-backed document directly as HTML.
    ///
    /// # Panics
    ///
    /// Panics if a leaf node has no `"html"` renderer.
    pub fn render_document(&self, document: &Document) -> String {
        self.render_document_as(document, "html")
    }

    /// Parse `src`, apply postprocessing, and render HTML using
    /// [`MarkdownIt::render_options`].
    pub fn render(&self, src: &str) -> String {
        self.render_document(&self.parse_document(src))
    }

    /// Register a new core rule for type `T`, returning a builder to
    /// position it relative to other rules (before/after/alias/...).
    pub fn add_rule<T: CoreRule>(&mut self) -> RuleBuilder<'_, DocumentCoreRule> {
        let item = self.ruler.add(RuleMark::of::<T>(), T::document_rule());
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
