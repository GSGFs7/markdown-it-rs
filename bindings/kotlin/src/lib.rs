use std::sync::Arc;

use markdown_it::MarkdownIt;
use markdown_it::parser::core::Root;
use markdown_it::plugins::extra::front_matter::{
    FrontMatter as RustFrontMatter,
    FrontMatterKind as RustFrontMatterKind,
};

uniffi::setup_scaffolding!();

/// Configuration fixed when a parser is created.
///
/// Keeping the parser immutable after construction makes it safe to reuse for
/// multiple documents and keeps configuration changes out of the FFI hot path.
#[derive(Clone, Debug, Default, uniffi::Record)]
pub struct MarkdownOptions {
    pub html: bool,
    pub linkify: bool,
    pub math: bool,
    pub front_matter: bool,
    pub typographer: bool,
    pub source_position: bool,
    pub heading_anchors: bool,
    pub directives: bool,
    pub task_list: bool,
    pub footnote: bool,
    pub syntax_highlighting: bool,
    pub syntax_theme: Option<String>,
    pub syntax_classed: bool,
}

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum FrontMatterKind {
    Yaml,
    Toml,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct FrontMatter {
    pub kind: FrontMatterKind,
    pub raw: String,
    pub start_line: u64,
    pub end_line: u64,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct RenderResult {
    pub html: String,
    pub front_matter: Option<FrontMatter>,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum MarkdownError {
    #[error("the binding was built without linkify support")]
    LinkifyUnavailable,
    #[error("the binding was built without syntax highlighting support")]
    SyntaxHighlightingUnavailable,
    #[error("unknown syntax highlighting theme")]
    UnknownSyntaxTheme,
}

#[derive(uniffi::Object)]
pub struct MarkdownParser {
    inner: MarkdownIt,
}

#[uniffi::export]
impl MarkdownParser {
    #[uniffi::constructor]
    pub fn new(options: MarkdownOptions) -> Result<Arc<Self>, MarkdownError> {
        validate_options(&options)?;

        Ok(Arc::new(Self {
            inner: build_parser(&options),
        }))
    }

    pub fn render(&self, source: String) -> String {
        self.inner.render(&source)
    }

    pub fn render_with_metadata(&self, source: String) -> RenderResult {
        let root = self.inner.parse(&source);
        let front_matter = root
            .cast::<Root>()
            .and_then(|root| root.ext.get::<RustFrontMatter>())
            .map(FrontMatter::from);

        RenderResult {
            html: root.render(),
            front_matter,
        }
    }

    pub fn syntax_theme_css(&self) -> Option<String> {
        #[cfg(feature = "syntect")]
        {
            markdown_it::plugins::extra::syntect::theme_css(&self.inner)
        }

        #[cfg(not(feature = "syntect"))]
        {
            None
        }
    }
}

/// Return the built-in syntax themes compiled into this native library.
#[uniffi::export]
pub fn available_syntax_themes() -> Vec<String> {
    #[cfg(feature = "syntect")]
    {
        markdown_it::plugins::extra::syntect::available_themes()
    }

    #[cfg(not(feature = "syntect"))]
    {
        Vec::new()
    }
}

fn validate_options(options: &MarkdownOptions) -> Result<(), MarkdownError> {
    #[cfg(not(feature = "linkify"))]
    if options.linkify {
        return Err(MarkdownError::LinkifyUnavailable);
    }

    #[cfg(not(feature = "syntect"))]
    if options.syntax_highlighting {
        return Err(MarkdownError::SyntaxHighlightingUnavailable);
    }

    #[cfg(feature = "syntect")]
    if options.syntax_highlighting
        && let Some(theme) = &options.syntax_theme
        && !markdown_it::plugins::extra::syntect::available_themes().contains(theme)
    {
        return Err(MarkdownError::UnknownSyntaxTheme);
    }

    Ok(())
}

fn build_parser(options: &MarkdownOptions) -> MarkdownIt {
    let mut parser = MarkdownIt::empty();

    // Front matter must run before CommonMark block parsing.
    if options.front_matter {
        markdown_it::plugins::extra::front_matter::add(&mut parser);
    }

    markdown_it::plugins::cmark::add(&mut parser);
    markdown_it::plugins::extra::tables::add(&mut parser);
    markdown_it::plugins::extra::strikethrough::add(&mut parser);
    markdown_it::plugins::extra::mark::add(&mut parser);
    markdown_it::plugins::extra::beautify_links::add(&mut parser);

    if options.directives {
        markdown_it::plugins::directives::add(&mut parser);
    }
    if options.task_list {
        markdown_it::plugins::extra::tasklist::add(&mut parser);
    }
    if options.footnote {
        markdown_it::plugins::extra::footnote::add(&mut parser);
    }
    if options.heading_anchors {
        markdown_it::plugins::extra::heading_anchors::add(&mut parser);
    }
    if options.html {
        markdown_it::plugins::html::add(&mut parser);
    }
    if options.linkify {
        #[cfg(feature = "linkify")]
        markdown_it::plugins::extra::linkify::add(&mut parser);
    }
    if options.math {
        markdown_it::plugins::extra::math::add(&mut parser);
    }
    if options.typographer {
        markdown_it::plugins::extra::typographer::add(&mut parser);
        markdown_it::plugins::extra::smartquotes::add(&mut parser);
    }
    if options.source_position {
        markdown_it::plugins::sourcepos::add(&mut parser);
    }
    if options.syntax_highlighting {
        #[cfg(feature = "syntect")]
        {
            markdown_it::plugins::extra::syntect::add(&mut parser);
            if let Some(theme) = &options.syntax_theme {
                markdown_it::plugins::extra::syntect::set_theme(&mut parser, theme);
            }
            if options.syntax_classed {
                markdown_it::plugins::extra::syntect::set_to_classed(&mut parser);
            }
        }
    }

    parser
}

impl From<&RustFrontMatter> for FrontMatter {
    fn from(value: &RustFrontMatter) -> Self {
        let kind = match value.kind {
            RustFrontMatterKind::Yaml => FrontMatterKind::Yaml,
            RustFrontMatterKind::Toml => FrontMatterKind::Toml,
        };

        Self {
            kind,
            raw: value.raw.clone(),
            start_line: value.start_line as u64,
            end_line: value.end_line as u64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_commonmark_and_extensions() {
        let parser = MarkdownParser::new(MarkdownOptions::default()).unwrap();

        assert_eq!(
            parser.render("# Hello **Kotlin**".to_owned()),
            "<h1>Hello <strong>Kotlin</strong></h1>\n"
        );
        assert_eq!(parser.render("~~old~~".to_owned()), "<p><s>old</s></p>\n");
    }

    #[test]
    fn renders_front_matter_metadata() {
        let parser = MarkdownParser::new(MarkdownOptions {
            front_matter: true,
            ..MarkdownOptions::default()
        })
        .unwrap();

        let result = parser.render_with_metadata("---\ntitle: Kotlin\n---\n# Hello".to_owned());
        let front_matter = result.front_matter.unwrap();

        assert_eq!(result.html, "<h1>Hello</h1>\n");
        assert!(matches!(front_matter.kind, FrontMatterKind::Yaml));
        assert_eq!(front_matter.raw, "title: Kotlin");
        assert_eq!(front_matter.start_line, 0);
        assert_eq!(front_matter.end_line, 2);
    }

    #[test]
    fn renders_optional_plugins() {
        let parser = MarkdownParser::new(MarkdownOptions {
            heading_anchors: true,
            task_list: true,
            ..MarkdownOptions::default()
        })
        .unwrap();

        let html = parser.render("# Hello Kotlin\n\n- [x] bound".to_owned());
        assert!(html.contains("<h1 id=\"hello-kotlin\">"));
        assert!(html.contains("task-list-item"));
    }

    #[test]
    fn renders_directive_attributes_without_sanitizing() {
        let parser = MarkdownParser::new(MarkdownOptions {
            directives: true,
            ..MarkdownOptions::default()
        })
        .unwrap();

        assert_eq!(
            parser.render(r#"hello :badge{label="Beta" onclick="alert(1)"} world"#.to_owned()),
            "<p>hello <span class=\"directive badge\" label=\"Beta\" onclick=\"alert(1)\"></span> world</p>\n"
        );
    }

    #[cfg(not(feature = "linkify"))]
    #[test]
    fn rejects_linkify_when_not_compiled_in() {
        let result = MarkdownParser::new(MarkdownOptions {
            linkify: true,
            ..MarkdownOptions::default()
        });

        assert!(matches!(result, Err(MarkdownError::LinkifyUnavailable)));
    }

    #[cfg(not(feature = "syntect"))]
    #[test]
    fn rejects_syntax_highlighting_when_not_compiled_in() {
        let result = MarkdownParser::new(MarkdownOptions {
            syntax_highlighting: true,
            ..MarkdownOptions::default()
        });

        assert!(matches!(
            result,
            Err(MarkdownError::SyntaxHighlightingUnavailable)
        ));
    }

    #[cfg(feature = "syntect")]
    #[test]
    fn configures_syntax_highlighting() {
        let theme = available_syntax_themes().into_iter().next().unwrap();
        let parser = MarkdownParser::new(MarkdownOptions {
            syntax_highlighting: true,
            syntax_theme: Some(theme),
            syntax_classed: true,
            ..MarkdownOptions::default()
        })
        .unwrap();

        assert!(parser.syntax_theme_css().is_some());
        assert!(
            parser
                .render("```rust\nfn main() {}\n```".to_owned())
                .contains("syntect-")
        );
    }
}
