//! High-level block leaves built on shared block plugin infrastructure.

use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

use super::shared::{BlockPluginState, ErasedBlockPlugin, block_window_end, install_block_plugin};
use crate::MarkdownIt;
use crate::document::{NodeId, NodeValue};
use crate::parser::block::BlockState;
use crate::plugin::renderer::{
    HtmlCallbackRenderer,
    Renderers,
    TextCallbackRenderer,
    WithRenderer,
};
use crate::plugin::spec::{PluginInstallContext, PluginInstaller};
use crate::plugin::{Html, PluginSpec};
use crate::render::{NodeRenderer, Writer};

type ParseFn<T> = Arc<dyn Fn(&mut BlockPluginState<'_>) -> Option<T> + Send + Sync + 'static>;

/// Builds one block leaf inside a [`PluginSpec`].
///
/// A parser and at least one renderer are required before [`finish`](Self::finish).
/// Paragraph interruption is disabled by default.
#[must_use = "call finish() to add this leaf to its plugin"]
pub struct BlockLeafBuilder<T: NodeValue, P = (), R = ()> {
    plugin: PluginSpec,
    rule: String,
    interrupts_paragraph: bool,
    parse: P,
    render: R,
    renderers: Renderers,
    payload: PhantomData<fn() -> T>,
}

impl<T: NodeValue> BlockLeafBuilder<T> {
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

impl<T: NodeValue, P, R> BlockLeafBuilder<T, P, R> {
    /// Allow this construct to interrupt an open paragraph (default `false`).
    pub fn interrupts_paragraph(mut self, interrupts: bool) -> Self {
        self.interrupts_paragraph = interrupts;
        self
    }

    /// Set the parser.
    ///
    /// Read lines with [`BlockPluginState::line`] and consume them with
    /// [`BlockPluginState::advance_lines`]. Returning `None` rolls back
    /// consumption; success must consume at least one line.
    ///
    /// This may also run during paragraph-interruption checks, so keep it free
    /// of side effects.
    pub fn parse<F>(self, parse: F) -> BlockLeafBuilder<T, F, R>
    where
        F: Fn(&mut BlockPluginState<'_>) -> Option<T> + Send + Sync + 'static,
    {
        BlockLeafBuilder {
            plugin: self.plugin,
            rule: self.rule,
            interrupts_paragraph: self.interrupts_paragraph,
            parse,
            render: self.render,
            renderers: self.renderers,
            payload: PhantomData,
        }
    }

    /// Register an HTML callback. Text and attributes are escaped.
    ///
    /// The fragment is rendered as a block, separated from adjacent block
    /// output by line breaks. Repeated registrations for the same format
    /// replace earlier ones.
    pub fn render_html<F>(self, render: F) -> BlockLeafBuilder<T, P, WithRenderer>
    where
        F: Fn(&T, &mut Html) + Send + Sync + 'static,
    {
        self.render_with("html", HtmlCallbackRenderer::<F, true>(render))
    }

    /// Register a plain-text callback for the `"text"` output format.
    ///
    /// Writes directly to [`Writer`], without HTML escaping. Like the HTML
    /// shortcut, the block output is separated from adjacent output by line
    /// breaks; existing trailing line breaks are preserved without duplication.
    /// The last registration for a payload type and format wins. Use
    /// [`render_with`](Self::render_with) to control all line breaks yourself.
    pub fn render_text<F>(self, render: F) -> BlockLeafBuilder<T, P, WithRenderer>
    where
        F: Fn(&T, &mut Writer) + Send + Sync + 'static,
    {
        self.render_with("text", TextCallbackRenderer::<F, true>(render))
    }

    /// Register a low-level renderer for an output format.
    ///
    /// May be called repeatedly; the last renderer for a payload type and
    /// format wins, just like [`MarkdownIt::add_renderer`]. The renderer
    /// controls all output, including block line breaks. Use
    /// [`RenderContext::cr`](crate::render::RenderContext::cr) when needed.
    pub fn render_with<N: NodeRenderer<T>>(
        mut self,
        format: impl Into<String>,
        renderer: N,
    ) -> BlockLeafBuilder<T, P, WithRenderer> {
        self.renderers.add::<T, _>(format, renderer);
        BlockLeafBuilder {
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

impl<T: NodeValue, P> BlockLeafBuilder<T, P, WithRenderer>
where
    P: Fn(&mut BlockPluginState<'_>) -> Option<T> + Send + Sync + 'static,
{
    /// Add the leaf to its plugin and continue building that plugin.
    pub fn finish(self) -> PluginSpec {
        let installer = ConfiguredBlockLeaf::<T> {
            rule: self.rule,
            interrupts_paragraph: self.interrupts_paragraph,
            parse: Arc::new(self.parse),
        };
        self.plugin.with_installer(self.renderers.attach(installer))
    }
}

struct ConfiguredBlockLeaf<T: NodeValue> {
    rule: String,
    interrupts_paragraph: bool,
    parse: ParseFn<T>,
}

impl<T: NodeValue> fmt::Debug for ConfiguredBlockLeaf<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConfiguredBlockLeaf")
            .field("rule", &self.rule)
            .field("interrupts_paragraph", &self.interrupts_paragraph)
            .finish_non_exhaustive()
    }
}

impl<T: NodeValue> ConfiguredBlockLeaf<T> {
    fn parse(&self, state: &BlockState<'_>) -> Option<(T, usize)> {
        // Like built-in block rules, an indented code line cannot start a
        // construct.
        if state.line_indent(state.line) >= state.md.max_indent {
            return None;
        }

        let window_end = block_window_end(state);

        let mut cx = BlockPluginState::new(
            state.src,
            &state.line_offsets,
            state.line,
            window_end,
            state.blk_indent,
            state.md,
        );
        let value = (self.parse)(&mut cx)?;
        let consumed = cx.consumed_lines();

        assert!(
            consumed > 0,
            "block plugin rule {:?} succeeded without consuming input",
            self.rule
        );
        assert!(
            state
                .line
                .checked_add(consumed)
                .is_some_and(|end| end <= state.line_max),
            "block plugin rule {:?} consumed past the source window",
            self.rule
        );

        Some((value, consumed))
    }

    fn check(&self, state: &BlockState<'_>) -> Option<()> {
        if !self.interrupts_paragraph {
            return None;
        }
        self.parse(state).map(|_| ())
    }

    fn run(&self, state: &mut BlockState<'_>) -> Option<(Option<NodeId>, usize)> {
        let (value, consumed) = self.parse(state)?;
        let node = state.document.create_node(value);
        Some((Some(node), consumed))
    }
}

impl<T: NodeValue> ErasedBlockPlugin for ConfiguredBlockLeaf<T> {
    fn check(&self, state: &BlockState<'_>) -> Option<()> {
        ConfiguredBlockLeaf::check(self, state)
    }

    fn run(&self, state: &mut BlockState<'_>) -> Option<(Option<NodeId>, usize)> {
        ConfiguredBlockLeaf::run(self, state)
    }
}

impl<T: NodeValue> PluginInstaller for ConfiguredBlockLeaf<T> {
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
