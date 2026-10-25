//! Code fence
//!
//! ` ```lang ` or `~~~lang`
//!
//! <https://spec.commonmark.org/0.30/#code-fence>
use crate::MarkdownIt;
use crate::common::utils::unescape_all;
use crate::document::{NodeId, NodeRef};
use crate::parser::block::{BlockRule, BlockState};
use crate::render::{
    NodeRenderer,
    RenderContext,
    write_html_close,
    write_html_open,
    write_html_text,
};

#[derive(Debug)]
pub struct CodeFence {
    pub info: String,
    pub marker: char,
    pub marker_len: usize,
    pub content: String,
    pub lang_prefix: String,
}

struct CodeFenceRenderer;

impl NodeRenderer<CodeFence> for CodeFenceRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        fence: &CodeFence,
        context: &mut RenderContext<'_>,
        output: &mut crate::Writer,
    ) {
        let info = unescape_all(&fence.info);
        let lang_name = info
            .split(|c: char| c == '\u{feff}' || (c.is_whitespace() && c != '\u{85}'))
            .find(|part| !part.is_empty())
            .unwrap_or("");
        let mut attrs = node.attrs().clone();
        let lang_prefix = context
            .options()
            .lang_prefix
            .as_deref()
            .unwrap_or(&fence.lang_prefix);
        if !lang_name.is_empty() {
            attrs.push(("class".into(), format!("{lang_prefix}{lang_name}")));
        }

        write_html_open(output, "pre", &[]);
        write_html_open(output, "code", &attrs);
        write_html_text(output, &fence.content);
        write_html_close(output, "code");
        write_html_close(output, "pre");
        context.cr(output);
    }
}

struct CodeFenceTextRenderer;

impl NodeRenderer<CodeFence> for CodeFenceTextRenderer {
    fn render(
        &self,
        _: NodeRef<'_>,
        fence: &CodeFence,
        context: &mut RenderContext<'_>,
        output: &mut crate::Writer,
    ) {
        context.cr(output);
        output.write_str(&fence.content);
        context.cr(output);
    }
}

#[derive(Debug, Clone)]
struct FenceSettings(String);
impl Default for FenceSettings {
    fn default() -> Self {
        Self("language-".to_string())
    }
}

pub fn add(md: &mut MarkdownIt) {
    md.block.add_rule::<FenceScanner>();
    md.add_renderer::<CodeFence, _>("html", CodeFenceRenderer);
    md.add_renderer::<CodeFence, _>("text", CodeFenceTextRenderer);
}

pub fn set_lang_prefix(md: &mut MarkdownIt, lang_prefix: impl Into<String>) {
    md.ext.insert(FenceSettings(lang_prefix.into()));
}

#[doc(hidden)]
pub struct FenceScanner;

fn get_header(line: &str, line_indent: i32, max_indent: i32) -> Option<(char, usize, &str)> {
    if line_indent >= max_indent {
        return None;
    }

    let mut chars = line.chars();

    let marker = chars.next()?;
    if marker != '~' && marker != '`' {
        return None;
    }

    // scan marker length
    let mut len = 1;
    while Some(marker) == chars.next() {
        len += 1;
    }

    if len < 3 {
        return None;
    }

    let params = &line[len..];

    if marker == '`' && params.contains(marker) {
        return None;
    }

    Some((marker, len, params))
}

impl FenceScanner {}

impl BlockRule for FenceScanner {
    const MARKERS: &'static [char] = &['`', '~'];
    const NAMES: &'static [&'static str] = &["fence"];
    fn check(state: &mut BlockState<'_>) -> Option<()> {
        get_header(
            state.get_line(state.line),
            state.line_indent(state.line),
            state.md.max_indent,
        )
        .map(|_| ())
    }

    fn run(state: &mut BlockState<'_>) -> Option<(Option<NodeId>, usize)> {
        let (marker, len, params) = {
            let line = state.get_line(state.line);
            let (marker, len, params) =
                get_header(line, state.line_indent(state.line), state.md.max_indent)?;
            (marker, len, params.to_owned())
        };

        let mut next_line = state.line;
        let mut have_end_marker = false;

        // search end of block
        'outer: loop {
            next_line += 1;
            if next_line >= state.line_max {
                // unclosed block should be autoclosed by end of document.
                // also block seems to be autoclosed by end of parent
                break;
            }

            let line = state.get_line(next_line);

            if !line.is_empty() && state.line_indent(next_line) < 0 {
                // non-empty line with negative indent should stop the list:
                // - ```
                //  test
                break;
            }

            let mut chars = line.chars().peekable();

            if Some(marker) != chars.next() {
                continue;
            }

            if state.line_indent(next_line) >= state.md.max_indent {
                continue;
            }

            // scan marker length
            let mut len_end = 1;
            while Some(&marker) == chars.peek() {
                chars.next();
                len_end += 1;
            }

            // closing code fence must be at least as long as the opening one
            if len_end < len {
                continue;
            }

            // make sure tail has spaces only
            loop {
                match chars.next() {
                    Some(' ' | '\t') => {}
                    Some(_) => continue 'outer,
                    None => {
                        have_end_marker = true;
                        break 'outer;
                    }
                }
            }
        }

        // If a fence has heading spaces, they should be removed from its inner block
        let indent = state.line_offsets[state.line].indent_nonspace;
        let keep_last_lf = state.line_offsets[next_line - 1].line_end < state.src.len();
        let (content, _) =
            state.get_lines(state.line + 1, next_line, indent as usize, keep_last_lf);

        let lang_prefix = state
            .md
            .ext
            .get::<FenceSettings>()
            .cloned()
            .unwrap_or_default()
            .0;
        let node = state.document.create_node(CodeFence {
            info: params,
            marker,
            marker_len: len,
            content,
            lang_prefix,
        });
        Some((
            Some(node),
            next_line - state.line + if have_end_marker { 1 } else { 0 },
        ))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn language_names_follow_javascript_whitespace_after_unescaping() {
        let mut md = crate::MarkdownIt::new();
        for marker in ["```", "~~~"] {
            for (info, language) in [
                ("js\u{feff}extra", "js"),
                ("\u{feff}js\u{feff}", "js"),
                ("&#xfeff;js&#xfeff;extra", "js"),
                ("js&#10;extra", "js"),
                ("js&#13;extra", "js"),
                ("js\u{85}extra", "js\u{85}extra"),
                ("\u{85}js\u{85}", "\u{85}js\u{85}"),
                ("js\u{200b}extra", "js\u{200b}extra"),
                ("js\u{180e}extra", "js\u{180e}extra"),
                ("\u{feff}", ""),
                ("&#xfeff;", ""),
                ("js&amp;x\u{feff}extra", "js&amp;x"),
            ] {
                let source = format!("{marker}{info}\ncode\u{feff}\n{marker}");
                let class = if language.is_empty() {
                    String::new()
                } else {
                    format!(" class=\"language-{language}\"")
                };
                assert_eq!(
                    md.render(&source),
                    format!("<pre><code{class}>code\u{feff}\n</code></pre>\n"),
                    "source: {source:?}"
                );
            }
        }
        for space in [
            '\t', '\u{b}', '\u{c}', ' ', '\u{a0}', '\u{1680}', '\u{2000}', '\u{200a}', '\u{2028}',
            '\u{2029}', '\u{202f}', '\u{205f}', '\u{3000}', '\u{feff}',
        ] {
            let source = format!("```{space}js{space}extra\ncode\n```");
            assert_eq!(
                md.render(&source),
                "<pre><code class=\"language-js\">code\n</code></pre>\n"
            );
        }
        md.render_options.lang_prefix = Some("".to_owned());
        assert_eq!(
            md.render("```js\u{feff}extra\ncode\n```"),
            "<pre><code class=\"js\">code\n</code></pre>\n"
        );
    }

    #[test]
    fn fences_after_tight_paragraphs_do_not_insert_a_line_break() {
        let mut md = crate::MarkdownIt::new();
        for limit in [3, 100] {
            md.max_nesting = limit;
            for (source, expected) in [
                (
                    "- a中< (9#` `  ```\n  c\n  ```\n- d\n",
                    "<ul>\n<li>a中&lt; (9#<code> </code>  ```\nc<pre><code></code></pre>\n</li>\n<li>d</li>\n</ul>\n",
                ),
                (
                    "- a\n  ```\n  code\n  ```\n- d\n",
                    "<ul>\n<li>a<pre><code>code\n</code></pre>\n</li>\n<li>d</li>\n</ul>\n",
                ),
                (
                    "- ```\n  code\n  ```\n",
                    "<ul>\n<li>\n<pre><code>code\n</code></pre>\n</li>\n</ul>\n",
                ),
                (
                    "- a\n\n  ```\n  code\n  ```\n",
                    "<ul>\n<li>\n<p>a</p>\n<pre><code>code\n</code></pre>\n</li>\n</ul>\n",
                ),
            ] {
                assert_eq!(md.render(source), expected, "limit={limit}, {source:?}");
            }
        }
    }

    #[test]
    fn unclosed_fences_preserve_source_final_line_break() {
        let md = crate::MarkdownIt::new();
        for (source, expected) in [
            ("```\ntext", "<pre><code>text</code></pre>\n"),
            ("```\ntext\n", "<pre><code>text\n</code></pre>\n"),
            ("~~~\ntext", "<pre><code>text</code></pre>\n"),
            ("```\ntext\n```", "<pre><code>text\n</code></pre>\n"),
            ("```", "<pre><code></code></pre>\n"),
            ("```\n", "<pre><code></code></pre>\n"),
            ("```\r\ntext\r\n", "<pre><code>text\n</code></pre>\n"),
            (
                "> ```\n> text",
                "<blockquote>\n<pre><code>text</code></pre>\n</blockquote>\n",
            ),
            (
                "- ```\n  text",
                "<ul>\n<li>\n<pre><code>text</code></pre>\n</li>\n</ul>\n",
            ),
        ] {
            assert_eq!(md.render(source), expected, "{source:?}");
        }
    }
}
