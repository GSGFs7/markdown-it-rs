use std::marker::PhantomData;
use std::sync::Arc;

use super::html::render_html_fragment;
use super::renderer::{HtmlCallbackRenderer, Renderers, TextCallbackRenderer, WithRenderer};
use super::spec::{PluginInstaller, RegistrationContext};
use super::{Html, InlinePluginState, Plugin, PluginSpec};
use crate::MarkdownIt;
use crate::document::{Document, NodeId, NodeValue};
use crate::parser::inline::{InlineRule, InlineState};
use crate::render::{NodeRenderer, RenderContext, Writer};

/// A leaf node that owns its parsing and HTML rendering.
///
/// Implement this trait for the common case: one payload type, one syntax rule,
/// one renderer. The payload automatically implements [`NodeValue`] when it is
/// `Debug + Send + Sync + 'static`. The payload type is also the rule identity,
/// so registering the same payload type twice is rejected.
///
/// ```
/// use markdown_it::{Html, InlinePlugin, InlinePluginState, MarkdownIt};
///
/// #[derive(Debug)]
/// struct Mark;
///
/// impl InlinePlugin for Mark {
///     const NAME: &'static str = "mark";
///     const MARKER: char = '=';
///
///     fn parse(cx: &mut InlinePluginState<'_>) -> Option<Self> {
///         cx.consume("==").then_some(Mark)
///     }
///
///     fn render_html(&self, html: &mut Html) {
///         html.element("mark").text("marked");
///     }
/// }
///
/// let md = MarkdownIt::builder().plugin(Mark).build();
/// assert_eq!(md.render("a == b"), "<p>a <mark>marked</mark> b</p>\n");
/// ```
pub trait InlinePlugin: NodeValue + Sized + 'static {
    /// Name used for diagnostics and plugin ordering.
    const NAME: &'static str;

    /// Character that may start this leaf.
    ///
    /// `'\0'` tries the rule at every position and disables bulk text scanning.
    const MARKER: char = '\0';

    /// Parse the leaf at the current inline position.
    ///
    /// Consumption is transactional: returning `None` discards the local
    /// cursor. A successful parse must consume at least one byte.
    fn parse(cx: &mut InlinePluginState<'_>) -> Option<Self>;

    /// Render the parsed value as HTML.
    fn render_html(&self, html: &mut Html);
}

/// A static leaf type can be registered directly as a plugin.
///
/// ```
/// use markdown_it::{Html, InlinePlugin, InlinePluginState, MarkdownIt};
///
/// #[derive(Debug)]
/// struct Emoji;
///
/// impl InlinePlugin for Emoji {
///     const NAME: &'static str = "rocket";
///     const MARKER: char = ':';
///
///     fn parse(cx: &mut InlinePluginState<'_>) -> Option<Self> {
///         cx.consume(":rocket:").then_some(Emoji)
///     }
///
///     fn render_html(&self, html: &mut Html) {
///         html.element("span").text("🚀");
///     }
/// }
///
/// let md = MarkdownIt::builder().plugin(Emoji).build();
/// assert_eq!(md.render(":rocket:"), "<p><span>🚀</span></p>\n");
/// ```
impl<T: InlinePlugin> Plugin for T {
    fn into_spec(self) -> PluginSpec {
        PluginSpec::new(T::NAME).with_installer(Box::new(LeafRule::<T>::new()))
    }
}

type ParseFn<T> = Arc<dyn Fn(&mut InlinePluginState<'_>) -> Option<T> + Send + Sync + 'static>;

/// Builds one inline leaf inside a [`PluginSpec`].
///
/// A parser and at least one renderer are required before [`finish`](Self::finish).
/// The marker defaults to `'\0'`, which tries matching at every position.
#[must_use = "call finish() to add this leaf to its plugin"]
pub struct InlineLeafBuilder<T: NodeValue, P = (), R = ()> {
    plugin: PluginSpec,
    rule: String,
    marker: char,
    parse: P,
    render: R,
    renderers: Renderers,
    payload: PhantomData<fn() -> T>,
}

impl<T: NodeValue> InlineLeafBuilder<T> {
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

impl<T: NodeValue, P, R> InlineLeafBuilder<T, P, R> {
    /// Set the character that may start this leaf.
    pub fn marker(mut self, marker: char) -> Self {
        self.marker = marker;
        self
    }

    /// Set the parser. Returning `None` rolls back local consumption.
    /// A successful parse must consume at least one byte.
    pub fn parse<F>(self, parse: F) -> InlineLeafBuilder<T, F, R>
    where
        F: Fn(&mut InlinePluginState<'_>) -> Option<T> + Send + Sync + 'static,
    {
        InlineLeafBuilder {
            plugin: self.plugin,
            rule: self.rule,
            marker: self.marker,
            parse,
            render: self.render,
            renderers: self.renderers,
            payload: PhantomData,
        }
    }

    /// Register an HTML callback. Text and attribute values are escaped.
    ///
    /// Equivalent to registering a `NodeRenderer<T>` for `"html"`. Repeated
    /// registrations for the same format replace earlier ones.
    pub fn render_html<F>(self, render: F) -> InlineLeafBuilder<T, P, WithRenderer>
    where
        F: Fn(&T, &mut Html) + Send + Sync + 'static,
    {
        self.render_with("html", HtmlCallbackRenderer::<F, false>(render))
    }

    /// Register a plain-text callback for the `"text"` output format.
    ///
    /// The callback writes directly to [`Writer`], without HTML escaping or
    /// added line breaks. May be combined with [`render_html`](Self::render_html).
    /// Like [`render_with`](Self::render_with), the last registration for a
    /// payload type and format wins.
    ///
    /// ```
    /// use markdown_it::{MarkdownIt, PluginSpec};
    /// #[derive(Debug)]
    /// struct Emoji;
    /// let emoji = PluginSpec::new("emoji")
    ///     .inline_leaf::<Emoji>("rocket")
    ///     .marker(':')
    ///     .parse(|cx| cx.consume(":rocket:").then_some(Emoji))
    ///     .render_html(|_, html| { html.text("🚀"); })
    ///     .render_text(|_, out| out.write_str("[rocket]"))
    ///     .finish();
    /// let md = MarkdownIt::builder().plugin(emoji).build();
    /// let document = md.parse("Go :rocket:!");
    /// assert_eq!(md.render_html(&document), "<p>Go 🚀!</p>\n");
    /// assert_eq!(md.render_as(&document, "text"), "Go [rocket]!\n");
    /// ```
    pub fn render_text<F>(self, render: F) -> InlineLeafBuilder<T, P, WithRenderer>
    where
        F: Fn(&T, &mut Writer) + Send + Sync + 'static,
    {
        self.render_with("text", TextCallbackRenderer::<F, false>(render))
    }

    /// Register a low-level renderer for an output format.
    ///
    /// May be called repeatedly for different formats. For the same payload
    /// type and format, the last registration wins, just like
    /// [`MarkdownIt::add_renderer`]. The renderer receives the original node,
    /// value and render context, without an HTML fragment adapter.
    pub fn render_with<N: NodeRenderer<T>>(
        mut self,
        format: impl Into<String>,
        renderer: N,
    ) -> InlineLeafBuilder<T, P, WithRenderer> {
        self.renderers.add::<T, _>(format, renderer);
        InlineLeafBuilder {
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

impl<T: NodeValue, P> InlineLeafBuilder<T, P, WithRenderer>
where
    P: Fn(&mut InlinePluginState<'_>) -> Option<T> + Send + Sync + 'static,
{
    /// Add the leaf to its owning plugin and resume building that plugin.
    pub fn finish(self) -> PluginSpec {
        let installer = ConfiguredLeaf::<T> {
            rule: self.rule,
            marker: self.marker,
            parse: Arc::new(self.parse),
        };
        self.plugin.with_installer(self.renderers.attach(installer))
    }
}

struct ConfiguredLeaf<T: NodeValue> {
    rule: String,
    marker: char,
    parse: ParseFn<T>,
}

impl<T: NodeValue> ConfiguredLeaf<T> {
    fn parse(&self, source: &str) -> Option<(T, usize)> {
        parse_leaf(&self.rule, source, |cx| (self.parse)(cx))
    }

    fn run(&self, document: &mut Document, source: &str) -> Option<(NodeId, usize)> {
        let (value, consumed) = self.parse(source)?;
        Some((document.create_node(value), consumed))
    }

    fn check(&self, source: &str) -> Option<usize> {
        self.parse(source).map(|(_, consumed)| consumed)
    }
}

impl<T: NodeValue> PluginInstaller for ConfiguredLeaf<T> {
    fn name(&self) -> &str {
        &self.rule
    }

    fn validate(&self) {
        assert!(
            !self.rule.is_empty(),
            "dynamic leaf rule name must not be empty"
        );
    }

    fn install(self: Box<Self>, md: &mut MarkdownIt) {
        let leaf = Arc::new(*self);

        let run_leaf = Arc::clone(&leaf);
        let run = Arc::new(move |state: &mut InlineState<'_>| {
            let (source, document) = state.remaining_and_document();
            run_leaf
                .run(document, source)
                .map(|(node, len)| (Some(node), len))
        });

        let check_leaf = Arc::clone(&leaf);
        let check =
            Arc::new(move |state: &mut InlineState<'_>| check_leaf.check(state.remaining()));

        md.inline
            .add_dynamic_rule(
                crate::common::RuleMark::named(format!("dynamic:{}", leaf.rule)),
                leaf.marker,
                run,
                check,
            )
            .after_named("text")
            .before_all();
    }
}

/// Low-level inline rule for an [`InlinePlugin`].
///
/// [`PluginSpec::inline_leaf`] registers this automatically. It is public so
/// static leaves can also use the low-level rule ordering, lookup and removal
/// APIs.
pub struct LeafRule<P: InlinePlugin>(PhantomData<fn() -> P>);

impl<P: InlinePlugin> LeafRule<P> {
    pub(super) fn new() -> Self {
        Self(PhantomData)
    }

    fn parse(source: &str) -> Option<(P, usize)> {
        parse_leaf(P::NAME, source, P::parse)
    }
}

impl<P: InlinePlugin> PluginInstaller for LeafRule<P> {
    fn name(&self) -> &str {
        P::NAME
    }

    fn validate(&self) {
        assert!(
            !P::NAME.is_empty(),
            "inline plugin rule name must not be empty"
        );
    }

    fn validate_registration(&self, md: &MarkdownIt, context: &mut RegistrationContext) {
        assert!(
            !md.inline.has_rule::<LeafRule<P>>()
                && context.rules.insert(std::any::TypeId::of::<LeafRule<P>>()),
            "inline plugin {:?} was added more than once",
            std::any::type_name::<P>()
        );
    }

    fn install(self: Box<Self>, md: &mut MarkdownIt) {
        md.inline
            .add_rule::<LeafRule<P>>()
            .after_named("text")
            .before_all();
        md.add_renderer::<P, _>("html", LeafRenderer::<P>::default());
    }
}

impl<P: InlinePlugin> InlineRule for LeafRule<P> {
    const MARKER: char = P::MARKER;

    fn check(state: &mut InlineState<'_>) -> Option<usize> {
        Self::parse(state.remaining()).map(|(_, len)| len)
    }

    fn run(state: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        let (value, len) = Self::parse(state.remaining())?;
        let (_, document) = state.remaining_and_document();
        let node = document.create_node(value);
        Some((Some(node), len))
    }
}

/// HTML renderer for an [`InlinePlugin`].
///
/// [`PluginSpec::inline_leaf`] registers this automatically. Use it during
/// manual low-level registration with [`LeafRule`].
pub struct LeafRenderer<P: InlinePlugin> {
    marker: PhantomData<fn() -> P>,
}

impl<P: InlinePlugin> Default for LeafRenderer<P> {
    fn default() -> Self {
        Self {
            marker: PhantomData,
        }
    }
}

impl<P: InlinePlugin> NodeRenderer<P> for LeafRenderer<P> {
    fn render(
        &self,
        _node: crate::NodeRef<'_>,
        value: &P,
        context: &mut RenderContext<'_>,
        output: &mut Writer,
    ) {
        render_html_fragment(|html| value.render_html(html), context.options(), output);
    }
}

fn parse_leaf<T>(
    rule: &str,
    source: &str,
    parse: impl FnOnce(&mut InlinePluginState<'_>) -> Option<T>,
) -> Option<(T, usize)> {
    let mut cx = InlinePluginState::new(source);
    let value = parse(&mut cx)?;
    let consumed = cx.consumed();

    assert!(
        consumed > 0,
        "inline plugin rule {rule:?} succeeded without consuming input"
    );
    assert!(
        source.get(..consumed).is_some(),
        "inline plugin rule {rule:?} consumed past the source window"
    );

    Some((value, consumed))
}
