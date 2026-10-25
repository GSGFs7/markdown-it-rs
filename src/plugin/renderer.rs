use super::Html;
use super::html::render_html_fragment;
use super::spec::{PluginInstaller, RegistrationContext};
use crate::{MarkdownIt, NodeRef, NodeRenderer, NodeValue, RenderContext, Writer};

/// Builder state indicating that at least one output renderer is configured.
pub struct WithRenderer;

type InstallRenderer = Box<dyn FnOnce(&mut MarkdownIt) + Send>;

/// Installation-only registrations; parsing never retains these callbacks.
#[derive(Default)]
pub(super) struct Renderers(Vec<InstallRenderer>);

impl Renderers {
    pub fn add<T: NodeValue, R: NodeRenderer<T>>(
        &mut self,
        format: impl Into<String>,
        renderer: R,
    ) {
        let format = format.into();
        self.0.push(Box::new(move |md| {
            md.add_renderer::<T, _>(format, renderer);
        }));
    }

    pub fn attach<I: PluginInstaller + 'static>(self, installer: I) -> Box<dyn PluginInstaller> {
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

    fn install(self: Box<Self>, md: &mut MarkdownIt) {
        let Self {
            installer,
            renderers,
        } = *self;
        Box::new(installer).install(md);
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
    fn render(&self, _: NodeRef<'_>, value: &T, cx: &mut RenderContext<'_>, out: &mut Writer) {
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
    fn render(&self, _: NodeRef<'_>, value: &T, cx: &mut RenderContext<'_>, out: &mut Writer) {
        if BLOCK {
            cx.cr(out);
        }
        (self.0)(value, out);
        if BLOCK {
            cx.cr(out);
        }
    }
}
