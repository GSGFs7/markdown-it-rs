use std::marker::PhantomData;

use super::PluginSpec;
use super::spec::{PluginInstaller, RegistrationContext};
use crate::MarkdownIt;
use crate::document::{Document, NodeId, NodeValue};
use crate::parser::inline::helpers::emph_pair;
use crate::render::{NodeRenderer, RenderContext, Writer};

/// Compile-time delimiter configuration for an inline pair.
pub struct PairDelimiter<const MARKER: char, const LENGTH: u8, const SPLIT: bool>;

/// Builds one emphasis-style inline pair inside a [`PluginSpec`].
///
/// A pair wraps its parsed children in an HTML tag, using the same delimiter
/// scanner and matching algorithm as [`emph_pair`](crate::parser::inline::helpers::emph_pair).
/// [`delimiter`](Self::delimiter), [`value`](Self::value) and
/// [`html_tag`](Self::html_tag) are required before [`finish`](Self::finish) is
/// available.
///
/// Delimiter parameters are compile-time constants; the value factory may
/// capture runtime configuration. Installation registers the corresponding
/// low-level `EmphPairScanner` and its HTML renderer.
#[must_use = "call finish() to add this pair to its plugin"]
pub struct InlinePairBuilder<T: NodeValue, D = (), V = (), H = ()> {
    plugin: PluginSpec,
    rule: String,
    value: V,
    html_tag: H,
    state: PhantomData<fn() -> (T, D)>,
}

impl<T: NodeValue> InlinePairBuilder<T> {
    pub(super) fn new(plugin: PluginSpec, rule: String) -> Self {
        Self {
            plugin,
            rule,
            value: (),
            html_tag: (),
            state: PhantomData,
        }
    }
}

impl<T: NodeValue, D, V, H> InlinePairBuilder<T, D, V, H> {
    /// Set the delimiter, length (1, 2 or 3), and whether it may split words.
    ///
    /// These parameters select the low-level emphasis rule at compile time.
    pub fn delimiter<const MARKER: char, const LENGTH: u8, const SPLIT: bool>(
        self,
    ) -> InlinePairBuilder<T, PairDelimiter<MARKER, LENGTH, SPLIT>, V, H> {
        assert!(
            (1..=3).contains(&LENGTH),
            "inline pair delimiter length must be 1, 2 or 3"
        );
        InlinePairBuilder {
            plugin: self.plugin,
            rule: self.rule,
            value: self.value,
            html_tag: self.html_tag,
            state: PhantomData,
        }
    }

    /// Set the value stored in each pair node.
    pub fn value<F>(self, value: F) -> InlinePairBuilder<T, D, F, H>
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        InlinePairBuilder {
            plugin: self.plugin,
            rule: self.rule,
            value,
            html_tag: self.html_tag,
            state: PhantomData,
        }
    }

    /// Set the HTML tag used to render the pair's children.
    pub fn html_tag(self, tag: impl Into<String>) -> InlinePairBuilder<T, D, V, String> {
        InlinePairBuilder {
            plugin: self.plugin,
            rule: self.rule,
            value: self.value,
            html_tag: tag.into(),
            state: PhantomData,
        }
    }
}

impl<T, V, const MARKER: char, const LENGTH: u8, const SPLIT: bool>
    InlinePairBuilder<T, PairDelimiter<MARKER, LENGTH, SPLIT>, V, String>
where
    T: NodeValue,
    V: Fn() -> T + Send + Sync + 'static,
{
    /// Add the pair to its owning plugin and resume building that plugin.
    pub fn finish(self) -> PluginSpec {
        self.plugin.with_installer(Box::new(ConfiguredInlinePair::<
            T,
            V,
            MARKER,
            LENGTH,
            SPLIT,
        > {
            rule: self.rule,
            value: self.value,
            html_tag: self.html_tag,
            payload: PhantomData,
        }))
    }
}

struct ConfiguredInlinePair<T, V, const MARKER: char, const LENGTH: u8, const SPLIT: bool> {
    rule: String,
    value: V,
    html_tag: String,
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
        assert!(
            !self.html_tag.is_empty()
                && self
                    .html_tag
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':')),
            "invalid HTML tag name: {:?}",
            self.html_tag
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
        md.add_renderer::<T, _>("html", InlinePairRenderer { tag: self.html_tag });
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
