use std::marker::PhantomData;

use super::Html;
use super::container_context::{ContainerHtml, ContainerText};
use super::html::render_html_fragment;
use super::spec::{PluginInstallContext, PluginInstaller, RegistrationContext};
use crate::MarkdownIt;
use crate::document::{Node, NodeValue};
use crate::render::{NodeRenderer, RenderContext, Writer};

/// Builder state indicating that at least one output renderer is configured.
pub struct WithRenderer;

type InstallRenderer = Box<dyn FnOnce(&mut MarkdownIt) + Send>;

/// Installation-only registrations; parsing never retains these callbacks.
#[derive(Default)]
pub(super) struct Renderers(Vec<InstallRenderer>);

impl Renderers {
    pub(super) fn add<T: NodeValue, R: NodeRenderer<T>>(
        &mut self,
        format: impl Into<String>,
        renderer: R,
    ) {
        let format = format.into();
        self.0.push(Box::new(move |md| {
            md.add_renderer::<T, _>(format, renderer);
        }));
    }

    pub(super) fn attach<I: PluginInstaller + 'static>(
        self,
        installer: I,
    ) -> Box<dyn PluginInstaller> {
        Box::new(RenderedInstaller {
            installer,
            renderers: self,
        })
    }
}

struct RenderedInstaller<I> {
    installer: I,
    renderers: Renderers,
}

impl<I: PluginInstaller + 'static> PluginInstaller for RenderedInstaller<I> {
    fn name(&self) -> &str {
        self.installer.name()
    }

    fn validate(&self) {
        self.installer.validate();
    }

    fn validate_registration(&self, md: &MarkdownIt, context: &mut RegistrationContext) {
        self.installer.validate_registration(md, context);
    }

    fn install(self: Box<Self>, md: &mut MarkdownIt, context: &PluginInstallContext<'_>) {
        let Self {
            installer,
            renderers,
        } = *self;
        Box::new(installer).install(md, context);
        for install in renderers.0 {
            install(md);
        }
    }
}

/// Adapter used by the HTML callback shortcuts.
pub(super) struct HtmlCallbackRenderer<F, const BLOCK: bool>(pub F);

impl<T, F, const BLOCK: bool> NodeRenderer<T> for HtmlCallbackRenderer<F, BLOCK>
where
    T: NodeValue,
    F: Fn(&T, &mut Html) + Send + Sync + 'static,
{
    fn render(&self, _: &Node, value: &T, cx: &mut RenderContext<'_>, out: &mut Writer) {
        if BLOCK {
            cx.cr(out);
        }
        render_html_fragment(|html| (self.0)(value, html), cx.options(), out);
        if BLOCK {
            cx.cr(out);
        }
    }
}

/// Adapter used by the plain-text callback shortcuts.
pub(super) struct TextCallbackRenderer<F, const BLOCK: bool>(pub F);

impl<T, F, const BLOCK: bool> NodeRenderer<T> for TextCallbackRenderer<F, BLOCK>
where
    T: NodeValue,
    F: Fn(&T, &mut Writer) + Send + Sync + 'static,
{
    fn render(&self, _: &Node, value: &T, cx: &mut RenderContext<'_>, out: &mut Writer) {
        if BLOCK {
            cx.cr(out);
        }
        (self.0)(value, out);
        if BLOCK {
            cx.cr(out);
        }
    }
}

/// Adapter used by container HTML callbacks.
///
/// `BLOCK` inserts the block line separators used by block containers.
pub(super) struct ContainerHtmlRenderer<F, T, const BLOCK: bool>(F, PhantomData<fn() -> T>);

impl<F, T, const BLOCK: bool> ContainerHtmlRenderer<F, T, BLOCK> {
    pub(super) fn new(render: F) -> Self {
        Self(render, PhantomData)
    }
}

impl<T, F, const BLOCK: bool> NodeRenderer<T> for ContainerHtmlRenderer<F, T, BLOCK>
where
    T: NodeValue,
    F: for<'a, 'b> Fn(&T, &mut ContainerHtml<'a, 'b>) + Send + Sync + 'static,
{
    fn render(&self, node: &Node, value: &T, context: &mut RenderContext<'_>, output: &mut Writer) {
        if BLOCK {
            context.cr(output);
        }
        let mut html = Html::new();
        {
            let mut cx = ContainerHtml::new(&mut html, context, node.id());
            (self.0)(value, &mut cx);
        }
        html.render_into(output, context.options());
        if BLOCK {
            context.cr(output);
        }
    }
}

/// Adapter used by container plain-text callbacks.
///
/// `BLOCK` inserts the block line separators used by block containers.
pub(super) struct ContainerTextRenderer<F, T, const BLOCK: bool>(F, PhantomData<fn() -> T>);

impl<F, T, const BLOCK: bool> ContainerTextRenderer<F, T, BLOCK> {
    pub(super) fn new(render: F) -> Self {
        Self(render, PhantomData)
    }
}

impl<T, F, const BLOCK: bool> NodeRenderer<T> for ContainerTextRenderer<F, T, BLOCK>
where
    T: NodeValue,
    F: for<'a, 'b> Fn(&T, &mut ContainerText<'a, 'b>) + Send + Sync + 'static,
{
    fn render(&self, node: &Node, value: &T, context: &mut RenderContext<'_>, output: &mut Writer) {
        if BLOCK {
            context.cr(output);
        }
        let mut cx = ContainerText::new(output, context, node.id());
        (self.0)(value, &mut cx);
        if BLOCK {
            context.cr(output);
        }
    }
}
