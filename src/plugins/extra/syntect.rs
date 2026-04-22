//! Syntax highlighting for code blocks

use syntect::{
    highlighting::ThemeSet,
    html::{
        css_for_theme_with_class_style, highlighted_html_for_string, ClassStyle,
        ClassedHTMLGenerator,
    },
    parsing::{SyntaxReference, SyntaxSet},
    util::LinesWithEndings,
};

use crate::{
    common::utils::{escape_html, unescape_all},
    parser::{core::CoreRule, extset::MarkdownItExt},
    plugins::cmark::block::{code::CodeBlock, fence::CodeFence},
    MarkdownIt, Node, NodeValue, Renderer,
};

// --- render ---

#[derive(Debug)]
pub struct SyntectSnippet {
    pub html: String,
}

impl NodeValue for SyntectSnippet {
    fn render(&self, _: &Node, fmt: &mut dyn Renderer) {
        fmt.text_raw(&self.html);
    }
}

// --- setting ---

#[derive(Debug, Clone, Copy)]
enum SyntectMode {
    Inline,
    Classed { prefix: &'static str },
}

#[derive(Debug, Clone)]
struct SyntectSettings {
    theme: String,
    mode: SyntectMode,
}

impl MarkdownItExt for SyntectSettings {}

impl Default for SyntectSettings {
    fn default() -> Self {
        Self {
            theme: "InspiredGitHub".to_owned(),
            mode: SyntectMode::Inline,
        }
    }
}

// --- behavior ---

pub struct SyntectRule;

impl CoreRule for SyntectRule {
    fn run(root: &mut Node, md: &MarkdownIt) {
        let ss = SyntaxSet::load_defaults_newlines();
        let ts = ThemeSet::load_defaults();
        let settings = load_syntect_settings(md);
        // why panic here? avoid change original behavior
        // `let theme = &ts.themes[md.ext.get::<SyntectSettings>().copied().unwrap_or_default().0];`
        let theme = resolve_theme(&ts, &settings)
            .unwrap_or_else(|| panic!("unknown syntect theme: {}", settings.theme));

        root.walk_mut(|node, _| {
            let mut content = None;
            let mut language = None::<String>;
            let mut lang_prefix = None::<&'static str>;

            if let Some(data) = node.cast::<CodeBlock>() {
                content = Some(data.content.as_str());
            } else if let Some(data) = node.cast::<CodeFence>() {
                language = extract_fence_language(data);
                content = Some(data.content.as_str());
                lang_prefix = Some(data.lang_prefix);
            }

            if let Some(content) = content {
                let syntax = language
                    .as_deref()
                    .and_then(|lang| ss.find_syntax_by_token(lang))
                    .unwrap_or_else(|| ss.find_syntax_plain_text());

                let html = match settings.mode {
                    SyntectMode::Inline => {
                        highlighted_html_for_string(content, &ss, syntax, theme).ok()
                    }
                    SyntectMode::Classed { prefix } => render_classed_html(
                        content,
                        &ss,
                        syntax,
                        language.as_deref(),
                        lang_prefix.unwrap_or("language-"),
                        prefix,
                    ),
                };

                if let Some(html) = html {
                    node.replace(SyntectSnippet { html });
                }
            }
        });
    }
}

// --- public method ---

pub fn add(md: &mut MarkdownIt) {
    md.add_rule::<SyntectRule>();
}

pub fn available_themes() -> Vec<String> {
    let ts = ThemeSet::load_defaults();
    let mut themes: Vec<_> = ts.themes.keys().cloned().collect();
    themes.sort();
    themes
}

pub fn set_theme(md: &mut MarkdownIt, theme: impl Into<String>) {
    update_syntect_settings(md, |settings| settings.theme = theme.into());
}

/// switch to classed mode
///
/// use your own css file
///
/// example:
///
/// ```rust
/// let mut md = markdown_it::MarkdownIt::new();
/// markdown_it::plugins::cmark::add(&mut md);
/// markdown_it::plugins::extra::syntect::add(&mut md);
/// markdown_it::plugins::extra::syntect::set_to_classed(&mut md);
/// ```
pub fn set_to_classed(md: &mut MarkdownIt) {
    set_to_classed_with_prefix(md, "syn-");
}

pub fn set_to_classed_with_prefix(md: &mut MarkdownIt, prefix: &'static str) {
    update_syntect_settings(md, |settings| {
        settings.mode = SyntectMode::Classed { prefix };
    });
}

pub fn theme_css(md: &MarkdownIt) -> Option<String> {
    let ts = ThemeSet::load_defaults();
    let settings = load_syntect_settings(md);
    let theme = resolve_theme(&ts, &settings)
        .unwrap_or_else(|| panic!("unknown syntect theme: {}", settings.theme));

    match settings.mode {
        SyntectMode::Inline => None,
        SyntectMode::Classed { prefix } => {
            css_for_theme_with_class_style(theme, ClassStyle::SpacedPrefixed { prefix }).ok()
        }
    }
}

// --- helper method ---

fn load_syntect_settings(md: &MarkdownIt) -> SyntectSettings {
    md.ext.get::<SyntectSettings>().cloned().unwrap_or_default()
}

fn update_syntect_settings(md: &mut MarkdownIt, f: impl FnOnce(&mut SyntectSettings)) {
    let mut settings = md.ext.remove::<SyntectSettings>().unwrap_or_default();
    f(&mut settings);
    md.ext.insert(settings);
}

fn resolve_theme<'a>(
    themes: &'a ThemeSet,
    settings: &SyntectSettings,
) -> Option<&'a syntect::highlighting::Theme> {
    themes.themes.get(settings.theme.as_str())
}

fn extract_fence_language(data: &CodeFence) -> Option<String> {
    let info = unescape_all(&data.info);
    info.split_whitespace().next().map(str::to_owned)
}

fn render_classed_html(
    content: &str,
    ss: &SyntaxSet,
    syntax: &SyntaxReference,
    language: Option<&str>,
    lang_prefix: &'static str,
    prefix: &'static str,
) -> Option<String> {
    let mut generator = ClassedHTMLGenerator::new_with_class_style(
        syntax,
        ss,
        ClassStyle::SpacedPrefixed { prefix },
    );
    for line in LinesWithEndings::from(content) {
        generator
            .parse_html_for_line_which_includes_newline(line)
            .ok()?
    }

    let highlight = generator.finalize();
    let mut class_attr = format!("{prefix}code");
    if let Some(lang) = language {
        if !lang.is_empty() {
            class_attr.push(' ');
            class_attr.push_str(lang_prefix);
            class_attr.push_str(lang);
        }
    }

    let mut html = String::from("<pre><code class=\"");
    html.push_str(&escape_html(&class_attr));
    html.push_str("\">");
    html.push_str(&highlight);
    html.push_str("</code></pre>");

    Some(html)
}
