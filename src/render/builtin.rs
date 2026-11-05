//! Built-in renderers shared by the supported output formats.

use super::html::{write_html_close, write_html_open};
use super::{NodeRenderer, RenderContext, Writer};
use crate::document::{Node, NodeValue, Root, StructuralEvent, Text, TextSpecial};

pub(crate) struct TransparentRenderer;

impl<T: NodeValue> NodeRenderer<T> for TransparentRenderer {
    fn render(&self, node: &Node, _: &T, context: &mut RenderContext<'_>, output: &mut Writer) {
        context.render_children(node.id(), output);
    }
}

pub(crate) struct EmptyRenderer;

impl<T: NodeValue> NodeRenderer<T> for EmptyRenderer {
    fn render(&self, _: &Node, _: &T, _: &mut RenderContext<'_>, _: &mut Writer) {}
}

pub(crate) struct HtmlTextRenderer;

impl NodeRenderer<Text> for HtmlTextRenderer {
    fn render(&self, _: &Node, value: &Text, _: &mut RenderContext<'_>, output: &mut Writer) {
        output.write_escaped_html(&value.content);
    }
}

impl NodeRenderer<TextSpecial> for HtmlTextRenderer {
    fn render(
        &self,
        _: &Node,
        value: &TextSpecial,
        _: &mut RenderContext<'_>,
        output: &mut Writer,
    ) {
        output.write_escaped_html(&value.content);
    }
}

pub(crate) struct PlainTextRenderer;

impl NodeRenderer<Text> for PlainTextRenderer {
    fn render(&self, _: &Node, value: &Text, _: &mut RenderContext<'_>, output: &mut Writer) {
        output.write_str(&value.content);
    }
}

impl NodeRenderer<TextSpecial> for PlainTextRenderer {
    fn render(
        &self,
        _: &Node,
        value: &TextSpecial,
        _: &mut RenderContext<'_>,
        output: &mut Writer,
    ) {
        output.write_str(&value.content);
    }
}

pub(crate) struct PlainTextBlockRenderer;

impl<T: NodeValue> NodeRenderer<T> for PlainTextBlockRenderer {
    fn render(&self, node: &Node, _: &T, context: &mut RenderContext<'_>, output: &mut Writer) {
        context.cr(output);
        context.render_children(node.id(), output);
        context.cr(output);
    }
}

pub(crate) struct PlainTextBreakRenderer;

impl<T: NodeValue> NodeRenderer<T> for PlainTextBreakRenderer {
    fn render(&self, _: &Node, _: &T, context: &mut RenderContext<'_>, output: &mut Writer) {
        context.cr(output);
    }
}

/// Diagnostic tree renderer for the built-in document root.
///
/// It owns the complete structural traversal so custom descendant payloads do
/// not need a per-type `"debug"` registration.
pub(crate) struct DebugTreeRenderer;

impl NodeRenderer<Root> for DebugTreeRenderer {
    // in "# heading *em*" it like:
    // container type=markdown_it::document::root::Root id=NodeId(0:0) srcmap=0..14 attrs=[]
    //   container type=markdown_it::plugins::cmark::block::heading::ATXHeading id=NodeId(1:0) srcmap=0..14 attrs=[]
    //     leaf type=markdown_it::document::text::Text id=NodeId(2:0) srcmap=2..10 attrs=[]
    //       container type=markdown_it::plugins::cmark::inline::emphasis::Em id=NodeId(3:0) srcmap=10..14 attrs=[]
    //         leaf type=markdown_it::document::text::Text id=NodeId(11..13)
    fn render(&self, node: &Node, _: &Root, context: &mut RenderContext<'_>, output: &mut Writer) {
        let mut depth = 0;
        for event in context.document().events(node.id()) {
            let (kind, current) = match event {
                StructuralEvent::Enter(current) => ("container", current),
                StructuralEvent::Leaf(current) => ("leaf", current),
                StructuralEvent::Exit(_) => {
                    depth -= 1;
                    continue;
                }
            };

            for _ in 0..depth {
                output.write_str("  ");
            }
            write!(
                output,
                "{kind} type={} id={:?}",
                current.name(),
                current.id()
            );
            #[cfg(feature = "sourcemap")]
            {
                output.write_str(" srcmap=");
                if let Some(srcmap) = current.srcmap() {
                    let (start, end) = srcmap.get_byte_offsets();
                    write!(output, "{start}..{end}");
                } else {
                    output.write_char('-');
                }
            }
            writeln!(output, " attrs={:?}", current.attrs());

            if matches!(event, StructuralEvent::Enter(_)) {
                depth += 1;
            }
        }
    }
}

pub(crate) struct HtmlInlineElementRenderer(pub(crate) &'static str);

impl<T: NodeValue> NodeRenderer<T> for HtmlInlineElementRenderer {
    fn render(&self, node: &Node, _: &T, context: &mut RenderContext<'_>, output: &mut Writer) {
        write_html_open(output, self.0, node.attrs());
        context.render_children(node.id(), output);
        write_html_close(output, self.0);
    }
}

pub(crate) struct HtmlBlockElementRenderer(pub(crate) &'static str);

impl<T: NodeValue> NodeRenderer<T> for HtmlBlockElementRenderer {
    fn render(&self, node: &Node, _: &T, context: &mut RenderContext<'_>, output: &mut Writer) {
        context.cr(output);
        write_html_open(output, self.0, node.attrs());
        context.render_children(node.id(), output);
        write_html_close(output, self.0);
        context.cr(output);
    }
}
