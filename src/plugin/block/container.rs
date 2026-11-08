//! High-level block containers.
//!
//! A container parses a fixed opening region, parses its body as nested block
//! markup, and attaches the resulting nodes to a payload node. Use
//! [`PluginSpec::block_container`](crate::plugin::PluginSpec::block_container)
//! to build one.

use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

use super::shared::{
    BlockContainerState,
    ErasedBlockPlugin,
    block_window_end,
    install_block_plugin,
};
use crate::MarkdownIt;
use crate::document::{Node, NodeId, NodeValue};
use crate::parser::block::BlockState;
use crate::plugin::PluginSpec;
use crate::plugin::container_context::{ContainerHtml, ContainerText};
use crate::plugin::html::validate_tag;
use crate::plugin::renderer::{
    ContainerHtmlRenderer,
    ContainerTextRenderer,
    Renderers,
    WithRenderer,
};
use crate::plugin::spec::{PluginInstallContext, PluginInstaller};
use crate::render::{NodeRenderer, RenderContext, Writer, write_html_close, write_html_open};

pub(super) struct BlockContainerTagRenderer<T> {
    tag: String,
    payload: PhantomData<fn() -> T>,
}

impl<T> BlockContainerTagRenderer<T> {
    fn new(tag: String) -> Self {
        Self {
            tag,
            payload: PhantomData,
        }
    }
}

impl<T: NodeValue> NodeRenderer<T> for BlockContainerTagRenderer<T> {
    fn render(&self, node: &Node, _: &T, context: &mut RenderContext<'_>, output: &mut Writer) {
        context.cr(output);
        write_html_open(output, &self.tag, node.attrs());
        if !node.children().is_empty() {
            context.cr(output);
            context.render_children(node.id(), output);
        }
        write_html_close(output, &self.tag);
        context.cr(output);
    }
}

type ContainerParseFn<T> =
    Arc<dyn Fn(&BlockContainerState<'_>) -> Option<BlockContainerParse<T>> + Send + Sync + 'static>;

/// Layout returned by a block container parser.
///
/// The ranges are relative to the line where the container starts. `open_lines`
/// must be at least one. The builder parses `body_lines` as nested block
/// content and skips `close_lines`; an unclosed container is represented by
/// `close_lines == 0` and a body that extends to the end of the visible block
/// window. Use [`BlockContainerParse::new`] for a one-line opening and
/// configure closing lines or indentation only when needed.
#[derive(Debug)]
pub struct BlockContainerParse<T> {
    /// Payload stored on the container node.
    pub value: T,
    /// Lines forming the opening fence or header. Must be greater than zero.
    pub open_lines: usize,
    /// Lines between the opening and closing markers, parsed as markup.
    pub body_lines: usize,
    /// Lines forming the closing marker.
    pub close_lines: usize,
    /// Additional indentation reserved for the body.
    ///
    /// This is added to the current block indent while parsing the body. The
    /// first implementation does not rewrite line offsets, so containers that
    /// strip a per-line prefix (blockquotes) still need a low-level
    /// [`BlockRule`](crate::parser::block::BlockRule).
    pub content_indent: usize,
}

impl<T> BlockContainerParse<T> {
    /// Create a layout with one opening line, `body_lines` body lines,
    /// no closing lines and no additional content indentation.
    ///
    /// For a closed fence, add `.close_lines(1)`. The body count excludes
    /// both opening and closing lines. An unclosed fence should extend its
    /// body to the end of the visible block window.
    ///
    /// ```
    /// use markdown_it::plugin::BlockContainerParse;
    ///
    /// let layout = BlockContainerParse::new("note", 3)
    ///     .close_lines(1)
    ///     .content_indent(2);
    /// assert_eq!(layout.body_lines, 3);
    /// assert_eq!(layout.close_lines, 1);
    /// ```
    pub fn new(value: T, body_lines: usize) -> Self {
        Self {
            value,
            open_lines: 1,
            body_lines,
            close_lines: 0,
            content_indent: 0,
        }
    }

    /// Set the number of opening lines (default `1`). Must be greater than zero.
    #[must_use]
    pub fn open_lines(mut self, lines: usize) -> Self {
        self.open_lines = lines;
        self
    }

    /// Set the number of closing lines to skip (default `0`).
    #[must_use]
    pub fn close_lines(mut self, lines: usize) -> Self {
        self.close_lines = lines;
        self
    }

    /// Set the additional indentation reserved for the body (default `0`).
    #[must_use]
    pub fn content_indent(mut self, indent: usize) -> Self {
        self.content_indent = indent;
        self
    }
}

/// Builds one block container inside a [`PluginSpec`].
///
/// A parser and at least one renderer are required before [`finish`](Self::finish).
/// Paragraph interruption is disabled by default.
#[must_use = "call finish() to add this container to its plugin"]
pub struct BlockContainerBuilder<T: NodeValue, P = (), R = ()> {
    plugin: PluginSpec,
    rule: String,
    interrupts_paragraph: bool,
    parse: P,
    render: R,
    renderers: Renderers,
    payload: PhantomData<fn() -> T>,
}

impl<T: NodeValue> BlockContainerBuilder<T> {
    pub(in crate::plugin) fn new(plugin: PluginSpec, rule: String) -> Self {
        Self {
            plugin,
            rule,
            interrupts_paragraph: false,
            parse: (),
            render: (),
            renderers: Renderers::default(),
            payload: PhantomData,
        }
    }
}

impl<T: NodeValue, P, R> BlockContainerBuilder<T, P, R> {
    /// Allow this container to interrupt an open paragraph (default `false`).
    pub fn interrupts_paragraph(mut self, interrupts: bool) -> Self {
        self.interrupts_paragraph = interrupts;
        self
    }

    /// Set the parser for the container layout.
    ///
    /// Read lines with [`BlockContainerState::line`] and inspect indentation
    /// with [`BlockContainerState::line_indent`]. Return the payload and line
    /// counts; the builder creates the node and parses the body after a
    /// successful match. The counts are always relative to the callback's
    /// initial line and are the only source of consumption.
    ///
    /// The callback must be free of side effects because it may also run while
    /// probing paragraph interruption.
    pub fn parse<F>(self, parse: F) -> BlockContainerBuilder<T, F, R>
    where
        F: Fn(&BlockContainerState<'_>) -> Option<BlockContainerParse<T>> + Send + Sync + 'static,
    {
        BlockContainerBuilder {
            plugin: self.plugin,
            rule: self.rule,
            interrupts_paragraph: self.interrupts_paragraph,
            parse,
            render: self.render,
            renderers: self.renderers,
            payload: PhantomData,
        }
    }

    /// Register an HTML callback that can render the container's children.
    ///
    /// Use [`ContainerHtml::render_children`] to render the parsed body. The
    /// fragment is rendered as a block, separated from adjacent block output by
    /// line breaks. Repeated registrations for the same format replace earlier
    /// ones.
    pub fn render_html<F>(self, render: F) -> BlockContainerBuilder<T, P, WithRenderer>
    where
        F: Fn(&T, &mut ContainerHtml<'_, '_>) + Send + Sync + 'static,
    {
        self.render_with("html", ContainerHtmlRenderer::<F, T, true>::new(render))
    }

    /// Register a plain-text callback that can render the container's children.
    ///
    /// Use [`ContainerText::render_children`] to render the parsed body. The
    /// output is separated from adjacent blocks by line breaks. The last
    /// registration for a payload type and format wins.
    pub fn render_text<F>(self, render: F) -> BlockContainerBuilder<T, P, WithRenderer>
    where
        F: Fn(&T, &mut ContainerText<'_, '_>) + Send + Sync + 'static,
    {
        self.render_with("text", ContainerTextRenderer::<F, T, true>::new(render))
    }

    /// Wrap this container's children in an HTML tag.
    ///
    /// Existing node attributes are written on the tag. The last renderer
    /// registered for a payload type and format wins.
    ///
    /// # Panics
    ///
    /// Panics if `tag` is not a valid ASCII HTML tag name.
    pub fn html_tag(self, tag: impl Into<String>) -> BlockContainerBuilder<T, P, WithRenderer> {
        let tag = tag.into();
        validate_tag(&tag);
        self.render_with("html", BlockContainerTagRenderer::<T>::new(tag))
    }

    /// Register a low-level renderer for an output format.
    ///
    /// May be called repeatedly for different formats. The renderer can use
    /// [`RenderContext::render_children`] to emit the container's parsed
    /// children in the active output format, and must control its own block
    /// line breaks.
    pub fn render_with<N: NodeRenderer<T>>(
        mut self,
        format: impl Into<String>,
        renderer: N,
    ) -> BlockContainerBuilder<T, P, WithRenderer> {
        self.renderers.add::<T, _>(format, renderer);
        BlockContainerBuilder {
            plugin: self.plugin,
            rule: self.rule,
            interrupts_paragraph: self.interrupts_paragraph,
            parse: self.parse,
            render: WithRenderer,
            renderers: self.renderers,
            payload: PhantomData,
        }
    }
}

impl<T: NodeValue, P> BlockContainerBuilder<T, P, WithRenderer>
where
    P: Fn(&BlockContainerState<'_>) -> Option<BlockContainerParse<T>> + Send + Sync + 'static,
{
    /// Add the container to its plugin and continue building that plugin.
    pub fn finish(self) -> PluginSpec {
        let installer = ConfiguredBlockContainer::<T> {
            rule: self.rule,
            interrupts_paragraph: self.interrupts_paragraph,
            parse: Arc::new(self.parse),
        };
        self.plugin.with_installer(self.renderers.attach(installer))
    }
}

struct ConfiguredBlockContainer<T: NodeValue> {
    rule: String,
    interrupts_paragraph: bool,
    parse: ContainerParseFn<T>,
}

impl<T: NodeValue> fmt::Debug for ConfiguredBlockContainer<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConfiguredBlockContainer")
            .field("rule", &self.rule)
            .field("interrupts_paragraph", &self.interrupts_paragraph)
            .finish_non_exhaustive()
    }
}

impl<T: NodeValue> ConfiguredBlockContainer<T> {
    fn parse(&self, state: &BlockState<'_>) -> Option<(BlockContainerParse<T>, usize)> {
        // Like built-in block rules, an indented code line cannot start a
        // construct.
        if state.line_indent(state.line) >= state.md.max_indent {
            return None;
        }

        let window_end = block_window_end(state);
        let cx = BlockContainerState::new(
            state.src,
            &state.line_offsets,
            state.line,
            window_end,
            state.blk_indent,
            state.md,
        );
        let parsed = (self.parse)(&cx)?;
        let total = self.validate(state, window_end, &parsed);
        Some((parsed, total))
    }

    fn validate(
        &self,
        state: &BlockState<'_>,
        window_end: usize,
        parsed: &BlockContainerParse<T>,
    ) -> usize {
        assert!(
            parsed.open_lines > 0,
            "block container rule {:?} must consume at least one opening line",
            self.rule
        );
        let total = parsed
            .open_lines
            .checked_add(parsed.body_lines)
            .and_then(|total| total.checked_add(parsed.close_lines))
            .expect("block container line counts overflowed");
        assert!(
            state
                .line
                .checked_add(total)
                .is_some_and(|end| end <= window_end),
            "block container rule {:?} consumed past the source window",
            self.rule
        );
        total
    }

    fn check(&self, state: &BlockState<'_>) -> Option<()> {
        if !self.interrupts_paragraph {
            return None;
        }
        self.parse(state).map(|_| ())
    }

    fn run(&self, state: &mut BlockState<'_>) -> Option<(Option<NodeId>, usize)> {
        let (parsed, total) = self.parse(state)?;
        let BlockContainerParse {
            value,
            open_lines,
            body_lines,
            content_indent,
            ..
        } = parsed;

        let start = state.line;
        let body_start = start
            .checked_add(open_lines)
            .expect("block container body start overflowed");
        let body_end = body_start
            .checked_add(body_lines)
            .expect("block container body end overflowed");
        let node = state.document.create_node(value);

        let old_node = std::mem::replace(&mut state.node, node);
        let old_line_max = state.line_max;
        let old_blk_indent = state.blk_indent;
        // Never let the declared indent outdent a non-empty body line: nested
        // tokenization would stop there while the container still consumed the
        // line. Reducing the effective indent keeps every body line parseable.
        let available_indent = (0..body_lines)
            .filter(|&offset| !state.is_empty(body_start + offset))
            .map(|offset| {
                let offsets = &state.line_offsets[body_start + offset];
                (offsets.indent_nonspace - old_blk_indent as i32).max(0) as usize
            })
            .min()
            .unwrap_or(content_indent);
        let content_indent = content_indent.min(available_indent);
        state.blk_indent = old_blk_indent
            .checked_add(content_indent)
            .expect("block container content indent overflowed");

        state.line = body_start;
        state.line_max = body_end;
        if body_lines > 0 {
            state.tokenize_nested();
        }

        state.line = start;
        state.line_max = old_line_max;
        state.blk_indent = old_blk_indent;
        let node = std::mem::replace(&mut state.node, old_node);
        Some((Some(node), total))
    }
}

impl<T: NodeValue> ErasedBlockPlugin for ConfiguredBlockContainer<T> {
    fn check(&self, state: &BlockState<'_>) -> Option<()> {
        ConfiguredBlockContainer::check(self, state)
    }

    fn run(&self, state: &mut BlockState<'_>) -> Option<(Option<NodeId>, usize)> {
        ConfiguredBlockContainer::run(self, state)
    }
}

impl<T: NodeValue> PluginInstaller for ConfiguredBlockContainer<T> {
    fn name(&self) -> &str {
        &self.rule
    }

    fn validate(&self) {
        assert!(
            !self.rule.is_empty(),
            "dynamic block rule name must not be empty"
        );
    }

    fn install(self: Box<Self>, md: &mut MarkdownIt, _context: &PluginInstallContext<'_>) {
        install_block_plugin(md, Arc::new(*self));
    }
}
