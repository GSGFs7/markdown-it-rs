//! Syntax highlighting for code blocks

use std::collections::HashSet;

use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::html::{
    append_highlighted_html_for_styled_line,
    css_for_theme_with_class_style,
    line_tokens_to_classed_spans,
    ClassStyle,
    IncludeBackground,
};
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

use crate::common::utils::{escape_html, unescape_all};
use crate::parser::core::CoreRule;
use crate::parser::extset::MarkdownItExt;
use crate::plugins::cmark::block::code::CodeBlock;
use crate::plugins::cmark::block::fence::CodeFence;
use crate::{MarkdownIt, Node, NodeValue, Renderer};

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

struct FenceMeta {
    language: Option<String>,
    // highlight some lines
    // it seems like:
    //
    // ```rust {1, 3-4}
    // fn main() {
    //     print!("Hello world!");
    //     Ok(())
    // }
    // ```
    highlighted_lines: HashSet<usize>,
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
            let mut highlighted_lines = HashSet::new();

            if let Some(data) = node.cast::<CodeBlock>() {
                content = Some(data.content.as_str());
            } else if let Some(data) = node.cast::<CodeFence>() {
                let meta = FenceMeta::parse_fence_meta(data);
                language = meta.language;
                highlighted_lines = meta.highlighted_lines;
                content = Some(data.content.as_str());
                lang_prefix = Some(data.lang_prefix);
            }

            if let Some(content) = content {
                let syntax = language
                    .as_deref()
                    .and_then(|lang| ss.find_syntax_by_token(lang))
                    .unwrap_or_else(|| ss.find_syntax_plain_text());

                let html = match settings.mode {
                    SyntectMode::Inline => render_inline_html(
                        content,
                        &ss,
                        syntax,
                        theme,
                        language.as_deref(),
                        lang_prefix.unwrap_or("language-"),
                        &highlighted_lines,
                    ),
                    SyntectMode::Classed { prefix } => render_classed_html(
                        content,
                        &ss,
                        syntax,
                        language.as_deref(),
                        lang_prefix.unwrap_or("language-"),
                        prefix,
                        &highlighted_lines,
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
    set_to_classed_with_prefix(md, "syntect-");
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

fn resolve_theme<'a>(themes: &'a ThemeSet, settings: &SyntectSettings) -> Option<&'a Theme> {
    themes.themes.get(settings.theme.as_str())
}

fn render_inline_html(
    content: &str,
    ss: &SyntaxSet,
    syntax: &SyntaxReference,
    theme: &Theme,
    language: Option<&str>,
    lang_prefix: &'static str,
    highlight_lines: &HashSet<usize>,
) -> Option<String> {
    let mut highlighter = HighlightLines::new(syntax, theme);
    let bg = theme
        .settings
        .background
        .unwrap_or(syntect::highlighting::Color::WHITE);
    let mut class_attr = String::new();
    if let Some(lang) = language {
        if !lang.is_empty() {
            class_attr.push_str(lang_prefix);
            class_attr.push_str(lang);
        }
    }

    // it seems `<pre><code>` or `<pre><code class="language-{lang}">`
    let mut html = String::from("<pre><code");
    if !class_attr.is_empty() {
        html.push_str(" class=\"");
        html.push_str(&escape_html(&class_attr));
        html.push('"');
    }
    html.push('>');

    // it seems `<span class="syntect-line [syntect-line-highlighted]" style="...">{code}</span>`
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
        html.push_str("<span class=\"syntect-line");
        if highlight_lines.contains(&line_no) {
            // mark as highlighted line. you may need to add styles to this class yourself
            html.push_str(" syntect-line-highlighted");
        }
        html.push_str("\">");
        html.push_str(&line_html);
        html.push_str("</span>");
    }

    // close html
    html.push_str("</code></pre>");

    Some(html)
}

fn render_classed_html(
    content: &str,
    ss: &SyntaxSet,
    syntax: &SyntaxReference,
    language: Option<&str>,
    lang_prefix: &'static str,
    prefix: &'static str,
    highlighted_lines: &HashSet<usize>,
) -> Option<String> {
    let mut parse_state = ParseState::new(syntax);
    let mut scope_stack = ScopeStack::new();

    let mut class_attr = format!("{prefix}code");
    if let Some(lang) = language {
        if !lang.is_empty() {
            class_attr.push(' ');
            class_attr.push_str(lang_prefix);
            class_attr.push_str(lang);
        }
    }

    // splicing HTML
    // head, it seems `<pre><code class="syntect-code language-rust">`
    let mut html = String::from("<pre><code class=\"");
    html.push_str(&escape_html(&class_attr));
    html.push_str("\">");

    for (idx, line) in LinesWithEndings::from(content).enumerate() {
        let line_no = idx + 1;
        let active_scopes = scope_stack.scopes.clone();

        // it seems `<span class="syntect-line [syntect-line-highlighted]">`
        html.push_str("<span class=\"");
        html.push_str(prefix);
        html.push_str("line");
        if highlighted_lines.contains(&line_no) {
            html.push(' ');
            html.push_str(prefix);
            html.push_str("line-highlighted");
        }
        html.push_str("\">");

        // too complex here

        // reopen the scope
        reopen_scopes(&mut html, &active_scopes, prefix);

        // use syntect process the line
        let ops = parse_state.parse_line(line, ss).ok()?;
        let (line_html, _) = line_tokens_to_classed_spans(
            line,
            ops.as_slice(),
            ClassStyle::SpacedPrefixed { prefix },
            &mut scope_stack,
        )
        .ok()?;
        html.push_str(&line_html);

        // close all scope <span>
        close_n_spans(&mut html, scope_stack.scopes.len());

        // close the <span> we added
        html.push_str("</span>");
    }

    // close
    html.push_str("</code></pre>");

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
