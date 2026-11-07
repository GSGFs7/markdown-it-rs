//! High-level inline containers.
//!
//! A container parses its body as an independent child inline session and
//! attaches the resulting nodes to a payload node. Use
//! [`PluginSpec::inline_container`](super::PluginSpec::inline_container) to
//! build one.

use std::fmt;
use std::marker::PhantomData;
use std::ops::Range;
use std::sync::Arc;

use super::html::validate_tag;
use super::renderer::{Renderers, WithRenderer};
use super::spec::PluginInstaller;
use super::{Html, HtmlElement, InlinePluginState, PluginSpec};
use crate::MarkdownIt;
use crate::document::{Node, NodeId, NodeValue};
use crate::parser::inline::InlineState;
use crate::render::{NodeRenderer, RenderContext, Writer, write_html_close, write_html_open};

type ParseFn<T> = Arc<
    dyn for<'a> Fn(&mut InlineContainerState<'a>) -> Option<(T, Range<usize>)>
        + Send
        + Sync
        + 'static,
>;

/// Cursor passed to an inline container parser.
///
/// The cursor starts at the container's first byte. Consumption is committed
/// only when the callback returns `Some`, so a failed match rolls back
/// automatically. This state deliberately has no document access: the parser
/// only identifies the child source range, and the builder parses that range
/// after the match succeeds. Match results and consumed lengths must therefore
/// depend on source and configuration only.
pub struct InlineContainerState<'a> {
    cursor: InlinePluginState<'a>,
    markdown_it: &'a MarkdownIt,
}

impl<'a> InlineContainerState<'a> {
    fn new(state: &'a mut InlineState<'_>) -> Self {
        Self {
            cursor: InlinePluginState::new(state.remaining()),
            markdown_it: state.markdown_it(),
        }
    }

    /// Unconsumed source available to this rule.
    pub fn remaining(&self) -> &str {
        self.cursor.remaining()
    }

    /// The complete source window visible to this rule.
    pub fn source(&self) -> &str {
        self.cursor.source()
    }

    /// Parser instance that owns the active inline rule.
    pub fn markdown_it(&self) -> &'a MarkdownIt {
        self.markdown_it
    }

    /// Number of bytes consumed so far.
    pub fn consumed(&self) -> usize {
        self.cursor.consumed()
    }

    /// Whether the unconsumed source starts with `literal`.
    pub fn starts_with(&self, literal: &str) -> bool {
        self.cursor.starts_with(literal)
    }

    /// Consume `literal` if it is next, returning whether it matched.
    pub fn consume(&mut self, literal: &str) -> bool {
        self.cursor.consume(literal)
    }

    /// Consume one character, returning it when one is available.
    pub fn consume_char(&mut self) -> Option<char> {
        self.cursor.consume_char()
    }

    /// Consume exactly `bytes` bytes.
    ///
    /// Returns `false` without moving the cursor when the range is outside the
    /// source or not on a UTF-8 boundary.
    pub fn advance(&mut self, bytes: usize) -> bool {
        self.cursor.advance(bytes)
    }
}

/// HTML builder passed to [`InlineContainerBuilder::render_html`].
///
/// Children must be rendered explicitly with
/// [`render_children`](Self::render_children). The returned string is already
/// escaped HTML and can be inserted with [`raw`](Self::raw) or
/// [`HtmlElement::raw`].
pub struct ContainerHtml<'a, 'b> {
    html: &'a mut Html,
    context: &'a mut RenderContext<'b>,
    node: NodeId,
}

impl<'a, 'b> ContainerHtml<'a, 'b> {
    /// Add a child element.
    pub fn element(&mut self, tag: impl Into<String>) -> HtmlElement<'_> {
        self.html.element(tag)
    }

    /// Append text. It is escaped when the fragment is rendered.
    pub fn text(&mut self, text: impl AsRef<str>) -> &mut Self {
        self.html.text(text);
        self
    }

    /// Append markup verbatim.
    pub fn raw(&mut self, html: impl AsRef<str>) -> &mut Self {
        self.html.raw(html);
        self
    }

    /// Render this container's children in the active output format.
    ///
    /// Returns an empty string for a leaf container with no children.
    pub fn render_children(&mut self) -> String {
        let mut output = Writer::default();
        self.context.render_children(self.node, &mut output);
        output.finish()
    }
}

/// Plain-text writer passed to [`InlineContainerBuilder::render_text`].
pub struct ContainerText<'a, 'b> {
    output: &'a mut Writer,
    context: &'a mut RenderContext<'b>,
    node: NodeId,
}

impl ContainerText<'_, '_> {
    /// Append a string to the output buffer.
    pub fn write_str(&mut self, value: &str) {
        self.output.write_str(value);
    }

    /// Append a char to the output buffer.
    pub fn write_char(&mut self, value: char) {
        self.output.write_char(value);
    }

    /// Append formatted output.
    ///
    /// # Panics
    ///
    /// Panics if a formatting implementation returns an error.
    pub fn write_fmt(&mut self, args: fmt::Arguments<'_>) {
        self.output.write_fmt(args);
    }

    /// Render this container's children in the active output format.
    pub fn render_children(&mut self) {
        self.context.render_children(self.node, self.output);
    }
}

impl fmt::Write for ContainerText<'_, '_> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        fmt::Write::write_str(self.output, value)
    }
}

/// Builds one inline container inside a [`PluginSpec`].
///
/// A parser and at least one renderer are required before [`finish`](Self::finish).
/// The marker defaults to `'\0'`, which tries matching at every position.
#[must_use = "call finish() to add this container to its plugin"]
pub struct InlineContainerBuilder<T: NodeValue, P = (), R = ()> {
    plugin: PluginSpec,
    rule: String,
    marker: char,
    parse: P,
    render: R,
    renderers: Renderers,
    payload: PhantomData<fn() -> T>,
}

impl<T: NodeValue> InlineContainerBuilder<T> {
    pub(super) fn new(plugin: PluginSpec, rule: String) -> Self {
        Self {
            plugin,
            rule,
            marker: '\0',
            parse: (),
            render: (),
            renderers: Renderers::default(),
            payload: PhantomData,
        }
    }
}

impl<T: NodeValue, P, R> InlineContainerBuilder<T, P, R> {
    /// Set the character that may start this container.
    pub fn marker(mut self, marker: char) -> Self {
        self.marker = marker;
        self
    }

    /// Set the parser.
    ///
    /// The callback receives a cursor and returns the container payload plus the
    /// byte range of its children. The range is parsed by the builder after the
    /// match succeeds.
    ///
    /// The child range must:
    ///
    /// - be relative to [`InlineContainerState::source`], the start of the
    ///   container;
    /// - lie entirely inside the bytes consumed by the callback, i.e.
    ///   `range.end <= consumed()`;
    /// - start and end on UTF-8 boundaries.
    ///
    /// An invalid range is a plugin implementation error and panics. Returning
    /// `None` rolls back local consumption. A successful parse must consume at
    /// least one byte.
    ///
    /// The callback may also run while probing text boundaries. Its result and
    /// consumed length must depend only on the source and captured
    /// configuration; keep it free of observable side effects.
    pub fn parse<F>(self, parse: F) -> InlineContainerBuilder<T, F, R>
    where
        F: for<'a> Fn(&mut InlineContainerState<'a>) -> Option<(T, Range<usize>)>
            + Send
            + Sync
            + 'static,
    {
        InlineContainerBuilder {
            plugin: self.plugin,
            rule: self.rule,
            marker: self.marker,
            parse,
            render: self.render,
            renderers: self.renderers,
            payload: PhantomData,
        }
    }

    /// Register an HTML callback that can render the container's children.
    ///
    /// Use [`ContainerHtml::render_children`] to render the parsed body.
    /// Repeated registrations for the same format replace earlier ones.
    pub fn render_html<F>(self, render: F) -> InlineContainerBuilder<T, P, WithRenderer>
    where
        F: for<'a, 'b> Fn(&T, &mut ContainerHtml<'a, 'b>) + Send + Sync + 'static,
    {
        self.render_with("html", ContainerHtmlRenderer::<F, T>::new(render))
    }

    /// Register a plain-text callback that can render the container's children.
    ///
    /// Use [`ContainerText::render_children`] to render the parsed body.
    pub fn render_text<F>(self, render: F) -> InlineContainerBuilder<T, P, WithRenderer>
    where
        F: for<'a, 'b> Fn(&T, &mut ContainerText<'a, 'b>) + Send + Sync + 'static,
    {
        self.render_with("text", ContainerTextRenderer::<F, T>::new(render))
    }

    /// Wrap this container's children in an HTML tag.
    ///
    /// This is a shortcut for [`render_with`](Self::render_with). Existing node
    /// attributes are written on the tag. The last renderer registered for a
    /// payload type and format wins.
    ///
    /// # Panics
    ///
    /// Panics if `tag` is not a valid ASCII HTML tag name.
    pub fn html_tag(self, tag: impl Into<String>) -> InlineContainerBuilder<T, P, WithRenderer> {
        let tag = tag.into();
        validate_tag(&tag);
        self.render_with(
            "html",
            ContainerTagRenderer::<T> {
                tag,
                payload: PhantomData,
            },
        )
    }

    /// Register a low-level renderer for an output format.
    ///
    /// May be called repeatedly for different formats. The renderer can use
    /// [`RenderContext::render_children`] to emit the container's parsed
    /// children in the active output format.
    pub fn render_with<N: NodeRenderer<T>>(
        mut self,
        format: impl Into<String>,
        renderer: N,
    ) -> InlineContainerBuilder<T, P, WithRenderer> {
        self.renderers.add::<T, _>(format, renderer);
        InlineContainerBuilder {
            plugin: self.plugin,
            rule: self.rule,
            marker: self.marker,
            parse: self.parse,
            render: WithRenderer,
            renderers: self.renderers,
            payload: PhantomData,
        }
    }
}

impl<T: NodeValue, P> InlineContainerBuilder<T, P, WithRenderer>
where
    P: for<'a> Fn(&mut InlineContainerState<'a>) -> Option<(T, Range<usize>)>
        + Send
        + Sync
        + 'static,
{
    /// Add the container to its owning plugin and resume building that plugin.
    pub fn finish(self) -> PluginSpec {
        let installer = ConfiguredInlineContainer::<T> {
            rule: self.rule,
            marker: self.marker,
            parse: Arc::new(self.parse),
        };
        self.plugin.with_installer(self.renderers.attach(installer))
    }
}

struct ConfiguredInlineContainer<T: NodeValue> {
    rule: String,
    marker: char,
    parse: ParseFn<T>,
}

impl<T: NodeValue> ConfiguredInlineContainer<T> {
    fn run(&self, state: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        let (value, children, consumed) = {
            let mut cx = InlineContainerState::new(state);
            let (value, children) = (self.parse)(&mut cx)?;
            (value, children, cx.consumed())
        };

        assert_progress(&self.rule, state.remaining(), consumed);
        assert_child_range(&self.rule, state.remaining(), &children, consumed);
        let children = state
            .parse_subrange(children)
            .expect("validated inline container child range");

        let (_, document) = state.remaining_and_document();
        let node = document.create_node(value);
        for child in children {
            document.push_child(node, child);
        }
        Some((Some(node), consumed))
    }

    fn check(&self, state: &mut InlineState<'_>) -> Option<usize> {
        let (children, consumed) = {
            let mut cx = InlineContainerState::new(state);
            let (_, children) = (self.parse)(&mut cx)?;
            (children, cx.consumed())
        };
        assert_child_range(&self.rule, state.remaining(), &children, consumed);
        assert_progress(&self.rule, state.remaining(), consumed);
        Some(consumed)
    }
}

impl<T: NodeValue> PluginInstaller for ConfiguredInlineContainer<T> {
    fn name(&self) -> &str {
        &self.rule
    }

    fn validate(&self) {
        assert!(
            !self.rule.is_empty(),
            "inline container rule name must not be empty"
        );
    }

    fn install(self: Box<Self>, md: &mut MarkdownIt) {
        let container = Arc::new(*self);
        let mark = crate::common::RuleMark::named(format!("dynamic:{}", container.rule));

        let run_container = Arc::clone(&container);
        let run = Arc::new(move |state: &mut InlineState<'_>| run_container.run(state));

        let check_container = Arc::clone(&container);
        let check = Arc::new(move |state: &mut InlineState<'_>| check_container.check(state));

        md.inline
            .add_dynamic_rule(mark, container.marker, run, check)
            .after_named("text")
            .before_all();
    }
}

struct ContainerTagRenderer<T> {
    tag: String,
    payload: PhantomData<fn() -> T>,
}

impl<T: NodeValue> NodeRenderer<T> for ContainerTagRenderer<T> {
    fn render(&self, node: &Node, _: &T, context: &mut RenderContext<'_>, output: &mut Writer) {
        write_html_open(output, &self.tag, node.attrs());
        context.render_children(node.id(), output);
        write_html_close(output, &self.tag);
    }
}

pub(super) struct ContainerHtmlRenderer<F, T>(F, PhantomData<fn() -> T>);

impl<F, T> ContainerHtmlRenderer<F, T> {
    pub(super) fn new(render: F) -> Self {
        Self(render, PhantomData)
    }
}

impl<T, F> NodeRenderer<T> for ContainerHtmlRenderer<F, T>
where
    T: NodeValue,
    F: for<'a, 'b> Fn(&T, &mut ContainerHtml<'a, 'b>) + Send + Sync + 'static,
{
    fn render(&self, node: &Node, value: &T, context: &mut RenderContext<'_>, output: &mut Writer) {
        let mut html = Html::new();
        {
            let mut cx = ContainerHtml {
                html: &mut html,
                context,
                node: node.id(),
            };
            (self.0)(value, &mut cx);
        }
        html.render_into(output, context.options());
    }
}

pub(super) struct ContainerTextRenderer<F, T>(F, PhantomData<fn() -> T>);

impl<F, T> ContainerTextRenderer<F, T> {
    pub(super) fn new(render: F) -> Self {
        Self(render, PhantomData)
    }
}

impl<T, F> NodeRenderer<T> for ContainerTextRenderer<F, T>
where
    T: NodeValue,
    F: for<'a, 'b> Fn(&T, &mut ContainerText<'a, 'b>) + Send + Sync + 'static,
{
    fn render(&self, node: &Node, value: &T, context: &mut RenderContext<'_>, output: &mut Writer) {
        let mut cx = ContainerText {
            output,
            context,
            node: node.id(),
        };
        (self.0)(value, &mut cx);
    }
}

fn assert_child_range(rule: &str, source: &str, range: &Range<usize>, consumed: usize) {
    assert!(
        range.end <= consumed && source.get(range.clone()).is_some(),
        "inline container rule {rule:?} returned an invalid child range {range:?} for {consumed} consumed bytes"
    );
}

fn assert_progress(rule: &str, source: &str, consumed: usize) {
    assert!(
        consumed > 0,
        "inline container rule {rule:?} succeeded without consuming input"
    );
    assert!(
        source.get(..consumed).is_some(),
        "inline container rule {rule:?} consumed past the source window or split a UTF-8 character"
    );
}
