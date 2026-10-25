use std::marker::PhantomData;

use super::PluginSpec;
use super::renderer::{Renderers, WithRenderer};
use super::spec::{PluginInstaller, RegistrationContext};
use crate::MarkdownIt;
use crate::document::{Document, NodeId, NodeValue};
use crate::parser::inline::helpers::emph_pair;
use crate::render::{NodeRenderer, RenderContext, Writer};

/// Compile-time delimiter configuration for an inline pair.
pub struct PairDelimiter<const MARKER: char, const LENGTH: u8, const SPLIT: bool>;

/// Builds one emphasis-style inline pair inside a [`PluginSpec`].
///
/// A pair contains parsed inline children, using the same delimiter
/// scanner and matching algorithm as [`emph_pair`](crate::parser::inline::helpers::emph_pair).
/// [`delimiter`](Self::delimiter), [`value`](Self::value) and
/// a renderer ([`html_tag`](Self::html_tag) or [`render_with`](Self::render_with))
/// are required before [`finish`](Self::finish) is available.
///
/// Delimiter parameters are compile-time constants; the value factory may
/// capture runtime configuration. Installation registers the corresponding
/// low-level `EmphPairScanner` and the configured renderers.
#[must_use = "call finish() to add this pair to its plugin"]
pub struct InlinePairBuilder<T: NodeValue, D = (), V = (), R = ()> {
    plugin: PluginSpec,
    rule: String,
    value: V,
    render: R,
    renderers: Renderers,
    state: PhantomData<fn() -> (T, D)>,
}

impl<T: NodeValue> InlinePairBuilder<T> {
    pub(super) fn new(plugin: PluginSpec, rule: String) -> Self {
        Self {
            plugin,
            rule,
            value: (),
            render: (),
            renderers: Renderers::default(),
            state: PhantomData,
        }
    }
}

impl<T: NodeValue, D, V, R> InlinePairBuilder<T, D, V, R> {
    /// Set the delimiter, length (1, 2 or 3), and whether it may split words.
    ///
    /// These parameters select the low-level emphasis rule at compile time.
    pub fn delimiter<const MARKER: char, const LENGTH: u8, const SPLIT: bool>(
        self,
    ) -> InlinePairBuilder<T, PairDelimiter<MARKER, LENGTH, SPLIT>, V, R> {
        assert!(
            (1..=3).contains(&LENGTH),
            "inline pair delimiter length must be 1, 2 or 3"
        );
        InlinePairBuilder {
            plugin: self.plugin,
            rule: self.rule,
            value: self.value,
            render: self.render,
            renderers: self.renderers,
            state: PhantomData,
        }
    }

    /// Set the value stored in each pair node.
    pub fn value<F>(self, value: F) -> InlinePairBuilder<T, D, F, R>
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        InlinePairBuilder {
            plugin: self.plugin,
            rule: self.rule,
            value,
            render: self.render,
            renderers: self.renderers,
            state: PhantomData,
        }
    }

    /// Register an HTML renderer that wraps the pair's children in `tag`.
    ///
    /// This is a shortcut for [`render_with`](Self::render_with). The last
    /// renderer registered for a payload type and format wins.
    ///
    /// # Panics
    ///
    /// Panics if `tag` is not a valid ASCII HTML tag name.
    pub fn html_tag(self, tag: impl Into<String>) -> InlinePairBuilder<T, D, V, WithRenderer> {
        let tag = tag.into();
        super::html::validate_tag(&tag);
        self.render_with("html", InlinePairRenderer { tag })
    }

    /// Register a low-level renderer for an output format.
    ///
    /// May be called repeatedly for different formats. The last registration
    /// for a payload type and format wins, just like [`MarkdownIt::add_renderer`].
    /// The renderer can use [`RenderContext::render_children`] to emit the
    /// pair's parsed children in the active output format.
    ///
    /// ```
    /// use markdown_it::{MarkdownIt, NodeRef, NodeRenderer, PluginSpec, RenderContext, Writer};
    ///
    /// #[derive(Debug)]
    /// struct Spoiler;
    /// struct SpoilerRenderer;
    /// impl NodeRenderer<Spoiler> for SpoilerRenderer {
    ///     fn render(&self, node: NodeRef<'_>, _: &Spoiler, cx: &mut RenderContext<'_>, out: &mut Writer) {
    ///         out.write_str("<span class=\"spoiler\">");
    ///         cx.render_children(node.id(), out);
    ///         out.write_str("</span>");
    ///     }
    /// }
    ///
    /// let spoiler = PluginSpec::new("spoiler")
    ///     .inline_pair::<Spoiler>("spoiler")
    ///     .delimiter::<'|', 2, true>()
    ///     .value(|| Spoiler)
    ///     .render_with("html", SpoilerRenderer)
    ///     .finish();
    /// let md = MarkdownIt::builder().plugin(spoiler).build();
    /// assert_eq!(md.render("||**secret**||"),
    ///     "<p><span class=\"spoiler\"><strong>secret</strong></span></p>\n");
    /// ```
    pub fn render_with<N: NodeRenderer<T>>(
        mut self,
        format: impl Into<String>,
        renderer: N,
    ) -> InlinePairBuilder<T, D, V, WithRenderer> {
        self.renderers.add::<T, _>(format, renderer);
        InlinePairBuilder {
            plugin: self.plugin,
            rule: self.rule,
            value: self.value,
            render: WithRenderer,
            renderers: self.renderers,
            state: PhantomData,
        }
    }
}

impl<T, V, const MARKER: char, const LENGTH: u8, const SPLIT: bool>
    InlinePairBuilder<T, PairDelimiter<MARKER, LENGTH, SPLIT>, V, WithRenderer>
where
    T: NodeValue,
    V: Fn() -> T + Send + Sync + 'static,
{
    /// Add the pair to its owning plugin and resume building that plugin.
    pub fn finish(self) -> PluginSpec {
        let installer = ConfiguredInlinePair::<T, V, MARKER, LENGTH, SPLIT> {
            rule: self.rule,
            value: self.value,
            payload: PhantomData,
        };
        self.plugin.with_installer(self.renderers.attach(installer))
    }
}

struct ConfiguredInlinePair<T, V, const MARKER: char, const LENGTH: u8, const SPLIT: bool> {
    rule: String,
    value: V,
    payload: PhantomData<fn() -> T>,
}

impl<T, V, const MARKER: char, const LENGTH: u8, const SPLIT: bool> PluginInstaller
    for ConfiguredInlinePair<T, V, MARKER, LENGTH, SPLIT>
where
    T: NodeValue,
    V: Fn() -> T + Send + Sync + 'static,
{
    fn name(&self) -> &str {
        &self.rule
    }

    fn validate(&self) {
        assert!(
            !self.rule.is_empty(),
            "inline pair rule name must not be empty"
        );
        assert!(
            (1..=3).contains(&LENGTH),
            "inline pair delimiter length must be 1, 2 or 3"
        );
    }

    fn validate_registration(&self, md: &MarkdownIt, context: &mut RegistrationContext) {
        context
            .pairs
            .entry(MARKER)
            .or_insert_with(|| emph_pair::registration::<MARKER>(md))
            .claim(MARKER, LENGTH, SPLIT);
    }

    fn install(self: Box<Self>, md: &mut MarkdownIt) {
        let value = self.value;
        emph_pair::add_with_factory::<MARKER, LENGTH, SPLIT>(
            md,
            move |document: &mut Document| -> NodeId { document.create_node(value()) },
        );
    }
}

struct InlinePairRenderer {
    tag: String,
}

impl<T: NodeValue> NodeRenderer<T> for InlinePairRenderer {
    fn render(
        &self,
        node: crate::NodeRef<'_>,
        _value: &T,
        context: &mut RenderContext<'_>,
        output: &mut Writer,
    ) {
        crate::render::write_html_open(output, &self.tag, &[]);
        context.render_children(node.id(), output);
        crate::render::write_html_close(output, &self.tag);
    }
}
