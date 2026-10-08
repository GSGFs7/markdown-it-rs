// reference to exist CodeFence & CodeSpan rule in the code base

use crate::document::{NodeDraft, NodeRef};
use crate::parser::block::BlockRule;
use crate::parser::document_parser::{DocumentBlockState, DocumentInlineState};
use crate::parser::inline::InlineRule;
use crate::parser::inline::probe::{InlineProbeContext, InlineProbeKind, InlineProbeResult};
use crate::render::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    write_html_close,
    write_html_open,
    write_html_text,
};
use crate::{MarkdownIt, NodeValue};

#[derive(Debug)]
struct MathBlock {
    pub content: String,
}

impl NodeValue for MathBlock {}

#[doc(hidden)]
pub struct MathBlockScanner;

fn math_block_header(line: &str, indent: i32, max_indent: i32) -> Option<()> {
    (indent < max_indent && line.trim_end() == "$$").then_some(())
}

fn scan_math_block<'a>(
    line: usize,
    line_max: usize,
    get_line: impl Fn(usize) -> (&'a str, i32),
) -> (usize, usize) {
    let mut next_line = line + 1;
    while next_line < line_max {
        let (text, indent) = get_line(next_line);
        if !text.is_empty() && indent < 0 {
            break;
        }
        if text.trim() == "$$" {
            return (next_line, next_line - line + 1);
        }
        next_line += 1;
    }
    (next_line, next_line - line)
}

impl BlockRule for MathBlockScanner {
    const MARKERS: &'static [char] = &['$'];
    const NAMES: &'static [&'static str] = &["math_block"];

    fn check(state: &mut DocumentBlockState<'_>) -> Option<()> {
        math_block_header(
            state.get_line(state.line),
            state.line_indent(state.line),
            state.md.max_indent,
        )
    }

    fn run(state: &mut DocumentBlockState<'_>) -> Option<(NodeDraft, usize)> {
        <Self as BlockRule>::check(state)?;
        let (end, consumed) = scan_math_block(state.line, state.line_max, |line| {
            (state.get_line(line), state.line_indent(line))
        });
        let indent = state.line_offsets[state.line].indent_nonspace;
        let (content, _) = state.get_lines(state.line + 1, end, indent as usize, false);
        Some((
            NodeDraft::new(MathBlock {
                content: content.trim().to_owned(),
            }),
            consumed,
        ))
    }
}

#[derive(Debug)]
struct MathInline {
    pub content: String,
}

impl NodeValue for MathInline {}

#[doc(hidden)]
pub struct MathInlineScanner;

fn scan_math_inline(src: &str) -> Option<(&str, usize)> {
    if !src.starts_with('$') {
        return None;
    }
    for pos in 1..src.len() {
        if src.as_bytes()[pos] != b'$' || src.as_bytes()[pos - 1] == b'\\' {
            continue;
        }
        let content = &src[1..pos];
        if content.is_empty()
            || content.starts_with(char::is_whitespace)
            || content.ends_with(char::is_whitespace)
            || src.as_bytes().get(pos + 1).is_some_and(u8::is_ascii_digit)
        {
            continue;
        }
        return Some((content, pos + 1));
    }
    None
}

impl InlineRule for MathInlineScanner {
    const MARKER: char = '$';
    const NAMES: &'static [&'static str] = &["math_inline"];

    fn probe(context: &mut InlineProbeContext<'_>) -> InlineProbeResult {
        match scan_math_inline(context.remaining()) {
            Some((_, len)) => InlineProbeResult::Match {
                len,
                kind: InlineProbeKind::Token,
            },
            None => InlineProbeResult::NoMatch,
        }
    }

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
        let (content, consumed) = scan_math_inline(state.remaining())?;
        Some((
            Some(NodeDraft::new(MathInline {
                content: content.to_owned(),
            })),
            consumed,
        ))
    }
}

impl AsRef<str> for MathBlock {
    fn as_ref(&self) -> &str {
        &self.content
    }
}

impl AsRef<str> for MathInline {
    fn as_ref(&self) -> &str {
        &self.content
    }
}

struct MathDocumentRenderer {
    block: bool,
    text: bool,
}

impl<T: NodeValue + AsRef<str>> DocumentNodeRenderer<T> for MathDocumentRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        value: &T,
        context: &mut DocumentRenderContext<'_>,
        output: &mut crate::DocumentWriter,
    ) {
        if self.block {
            context.cr(output);
        }
        if self.text {
            output.write_str(value.as_ref());
        } else {
            let tag = if self.block { "div" } else { "span" };
            let mut attrs = node.attrs().clone();
            attrs.push((
                "class".into(),
                if self.block {
                    "math-block"
                } else {
                    "math-inline"
                }
                .into(),
            ));
            write_html_open(output, tag, &attrs);
            #[cfg(not(feature = "katex"))]
            write_html_text(output, value.as_ref());
            #[cfg(feature = "katex")]
            {
                // render katex
                let ctx = katex::KatexContext::default();
                let setting = katex::Settings::builder().display_mode(self.block).build();
                match katex::render_to_string(&ctx, value.as_ref(), &setting) {
                    Ok(html) => output.write_str(&html),
                    Err(_) => write_html_text(output, value.as_ref()),
                }
            }
            write_html_close(output, tag);
        }
        if self.block {
            context.cr(output);
        }
    }
}

pub fn add(md: &mut MarkdownIt) {
    md.block.add_rule::<MathBlockScanner>();
    md.inline.add_rule::<MathInlineScanner>();
    for (format, text) in [("html", false), ("text", true)] {
        md.add_document_renderer::<MathBlock, _>(
            format,
            MathDocumentRenderer { block: true, text },
        );
        md.add_document_renderer::<MathInline, _>(
            format,
            MathDocumentRenderer { block: false, text },
        );
    }
}

#[cfg(test)]
mod tests {
    use crate as markdown_it;

    fn run(input: &str, output: &str) {
        let output = if output.is_empty() {
            "".to_owned()
        } else {
            output.to_owned() + "\n"
        };

        let md = &mut markdown_it::MarkdownIt::empty();
        markdown_it::plugins::cmark::add(md);
        markdown_it::plugins::html::add(md);
        markdown_it::plugins::extra::math::add(md);

        let node = md.parse_document(&(input.to_owned() + "\n"));
        for event in node.events(node.root()) {
            assert!(event.node().srcmap().is_some());
        }

        // fix attrs order in katex
        fn normalize_katex_attrs(html: &str) -> String {
            let style_re = regex::Regex::new(r#"style="([^"]+)""#).unwrap();
            let html = style_re
                .replace_all(html, |caps: &regex::Captures| {
                    let mut styles: Vec<&str> = caps[1]
                        .split(';')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .collect();
                    styles.sort();
                    format!(r#"style="{}""#, styles.join("; ") + ";")
                })
                .into_owned();

            let math_re = regex::Regex::new(r#"<math ([^>]+)>"#).unwrap();
            math_re
                .replace_all(&html, |caps: &regex::Captures| {
                    let mut attrs: Vec<&str> = caps[1].split_whitespace().collect();
                    attrs.sort();
                    format!("<math {}>", attrs.join(" "))
                })
                .into_owned()
        }

        let actual = normalize_katex_attrs(&md.render_document(&node));
        let expected = normalize_katex_attrs(&output);
        assert_eq!(actual, expected);

        let _ = md.parse_document(input.trim_end());
    }

    #[test]
    #[cfg(not(feature = "katex"))]
    fn math_block_multiline() {
        let input = r#"$$
E=mc^2
$$"#;

        let output = r#"<div class="math-block">E=mc^2</div>"#;

        run(input, output);
    }

    #[test]
    #[cfg(feature = "katex")]
    fn math_block_multiline() {
        let input = r#"$$
E=mc^2
$$"#;

        let output = r#"<div class="math-block"><span class="katex-display"><span class="katex"><span class="katex-mathml"><math xmlns="http://www.w3.org/1998/Math/MathML" display="block"><semantics><mrow><mi>E</mi><mo>=</mo><mi>m</mi><msup><mi>c</mi><mn>2</mn></msup></mrow><annotation encoding="application/x-tex">E=mc^2</annotation></semantics></math></span><span class="katex-html" aria-hidden="true"><span class="base"><span class="strut" style="height:0.6833em;"></span><span class="mord mathnormal" style="margin-right:0.0576em;">E</span><span class="mspace" style="margin-right:0.2778em;"></span><span class="mrel">=</span><span class="mspace" style="margin-right:0.2778em;"></span></span><span class="base"><span class="strut" style="height:0.8641em;"></span><span class="mord mathnormal">m</span><span class="mord"><span class="mord mathnormal">c</span><span class="msupsub"><span class="vlist-t"><span class="vlist-r"><span class="vlist" style="height:0.8641em;"><span style="margin-right:0.05em; top:-3.113em;"><span class="pstrut" style="height:2.7em;"></span><span class="sizing reset-size6 size3 mtight"><span class="mord mtight">2</span></span></span></span></span></span></span></span></span></span></span></span></div>"#;

        run(input, output);
    }

    #[test]
    #[cfg(not(feature = "katex"))]
    fn math_block_with_empty_line() {
        let input = r#"$$

E=mc^2


$$"#;

        let output = r#"<div class="math-block">E=mc^2</div>"#;

        run(input, output);
    }

    #[test]
    #[cfg(feature = "katex")]
    fn math_block_with_empty_line() {
        let input = r#"$$

E=mc^2


$$"#;

        let output = r#"<div class="math-block"><span class="katex-display"><span class="katex"><span class="katex-mathml"><math xmlns="http://www.w3.org/1998/Math/MathML" display="block"><semantics><mrow><mi>E</mi><mo>=</mo><mi>m</mi><msup><mi>c</mi><mn>2</mn></msup></mrow><annotation encoding="application/x-tex">E=mc^2</annotation></semantics></math></span><span class="katex-html" aria-hidden="true"><span class="base"><span class="strut" style="height:0.6833em;"></span><span class="mord mathnormal" style="margin-right:0.0576em;">E</span><span class="mspace" style="margin-right:0.2778em;"></span><span class="mrel">=</span><span class="mspace" style="margin-right:0.2778em;"></span></span><span class="base"><span class="strut" style="height:0.8641em;"></span><span class="mord mathnormal">m</span><span class="mord"><span class="mord mathnormal">c</span><span class="msupsub"><span class="vlist-t"><span class="vlist-r"><span class="vlist" style="height:0.8641em;"><span style="margin-right:0.05em; top:-3.113em;"><span class="pstrut" style="height:2.7em;"></span><span class="sizing reset-size6 size3 mtight"><span class="mord mtight">2</span></span></span></span></span></span></span></span></span></span></span></span></div>"#;

        run(input, output);
    }

    #[test]
    #[cfg(not(feature = "katex"))]
    fn math_inline() {
        let input = r#"$E=mc^2$"#;

        let output = r#"<p><span class="math-inline">E=mc^2</span></p>"#;

        run(input, output);
    }

    #[test]
    #[cfg(feature = "katex")]
    fn math_inline() {
        let input = r#"$E=mc^2$"#;

        let output = r#"<p><span class="math-inline"><span class="katex"><span class="katex-mathml"><math xmlns="http://www.w3.org/1998/Math/MathML"><semantics><mrow><mi>E</mi><mo>=</mo><mi>m</mi><msup><mi>c</mi><mn>2</mn></msup></mrow><annotation encoding="application/x-tex">E=mc^2</annotation></semantics></math></span><span class="katex-html" aria-hidden="true"><span class="base"><span class="strut" style="height:0.6833em;"></span><span class="mord mathnormal" style="margin-right:0.0576em;">E</span><span class="mspace" style="margin-right:0.2778em;"></span><span class="mrel">=</span><span class="mspace" style="margin-right:0.2778em;"></span></span><span class="base"><span class="strut" style="height:0.8141em;"></span><span class="mord mathnormal">m</span><span class="mord"><span class="mord mathnormal">c</span><span class="msupsub"><span class="vlist-t"><span class="vlist-r"><span class="vlist" style="height:0.8141em;"><span style="margin-right:0.05em; top:-3.063em;"><span class="pstrut" style="height:2.7em;"></span><span class="sizing reset-size6 size3 mtight"><span class="mord mtight">2</span></span></span></span></span></span></span></span></span></span></span></span></p>"#;

        run(input, output);
    }

    #[test]
    #[cfg(not(feature = "katex"))]
    fn math_inline_mixed() {
        let input = r#"something$E=mc^2$something"#;

        let output = r#"<p>something<span class="math-inline">E=mc^2</span>something</p>"#;

        run(input, output);
    }

    #[test]
    #[cfg(feature = "katex")]
    fn math_inline_mixed() {
        let input = r#"something$E=mc^2$something"#;

        let output = r#"<p>something<span class="math-inline"><span class="katex"><span class="katex-mathml"><math xmlns="http://www.w3.org/1998/Math/MathML"><semantics><mrow><mi>E</mi><mo>=</mo><mi>m</mi><msup><mi>c</mi><mn>2</mn></msup></mrow><annotation encoding="application/x-tex">E=mc^2</annotation></semantics></math></span><span class="katex-html" aria-hidden="true"><span class="base"><span class="strut" style="height:0.6833em;"></span><span class="mord mathnormal" style="margin-right:0.0576em;">E</span><span class="mspace" style="margin-right:0.2778em;"></span><span class="mrel">=</span><span class="mspace" style="margin-right:0.2778em;"></span></span><span class="base"><span class="strut" style="height:0.8141em;"></span><span class="mord mathnormal">m</span><span class="mord"><span class="mord mathnormal">c</span><span class="msupsub"><span class="vlist-t"><span class="vlist-r"><span class="vlist" style="height:0.8141em;"><span style="margin-right:0.05em;top:-3.063em;"><span class="pstrut" style="height:2.7em;"></span><span class="sizing reset-size6 size3 mtight"><span class="mord mtight">2</span></span></span></span></span></span></span></span></span></span></span></span>something</p>"#;

        run(input, output);
    }

    #[test]
    fn math_inline_with_spaces_not_allowed() {
        let input = r#"$ E=mc^2 $"#;
        let output = r#"<p>$ E=mc^2 $</p>"#;
        run(input, output);

        let input = r#"$E=mc^2 $"#;
        let output = r#"<p>$E=mc^2 $</p>"#;
        run(input, output);

        let input = r#"$ E=mc^2$"#;
        let output = r#"<p>$ E=mc^2$</p>"#;
        run(input, output);
    }

    #[test]
    fn math_inline_with_digit_after_closing() {
        let input = r#"$10 to $20"#;
        let output = r#"<p>$10 to $20</p>"#;
        run(input, output);

        let input = r#"$E=mc^2$1"#;
        let output = r#"<p>$E=mc^2$1</p>"#;
        run(input, output);
    }
}
