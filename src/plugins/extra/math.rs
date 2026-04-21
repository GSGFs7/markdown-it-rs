// reference to exist CodeFence & CodeSpan rule in the code base

use crate::{
    parser::{
        block::{BlockRule, BlockState},
        inline::{InlineRule, InlineState},
    },
    MarkdownIt, Node, NodeValue, Renderer,
};

#[derive(Debug)]
struct MathBlock {
    pub content: String,
}

impl NodeValue for MathBlock {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        #[cfg(not(feature = "katex"))]
        {
            let mut attrs = node.attrs.clone();
            attrs.push(("class", "math-block".into()));

            fmt.cr();
            fmt.open("div", &attrs);
            fmt.text(&self.content);
            fmt.close("div");
            fmt.cr();
        }

        #[cfg(feature = "katex")]
        {
            let mut attrs = node.attrs.clone();
            attrs.push(("class", "math-block".into()));
            fmt.cr();
            fmt.open("div", &attrs);

            // render katex
            let ctx = katex::KatexContext::default();
            let setting = katex::Settings::default();
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
            attrs.push(("class", "math-inline".into()));
            fmt.open("span", &attrs);
            fmt.text(&self.content);
            fmt.close("span");
        }

        #[cfg(feature = "katex")]
        {
            let mut attrs = node.attrs.clone();
            attrs.push(("class", "math-inline".into()));
            fmt.open("span", &attrs);

            let ctx = katex::KatexContext::default();
            let mut setting = katex::Settings::default();
            setting.display_mode = false;
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
