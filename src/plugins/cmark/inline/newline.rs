//! Line breaks
//!
//! Processes EOL (`\n`, soft and hard breaks).
//!
//!  - <https://spec.commonmark.org/0.30/#hard-line-breaks>
//!  - <https://spec.commonmark.org/0.30/#soft-line-breaks>
use crate::MarkdownIt;
use crate::document::{NodeId, NodeRef};
use crate::parser::inline::{InlineRule, InlineState};
use crate::render::{NodeRenderer, PlainTextBreakRenderer, RenderContext, write_html_self_close};

#[derive(Debug)]
pub struct Hardbreak;

struct HardbreakRenderer;

impl NodeRenderer<Hardbreak> for HardbreakRenderer {
    fn render(
        &self,
        _: NodeRef<'_>,
        _: &Hardbreak,
        context: &mut RenderContext<'_>,
        output: &mut crate::Writer,
    ) {
        write_html_self_close(output, "br", &[], context.options().xhtml_out);
        output.write_str("\n");
    }
}

#[derive(Debug)]
pub struct Softbreak;

struct SoftbreakRenderer;

impl NodeRenderer<Softbreak> for SoftbreakRenderer {
    fn render(
        &self,
        _: NodeRef<'_>,
        _: &Softbreak,
        context: &mut RenderContext<'_>,
        output: &mut crate::Writer,
    ) {
        if context.options().breaks {
            write_html_self_close(output, "br", &[], context.options().xhtml_out);
        }
        // A decoded entity may already end in a newline; write ours directly
        // instead of `cr`, which would coalesce the two.
        output.write_str("\n");
    }
}

pub fn add(md: &mut MarkdownIt) {
    md.inline.add_rule::<NewlineScanner>();
    md.add_renderer::<Hardbreak, _>("html", HardbreakRenderer);
    md.add_renderer::<Softbreak, _>("html", SoftbreakRenderer);
    md.add_renderer::<Hardbreak, _>("text", PlainTextBreakRenderer);
    md.add_renderer::<Softbreak, _>("text", PlainTextBreakRenderer);
}

#[doc(hidden)]
pub struct NewlineScanner;

impl InlineRule for NewlineScanner {
    const MARKER: char = '\n';
    const NAMES: &'static [&'static str] = &["newline"];

    fn check(context: &mut InlineState<'_>) -> Option<usize> {
        // check rule is required because run() modifies trailing text
        if context.remaining().starts_with('\n') {
            Some(1)
        } else {
            None
        }
    }

    fn run(state: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        let mut chars = state.src[state.pos..state.pos_max].chars();
        if chars.next()? != '\n' {
            return None;
        }

        // skip leading whitespaces from next line
        let end = state.pos + 1 + chars.take_while(|ch| matches!(ch, ' ' | '\t')).count();
        let spaces = state
            .trailing_text()
            .bytes()
            .rev()
            .take_while(|&ch| ch == b' ')
            .count();
        state.pop_trailing_text(spaces);

        // '  \n' -> hardbreak
        let node = if spaces >= 2 {
            state.document.create_node(Hardbreak)
        } else {
            state.document.create_node(Softbreak)
        };
        state.pos -= spaces; // backtrack to include tail in source maps
        Some((Some(node), end - state.pos))
    }
}

#[cfg(test)]
mod test {
    use crate::*;

    fn parser() -> MarkdownIt {
        let mut md = MarkdownIt::empty();
        plugins::cmark::add(&mut md);
        md
    }

    #[test]
    fn renders_softbreak_as_newline_by_default() {
        let md = parser();
        let ast = md.parse("hello\nworld");
        assert_eq!(md.render_html(&ast), "<p>hello\nworld</p>\n");
    }

    #[test]
    fn softbreak_preserves_newlines_decoded_from_entities() {
        let mut md = parser();
        for (source, expected) in [
            ("foo&#10;\nbar", "<p>foo\n\nbar</p>\n"),
            (
                "foo&#10;&#10;\n\\a~9中\rbar\n",
                "<p>foo\n\n\n\\a~9中\nbar</p>\n",
            ),
            (
                "foo&#10;\r*_ é*:`\t0|bar\n",
                "<p>foo\n\n<em>_ é</em>:`\t0|bar</p>\n",
            ),
        ] {
            assert_eq!(md.render(source), expected, "{source:?}");
        }
        md.render_options.breaks = true;
        assert_eq!(md.render("foo&#10;\nbar"), "<p>foo\n<br>\nbar</p>\n");
    }

    #[test]
    fn breaks_respects_xhtml_out() {
        let md = parser();
        let ast = md.parse("hello\nworld");

        assert_eq!(
            md.renderers.render(
                &ast,
                "html",
                &RenderOptions {
                    breaks: true,
                    xhtml_out: true,
                    ..Default::default()
                }
            ),
            "<p>hello<br />\nworld</p>\n"
        );
    }

    #[test]
    fn breaks_match_expected_output() {
        let mut md = MarkdownIt::empty();
        plugins::cmark::block::paragraph::add(&mut md);
        plugins::cmark::inline::newline::add(&mut md);

        for (source, expected) in [
            ("a  \n \tb", "<p>a<br>\nb</p>\n"),
            ("a\nb", "<p>a\nb</p>\n"),
            ("a  \nb", "<p>a<br>\nb</p>\n"),
        ] {
            assert_eq!(md.render(source), expected, "{source}");
        }
    }
}
