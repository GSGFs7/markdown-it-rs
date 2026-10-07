//! Syntax highlighting for code blocks using `syntect`
//!
//! This plugin will highlight indented code blocks and fenced code blocks.
//! Fenced block will read the first token as language, for example `` ```rust ``.
//! unknown language and indented code blocks will be rendered as plain text.
//!
//! This plugin use `InspiredGitHub` theme and render inline styles by default.
//! Use [`set_theme`] to select another built-in theme (syntect defaults plus
//! two-face extras, e.g. `Nord`, `Dracula`, `Catppuccin Mocha`, `Solarized (dark)`).
//! It will panic when get an unknown theme.
//! Use [`available_themes`] to view all available themes.
//!
//! Use [`set_to_classed`] or [`set_to_classed_with_prefix`] to switch classed mode.
//! In this mode, you need to provide yourself styles.
//! You can also use [`theme_css`] get the CSS for selected built-in theme.
//! In inline mode, it will return `None`.
//!
//! Fenced code blocks can mark highlighted lines with a `{...}` line spec in the info string,
//! such as ` ```rust {1, 3-5} `.
//! Line number started with 1.
//!
//! ```rust
//! let mut md = markdown_it::MarkdownIt::empty();
//! markdown_it::plugins::cmark::add(&mut md);
//! markdown_it::plugins::extra::syntect::add(&mut md);
//! markdown_it::plugins::extra::syntect::set_theme(&mut md, "base16-ocean.dark");
//!
//! let html = md.parse("```rust\nfn main() {}\n```").render();
//! assert!(html.contains(r#"class="language-rust""#));
//! ```

//! For the arena-backed pipeline, use [`add_document`] and run transforms:
//!
//! ```rust
//! let mut md = markdown_it::MarkdownIt::empty();
//! markdown_it::plugins::cmark::add(&mut md);
//! markdown_it::plugins::extra::syntect::add_document(&mut md);
//! let mut document = md.parse_document_direct("```rust\nfn main() {}\n```");
//! md.run_document_transforms(&mut document);
//! assert!(md.render_document(&document).contains("language-rust"));
//! ```

use std::collections::HashSet;
use std::sync::{Arc, LazyLock, RwLock};

use syntect::easy::HighlightLines;
use syntect::highlighting::Theme;
use syntect::html::{
    ClassStyle,
    IncludeBackground,
    append_highlighted_html_for_styled_line,
    css_for_theme_with_class_style,
    line_tokens_to_classed_spans,
};
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;
use two_face::theme::LazyThemeSet;

use crate::common::utils::unescape_all;
use crate::document::edit::EditBatch;
use crate::document::transform::DocumentTransform;
use crate::document::{Document, NodeRef, StructuralEvent};
use crate::parser::core::CoreRule;
use crate::plugins::cmark::block::code::CodeBlock;
use crate::plugins::cmark::block::fence::CodeFence;
use crate::render::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    write_html_close,
    write_html_open,
};
use crate::{DocumentWriter, MarkdownIt, Node, NodeValue, Renderer};

// lazy load themes. it wast a lot of performance
static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(two_face::syntax::extra_newlines);
static THEME_SET: LazyLock<LazyThemeSet> =
    LazyLock::new(|| LazyThemeSet::from(two_face::theme::extra()));

// --- render ---

/// Rendered HTML produced by the syntect plugin.
///
/// This node will replace parsed code block nodes.
/// Its `html` field is emitted as raw HTML during rendering.
#[derive(Debug)]
pub struct SyntectSnippet {
    /// Highlighted HTML
    pub html: String,
    content: String,
    /// Language of the fenced code block (e.g. `rust`), if any.
    language: Option<String>,
    /// Default class prefix prepended to the language, e.g. `language-`.
    lang_prefix: String,
    /// Extra CSS class for the `<code>` element in classed mode (e.g. `syntect-code`).
    code_class: Option<String>,
}

impl NodeValue for SyntectSnippet {
    fn render(&self, _: &Node, fmt: &mut dyn Renderer) {
        let attrs = self.code_attrs(
            fmt.options()
                .and_then(|options| options.lang_prefix.as_deref()),
        );

        fmt.open("pre", &[]);
        fmt.open("code", &attrs);
        fmt.text_raw(&self.html);
        fmt.close("code");
        fmt.close("pre");
    }
}

impl SyntectSnippet {
    fn code_attrs(&self, lang_prefix: Option<&str>) -> Vec<(String, String)> {
        let lang_prefix = lang_prefix.unwrap_or(&self.lang_prefix);
        let mut classes = Vec::new();
        if let Some(class) = &self.code_class {
            classes.push(class.clone());
        }
        if let Some(language) = &self.language {
            if !language.is_empty() {
                classes.push(format!("{lang_prefix}{language}"));
            }
        }
        if classes.is_empty() {
            Vec::new()
        } else {
            vec![("class".into(), classes.join(" "))]
        }
    }
}

struct SyntectHtmlRenderer;

impl DocumentNodeRenderer<SyntectSnippet> for SyntectHtmlRenderer {
    fn render(
        &self,
        _: NodeRef<'_>,
        value: &SyntectSnippet,
        context: &mut DocumentRenderContext<'_>,
        output: &mut DocumentWriter,
    ) {
        write_html_open(output, "pre", &[]);
        write_html_open(
            output,
            "code",
            &value.code_attrs(context.options().lang_prefix.as_deref()),
        );
        output.write_str(&value.html);
        write_html_close(output, "code");
        write_html_close(output, "pre");
    }
}

struct SyntectTextRenderer;

impl DocumentNodeRenderer<SyntectSnippet> for SyntectTextRenderer {
    fn render(
        &self,
        _: NodeRef<'_>,
        value: &SyntectSnippet,
        context: &mut DocumentRenderContext<'_>,
        output: &mut DocumentWriter,
    ) {
        context.cr(output);
        output.write_str(&value.content);
        context.cr(output);
    }
}

// --- setting ---

#[derive(Debug, Clone, Copy)]
enum SyntectMode {
    Inline,
    Classed,
}

#[derive(Debug, Clone)]
struct SyntectSettings {
    theme: String,
    mode: SyntectMode,
    prefix: &'static str,
}

impl Default for SyntectSettings {
    fn default() -> Self {
        Self {
            theme: "InspiredGitHub".to_owned(),
            mode: SyntectMode::Inline,
            prefix: "syntect-",
        }
    }
}

struct FenceMeta {
    language: Option<String>,
    // highlight some lines
    // it looks like:
    //
    // ```rust {1, 3-4}
    // fn main() {
    //     print!("Hello world!");
    //     Ok(())
    // }
    // ```
    highlighted_lines: HashSet<usize>,
}

struct HighlightOptions<'a> {
    prefix: &'static str,
    highlighted_lines: &'a HashSet<usize>,
}

impl FenceMeta {
    // parse "{1, 4-7}" -> Set[1, 4, 5, 6, 7]
    fn parse_line_spec(spec: &str) -> HashSet<usize> {
        let mut lines = HashSet::new();
        for item in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            if let Some((start, end)) = item.split_once('-') {
                // parse "4-7"
                if let (Ok(start), Ok(end)) = (start.parse::<usize>(), end.parse::<usize>()) {
                    if start <= end {
                        lines.extend(start..=end);
                    }
                }
            } else if let Ok(line) = item.parse::<usize>() {
                // parse "1"
                lines.insert(line);
            }
        }

        lines
    }

    // parse "rust{1, 3}rs" -> "{1, 3}"
    fn extract_highlight_spec(info: &str) -> Option<&str> {
        let start = info.find('{')?;
        let rest = &info[start + 1..];
        let end = rest.find('}')?;
        Some(&rest[..end])
    }

    fn parse_fence_meta(data: &CodeFence) -> FenceMeta {
        // ```rust {1,3-5}   <-- CodeFence.info
        let info = unescape_all(&data.info);
        let trimmed = info.trim();

        let mut parts = trimmed.splitn(2, |c: char| c.is_whitespace());
        let first_part = parts.next().unwrap_or("");
        let rest_part = parts.next().unwrap_or("");
        let (language, meta_part) = if first_part.starts_with('{') || first_part.is_empty() {
            // not any language provide
            (None, trimmed)
        } else if let Some(highlight_start) = first_part.find('{') {
            // support attached line specs such as ```rust{1,3}
            (
                Some(first_part[..highlight_start].to_string()),
                &first_part[highlight_start..],
            )
        } else {
            // language + other mark
            (Some(first_part.to_string()), rest_part)
        };

        let highlighted_lines = Self::extract_highlight_spec(meta_part)
            .map(Self::parse_line_spec)
            .unwrap_or_default();

        FenceMeta {
            language,
            highlighted_lines,
        }
    }
}

// --- behavior ---

/// Replaces code blocks with syntect highlighted HTML.
pub struct SyntectRule;

impl CoreRule for SyntectRule {
    const NAMES: &'static [&'static str] = &["syntect"];

    fn run(root: &mut Node, md: &MarkdownIt) {
        let settings = load_syntect_settings(md);

        root.walk_mut(|node, _| {
            if let Some(snippet) = highlight_node(
                node.cast::<CodeBlock>(),
                node.cast::<CodeFence>(),
                &settings,
            ) {
                node.replace(snippet);
            }
        });
    }
}

fn highlight_node(
    code: Option<&CodeBlock>,
    fence: Option<&CodeFence>,
    settings: &SyntectSettings,
) -> Option<SyntectSnippet> {
    let (content, meta, lang_prefix) = if let Some(code) = code {
        (
            code.content.as_str(),
            FenceMeta {
                language: None,
                highlighted_lines: HashSet::new(),
            },
            "language-".to_owned(),
        )
    } else {
        let fence = fence?;
        (
            fence.content.as_str(),
            FenceMeta::parse_fence_meta(fence),
            fence.lang_prefix.clone(),
        )
    };
    let ss = &*SYNTAX_SET;
    let syntax = meta
        .language
        .as_deref()
        .and_then(|lang| ss.find_syntax_by_token(lang))
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    let options = HighlightOptions {
        prefix: settings.prefix,
        highlighted_lines: &meta.highlighted_lines,
    };

    let (html, code_class) = match settings.mode {
        SyntectMode::Inline => {
            let theme = resolve_theme(&THEME_SET, settings)
                .unwrap_or_else(|| panic!("unknown syntect theme: {}", settings.theme));
            (
                render_inline_html(content, ss, syntax, theme, &options)?,
                None,
            )
        }
        SyntectMode::Classed => (
            render_classed_html(content, ss, syntax, &options)?,
            Some(format!("{}code", settings.prefix)),
        ),
    };
    Some(SyntectSnippet {
        html,
        content: content.to_owned(),
        language: meta.language,
        lang_prefix,
        code_class,
    })
}

#[derive(Debug, Clone, Default)]
struct SharedSyntectSettings(Arc<RwLock<SyntectSettings>>);

/// Highlights code blocks in an explicitly executed document transform pipeline.
#[derive(Debug, Default)]
pub struct SyntectDocumentTransform {
    settings: SharedSyntectSettings,
}

impl DocumentTransform for SyntectDocumentTransform {
    const KEY: &'static str = "syntect";

    fn run(&self, document: &Document) -> EditBatch {
        let settings = self.settings.0.read().unwrap().clone();
        let mut edits = EditBatch::new();
        for event in document.events(document.root()) {
            let node = match event {
                StructuralEvent::Enter(node) | StructuralEvent::Leaf(node) => node,
                StructuralEvent::Exit(_) => continue,
            };
            if let Some(snippet) = highlight_node(
                node.cast::<CodeBlock>(),
                node.cast::<CodeFence>(),
                &settings,
            ) {
                edits.replace_value(node.id(), snippet);
            }
        }
        edits
    }
}

// --- public method ---

/// Add the syntect highlighting rule.
///
/// The rule will replace [`CodeBlock`] and [`CodeFence`] nodes with syntect rendered HTML snippets.
pub fn add(md: &mut MarkdownIt) {
    md.add_rule::<SyntectRule>();
    register_document_renderers(md);
}

/// Register syntax highlighting for an explicit arena-backed document pipeline.
/// Run [`MarkdownIt::run_document_transforms`] after parsing.
pub fn add_document(md: &mut MarkdownIt) {
    let settings = md
        .ext
        .get_or_insert_default::<SharedSyntectSettings>()
        .clone();
    md.add_document_transform_instance(SyntectDocumentTransform { settings });
    register_document_renderers(md);
}

fn register_document_renderers(md: &mut MarkdownIt) {
    md.add_document_renderer::<SyntectSnippet, _>("html", SyntectHtmlRenderer);
    md.add_document_renderer::<SyntectSnippet, _>("text", SyntectTextRenderer);
}

/// Return the names of all built-in themes available to this plugin.
///
/// Includes syntect's defaults plus the extra themes shipped with two-face.
pub fn available_themes() -> Vec<String> {
    let mut themes: Vec<String> = THEME_SET.theme_names().map(str::to_owned).collect();
    themes.sort();
    themes
}

/// Set the theme used for syntax highlighting.
///
/// The names should match one of returned by [`available_themes`].
/// If not, it will panic.
pub fn set_theme(md: &mut MarkdownIt, theme: impl Into<String>) {
    update_syntect_settings(md, |settings| settings.theme = theme.into());
}

/// switch to stylesheet-based highlighting mode with default `syntect-` prefix.
///
/// In this mode, rendered code will use CSS class instead of inline styles.
/// Use [`theme_css`] to generate CSS for the selected theme.
///
/// ```rust
/// let mut md = markdown_it::MarkdownIt::empty();
/// markdown_it::plugins::cmark::add(&mut md);
/// markdown_it::plugins::extra::syntect::add(&mut md);
/// markdown_it::plugins::extra::syntect::set_to_classed(&mut md);
///
/// let css = markdown_it::plugins::extra::syntect::theme_css(&mut md);
/// assert!(css.is_some())
/// ```
pub fn set_to_classed(md: &mut MarkdownIt) {
    set_to_classed_with_prefix(md, "syntect-");
}

/// Switch to stylesheet-based highlighting with a custom class prefix.
pub fn set_to_classed_with_prefix(md: &mut MarkdownIt, prefix: &'static str) {
    update_syntect_settings(md, |settings| {
        settings.mode = SyntectMode::Classed;
        settings.prefix = prefix;
    });
}

/// Set the class prefix used for line highlighting and classed mode.
pub fn set_prefix(md: &mut MarkdownIt, prefix: &'static str) {
    update_syntect_settings(md, |settings| settings.prefix = prefix);
}

/// Generate CSS for selected built-in theme
///
/// # Panics
///
/// Panics if the configured theme not found in built-in themes
pub fn theme_css(md: &MarkdownIt) -> Option<String> {
    let settings = load_syntect_settings(md);
    let theme = resolve_theme(&THEME_SET, &settings)
        .unwrap_or_else(|| panic!("unknown syntect theme: {}", settings.theme));

    match settings.mode {
        SyntectMode::Inline => None,
        SyntectMode::Classed => css_for_theme_with_class_style(
            theme,
            ClassStyle::SpacedPrefixed {
                prefix: settings.prefix,
            },
        )
        .ok(),
    }
}

// --- helper method ---

fn load_syntect_settings(md: &MarkdownIt) -> SyntectSettings {
    md.ext
        .get::<SharedSyntectSettings>()
        .map(|settings| settings.0.read().unwrap().clone())
        .unwrap_or_default()
}

fn update_syntect_settings(md: &mut MarkdownIt, f: impl FnOnce(&mut SyntectSettings)) {
    let settings = md.ext.get_or_insert_default::<SharedSyntectSettings>();
    f(&mut settings.0.write().unwrap());
}

fn resolve_theme<'a>(themes: &'a LazyThemeSet, settings: &SyntectSettings) -> Option<&'a Theme> {
    themes.get(settings.theme.as_str())
}

fn render_inline_html(
    content: &str,
    ss: &SyntaxSet,
    syntax: &SyntaxReference,
    theme: &Theme,
    options: &HighlightOptions<'_>,
) -> Option<String> {
    let mut highlighter = HighlightLines::new(syntax, theme);
    let bg = theme
        .settings
        .background
        .unwrap_or(syntect::highlighting::Color::WHITE);

    let mut html = String::new();

    // it looks like `<span class="syntect-line [syntect-line-highlighted]" style="...">{code}</span>`
    for (idx, line) in LinesWithEndings::from(content).enumerate() {
        let line_no = idx + 1;
        let regions = highlighter.highlight_line(line, ss).ok()?;

        // use syntect process code
        let mut line_html = String::new();
        append_highlighted_html_for_styled_line(
            &regions[..],
            IncludeBackground::IfDifferent(bg),
            &mut line_html,
        )
        .ok()?;

        // splicing HTML
        html.push_str("<span class=\"");
        html.push_str(options.prefix);
        html.push_str("line");
        if options.highlighted_lines.contains(&line_no) {
            // mark as highlighted line. you may need to add styles to this class yourself
            html.push(' ');
            html.push_str(options.prefix);
            html.push_str("line-highlighted");
        }
        html.push_str("\">");
        html.push_str(&line_html);
        html.push_str("</span>");
    }

    Some(html)
}

fn render_classed_html(
    content: &str,
    ss: &SyntaxSet,
    syntax: &SyntaxReference,
    options: &HighlightOptions<'_>,
) -> Option<String> {
    let mut parse_state = ParseState::new(syntax);
    let mut scope_stack = ScopeStack::new();

    // splicing HTML
    let mut html = String::new();

    for (idx, line) in LinesWithEndings::from(content).enumerate() {
        let line_no = idx + 1;
        let active_scopes = scope_stack.scopes.clone();

        // it looks like `<span class="syntect-line [syntect-line-highlighted]">`
        html.push_str("<span class=\"");
        html.push_str(options.prefix);
        html.push_str("line");
        if options.highlighted_lines.contains(&line_no) {
            html.push(' ');
            html.push_str(options.prefix);
            html.push_str("line-highlighted");
        }
        html.push_str("\">");

        // too complex here

        // reopen the scope
        reopen_scopes(&mut html, &active_scopes, options.prefix);

        // use syntect process the line
        let ops = parse_state.parse_line(line, ss).ok()?;
        let (line_html, _) = line_tokens_to_classed_spans(
            line,
            ops.as_slice(),
            ClassStyle::SpacedPrefixed {
                prefix: options.prefix,
            },
            &mut scope_stack,
        )
        .ok()?;
        html.push_str(&line_html);

        // close all scope <span>
        close_n_spans(&mut html, scope_stack.scopes.len());

        // close the <span> we added
        html.push_str("</span>");
    }

    Some(html)
}

fn reopen_scopes(html: &mut String, scopes: &[Scope], prefix: &'static str) {
    for &scope in scopes {
        html.push_str("<span class=\"");
        push_scope_classes(html, scope, prefix);
        html.push_str("\">");
    }
}

fn close_n_spans(html: &mut String, count: usize) {
    for _ in 0..count {
        html.push_str("</span>");
    }
}

fn push_scope_classes(html: &mut String, scope: Scope, prefix: &'static str) {
    let scope_text = scope.to_string();
    for (idx, atom) in scope_text.split('.').enumerate() {
        if idx != 0 {
            html.push(' ');
        }
        html.push_str(prefix);
        html.push_str(atom);
    }
}

#[cfg(test)]
mod test {
    use crate::*;

    fn parser() -> MarkdownIt {
        let mut md = MarkdownIt::empty();
        plugins::cmark::add(&mut md);
        plugins::extra::syntect::add(&mut md);
        md
    }

    #[test]
    fn render_options_override_syntect_lang_prefix() {
        let ast = parser().parse("```rust\nfn main() {}\n```");
        let html = ast.render_with(&RenderOptions {
            lang_prefix: Some("lang-".into()),
            ..Default::default()
        });

        assert!(html.contains(r#"class="lang-rust""#));
        assert!(!html.contains("language-rust"));
    }

    #[test]
    fn highlights_indented_code_blocks() {
        let html = parser().parse("    plain code\n").render();

        assert!(html.contains(r#"class="syntect-line""#));
    }
}
