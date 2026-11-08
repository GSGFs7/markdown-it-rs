use std::marker::PhantomData;
use std::sync::Arc;

use crate::MarkdownIt;
use crate::document::{Document, NodeId, NodeValue};
use crate::parser::inline::InlineState;
use crate::plugin::renderer::{
    HtmlCallbackRenderer,
    Renderers,
    TextCallbackRenderer,
    WithRenderer,
};
use crate::plugin::spec::{PluginInstallContext, PluginInstaller};
use crate::plugin::{Html, InlinePluginState, PluginSpec};
use crate::render::{NodeRenderer, Writer};

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
    pub(in crate::plugin) fn new(plugin: PluginSpec, rule: String) -> Self {
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

    /// Set the parser. Returning `None` rolls back local consumption and a
    /// successful parse must consume at least one byte.
    ///
    /// The callback may run during boundary probing as well as actual parsing.
    /// Returning `None` only rolls back the local cursor, not external side
    /// effects, so keep the callback free of observable side effects.
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
    /// use markdown_it::MarkdownIt;
    /// use markdown_it::plugin::PluginSpec;
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
    /// assert_eq!(md.render_document(&document, "html"), "<p>Go 🚀!</p>\n");
    /// assert_eq!(md.render_document(&document, "text"), "Go [rocket]!\n");
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
    fn parse(&self, source: &str, markdown_it: &MarkdownIt) -> Option<(T, usize)> {
        parse_leaf(&self.rule, source, markdown_it, |cx| (self.parse)(cx))
    }

    fn run(
        &self,
        document: &mut Document,
        source: &str,
        markdown_it: &MarkdownIt,
    ) -> Option<(NodeId, usize)> {
        let (value, consumed) = self.parse(source, markdown_it)?;
        Some((document.create_node(value), consumed))
    }

    fn check(&self, source: &str, markdown_it: &MarkdownIt) -> Option<usize> {
        self.parse(source, markdown_it)
            .map(|(_, consumed)| consumed)
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

    fn install(self: Box<Self>, md: &mut MarkdownIt, context: &PluginInstallContext<'_>) {
        let leaf = Arc::new(*self);

        let run_leaf = Arc::clone(&leaf);
        let run = Arc::new(move |state: &mut InlineState<'_>| {
            let md = state.markdown_it();
            let (source, document) = state.remaining_and_document();
            run_leaf
                .run(document, source, md)
                .map(|(node, len)| (Some(node), len))
        });

        let check_leaf = Arc::clone(&leaf);
        let check = Arc::new(move |state: &mut InlineState<'_>| {
            let md = state.markdown_it();
            check_leaf.check(state.remaining(), md)
        });

        md.inline
            .add_dynamic_rule(
                crate::plugin::identity::rule_mark("inline", context.name, &leaf.rule),
                leaf.marker,
                run,
                check,
            )
            .after_named("text")
            .before_all();
    }
}

fn parse_leaf<T>(
    rule: &str,
    source: &str,
    markdown_it: &MarkdownIt,
    parse: impl FnOnce(&mut InlinePluginState<'_>) -> Option<T>,
) -> Option<(T, usize)> {
    let mut cx = InlinePluginState::new(source, markdown_it);
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
