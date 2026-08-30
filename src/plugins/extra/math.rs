// reference to exist CodeFence & CodeSpan rule in the code base

use crate::parser::block::{BlockRule, BlockState};
use crate::parser::inline::{InlineRule, InlineState};
use crate::{MarkdownIt, Node, NodeValue, Renderer};

#[derive(Debug)]
struct MathBlock {
    pub content: String,
}

impl NodeValue for MathBlock {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        #[cfg(not(feature = "katex"))]
        {
            let mut attrs = node.attrs.clone();
            attrs.push(("class".into(), "math-block".into()));

            fmt.cr();
            fmt.open("div", &attrs);
            fmt.text(&self.content);
            fmt.close("div");
            fmt.cr();
        }

        #[cfg(feature = "katex")]
        {
            let mut attrs = node.attrs.clone();
            attrs.push(("class".into(), "math-block".into()));
            fmt.cr();
            fmt.open("div", &attrs);

            // render katex
            let ctx = katex::KatexContext::default();
            let setting = katex::Settings::builder().display_mode(true).build();
            match katex::render_to_string(&ctx, &self.content, &setting) {
                Ok(html) => fmt.text_raw(&html),
                Err(_) => fmt.text(&self.content),
            }

            fmt.close("div");
            fmt.cr();
        }
    }
}

#[doc(hidden)]
pub struct MathBlockScanner;

impl MathBlockScanner {
    fn get_header<'a>(state: &'a mut BlockState) -> Option<&'a str> {
        if state.line_indent(state.line) >= state.md.max_indent {
            return None;
        }

        let line = state.get_line(state.line);
        let trimmed = line.trim_end();
        if trimmed != "$$" {
            return None;
        }

        Some(trimmed)
    }
}

impl BlockRule for MathBlockScanner {
    const NAMES: &'static [&'static str] = &["math_block"];

    fn check(state: &mut BlockState) -> Option<()> {
        Self::get_header(state).map(|_| ())
    }

    fn run(state: &mut BlockState) -> Option<(Node, usize)> {
        Self::get_header(state)?;

        let mut next_line = state.line;
        let mut have_end_marker = false;

        loop {
            next_line += 1;
            if next_line >= state.line_max {
                break;
            }

            let line = state.get_line(next_line);
            let trimmed = line.trim();
            if !line.is_empty() && state.line_indent(next_line) < 0 {
                break;
            }
            if trimmed == "$$" {
                have_end_marker = true;
                break;
            }
        }

        let indent = state.line_offsets[state.line].indent_nonspace;
        let (content, _) = state.get_lines(state.line + 1, next_line, indent as usize, false);

        Some((
            Node::new(MathBlock {
                content: content.trim().to_owned(),
            }),
            next_line - state.line + if have_end_marker { 1 } else { 0 },
        ))
    }
}

#[derive(Debug)]
struct MathInline {
    pub content: String,
}

impl NodeValue for MathInline {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        #[cfg(not(feature = "katex"))]
        {
            let mut attrs = node.attrs.clone();
            attrs.push(("class".into(), "math-inline".into()));
            fmt.open("span", &attrs);
            fmt.text(&self.content);
            fmt.close("span");
        }

        #[cfg(feature = "katex")]
        {
            let mut attrs = node.attrs.clone();
            attrs.push(("class".into(), "math-inline".into()));
            fmt.open("span", &attrs);

            let ctx = katex::KatexContext::default();
            let setting = katex::Settings::builder().display_mode(false).build();
            match katex::render_to_string(&ctx, &self.content, &setting) {
                Ok(html) => fmt.text_raw(&html),
                Err(_) => fmt.text(&self.content),
            }

            fmt.close("span");
        }
    }
}

#[doc(hidden)]
pub struct MathInlineScanner;

impl InlineRule for MathInlineScanner {
    const MARKER: char = '$';
    const NAMES: &'static [&'static str] = &["math_inline"];

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let mut char = state.src[state.pos..state.pos_max].chars();
        if char.next()? != '$' {
            return None;
        }

        let mut pos = state.pos + 1;
        while pos < state.pos_max {
            if state.src.as_bytes()[pos] == b'$' {
                if state.src.as_bytes()[pos - 1] == b'\\' {
                    pos += 1;
                    continue;
                }

                let content = &state.src[state.pos + 1..pos];
                if content.is_empty() {
                    pos += 1;
                    continue;
                }

                // $ something$ or $something $
                if content.starts_with(|c: char| c.is_whitespace())
                    || content.ends_with(|c: char| c.is_whitespace())
                {
                    pos += 1;
                    continue;
                }

                // $20
                if pos + 1 < state.pos_max && state.src.as_bytes()[pos + 1].is_ascii_digit() {
                    pos += 1;
                    continue;
                }

                let mut node = Node::new(MathInline {
                    content: content.to_owned(),
                });
                node.srcmap = state.get_map(state.pos, pos + 1);
                return Some((node, pos - state.pos + 1));
            }

            pos += 1;
        }

        None
    }
}

pub fn add(md: &mut MarkdownIt) {
    md.block.add_rule::<MathBlockScanner>();
    md.inline.add_rule::<MathInlineScanner>();
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

        let node = md.parse(&(input.to_owned() + "\n"));
        node.walk(|node, _| assert!(node.srcmap.is_some()));

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

        let actual = normalize_katex_attrs(&node.render());
        let expected = normalize_katex_attrs(&output);
        assert_eq!(actual, expected);

        let _ = md.parse(input.trim_end());
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
