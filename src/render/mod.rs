//! Format-specific rendering for documents.

mod options;

use std::any::TypeId;
use std::collections::HashMap;
use std::fmt;
use std::hash::BuildHasherDefault;
use std::marker::PhantomData;

pub use options::RenderOptions;

use crate::common::extset::RenderExtSet;
use crate::common::typekey::TypeIdHasher;
use crate::common::utils::escape_html;
use crate::document::{
    Document,
    HtmlAttribute,
    Node,
    NodeId,
    NodeRef,
    NodeValue,
    Root,
    StructuralEvent,
    Text,
    TextSpecial,
};

// --- protocol ---

/// Renderer for one payload type in one output format.
///
/// ```
/// use markdown_it::{
///     Document, NodeRenderer, RenderContext,
///     Writer, MarkdownIt, NodeDraft, NodeRef,
/// };
///
/// #[derive(Debug)]
/// struct Badge(&'static str);
///
/// struct BadgeRenderer;
/// impl NodeRenderer<Badge> for BadgeRenderer {
///     fn render(
///         &self,
///         _: NodeRef<'_>,
///         badge: &Badge,
///         _: &mut RenderContext<'_>,
///         output: &mut Writer,
///     ) {
///         output.write_str(badge.0);
///     }
/// }
///
/// let mut md = MarkdownIt::empty();
/// md.add_renderer::<Badge, _>("html", BadgeRenderer);
/// let document = Document::from_draft("", NodeDraft::new(Badge("new")));
/// assert_eq!(md.render_html(&document), "new");
/// ```
pub trait NodeRenderer<T: NodeValue>: Send + Sync + 'static {
    fn render(
        &self,
        node: NodeRef<'_>,
        value: &T,
        context: &mut RenderContext<'_>,
        output: &mut Writer,
    );
}

trait ErasedNodeRenderer: Send + Sync {
    fn render(&self, node: NodeRef<'_>, context: &mut RenderContext<'_>, output: &mut Writer);
}

// --- machinery ---

type FormatRenderers =
    HashMap<TypeId, std::sync::Arc<dyn ErasedNodeRenderer>, BuildHasherDefault<TypeIdHasher>>;

struct TypedNodeRenderer<T, R> {
    renderer: R,
    marker: PhantomData<fn() -> T>,
}

impl<T, R> ErasedNodeRenderer for TypedNodeRenderer<T, R>
where
    T: NodeValue,
    R: NodeRenderer<T>,
{
    fn render(&self, node: NodeRef<'_>, context: &mut RenderContext<'_>, output: &mut Writer) {
        let value = node
            .cast::<T>()
            .expect("renderer registry type and payload type must agree");
        self.renderer.render(node, value, context, output);
    }
}

// --- registry ---

/// Registry keyed by output format and concrete node payload type.
///
/// Adding the same pair again replaces the previous renderer and returns
/// `true`. This gives applications an explicit override mechanism.
#[derive(Default, Clone)]
pub struct RendererRegistry {
    formats: HashMap<String, FormatRenderers>,
}

impl fmt::Debug for RendererRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RendererRegistry")
            .field("formats", &self.formats.len())
            .field(
                "renderers",
                &self.formats.values().map(HashMap::len).sum::<usize>(),
            )
            .finish()
    }
}

impl RendererRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register or replace the renderer for `T` in `format`.
    pub fn add<T, R>(&mut self, format: impl Into<String>, renderer: R) -> bool
    where
        T: NodeValue,
        R: NodeRenderer<T>,
    {
        self.formats
            .entry(format.into())
            .or_default()
            .insert(
                TypeId::of::<T>(),
                std::sync::Arc::new(TypedNodeRenderer::<T, R> {
                    renderer,
                    marker: PhantomData,
                }),
            )
            .is_some()
    }

    pub fn contains<T: NodeValue>(&self, format: &str) -> bool {
        self.formats
            .get(format)
            .is_some_and(|renderers| renderers.contains_key(&TypeId::of::<T>()))
    }

    pub fn remove<T: NodeValue>(&mut self, format: &str) -> bool {
        let Some(renderers) = self.formats.get_mut(format) else {
            return false;
        };
        let removed = renderers.remove(&TypeId::of::<T>()).is_some();
        if renderers.is_empty() {
            self.formats.remove(format);
        }
        removed
    }

    /// Render a document.
    ///
    /// # Panics
    ///
    /// Panics if a leaf node has no renderer registered for `format`.
    pub fn render(&self, document: &Document, format: &str, options: &RenderOptions) -> String {
        self.render_subtree(document, document.root(), format, options)
    }

    /// Render a subtree using the same format registry.
    pub fn render_subtree(
        &self,
        document: &Document,
        root: NodeId,
        format: &str,
        options: &RenderOptions,
    ) -> String {
        let mut ext = RenderExtSet::new();
        let mut output = Writer::new();
        let shared = RenderShared {
            document,
            renderers: self.formats.get(format),
            format,
            options,
        };
        render_node(&shared, &mut ext, root, &mut output);
        output.finish()
    }
}

// --- engine ---

struct RenderShared<'a> {
    document: &'a Document,
    renderers: Option<&'a FormatRenderers>,
    format: &'a str,
    options: &'a RenderOptions,
}

/// Output buffer supplied to [`NodeRenderer`] implementations.
///
/// The inherent [`write_str`](Self::write_str), [`write_char`](Self::write_char),
/// and [`write_fmt`](Self::write_fmt) methods return `()` and panic on a
/// formatting error. The type also implements [`fmt::Write`], whose methods
/// return [`fmt::Result`], for interoperability with generic code.
#[derive(Debug, Default)]
pub struct Writer {
    output: String,
}

impl Writer {
    fn new() -> Self {
        Self::default()
    }

    fn cr(&mut self) {
        if !self.output.is_empty() && !self.output.ends_with('\n') {
            self.output.push('\n');
        }
    }

    pub(crate) fn finish(self) -> String {
        self.output
    }

    /// Append a string to the output buffer.
    pub fn write_str(&mut self, value: &str) {
        self.output.push_str(value);
    }

    /// Append a char to the output buffer.
    pub fn write_char(&mut self, value: char) {
        self.output.push(value);
    }

    /// Avoid scalar entity detection for long text that needs no escaping.
    #[inline]
    pub(crate) fn write_escaped_html(&mut self, value: &str) {
        if value.len() >= 32
            && memchr::memchr3(b'&', b'<', b'>', value.as_bytes()).is_none()
            && memchr::memchr(b'"', value.as_bytes()).is_none()
        {
            self.output.push_str(value);
        } else {
            self.output.push_str(&escape_html(value));
        }
    }

    /// Append formatted output.
    ///
    /// # Panics
    /// Panic if a formatting implementation returns an error.
    pub fn write_fmt(&mut self, args: fmt::Arguments<'_>) {
        fmt::write(&mut self.output, args)
            .expect("a formatting implementation failed while rendering");
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.output.push_str(s);
        Ok(())
    }
}

/// Recursive services shared by node renderers for one render operation.
pub struct RenderContext<'a> {
    shared: &'a RenderShared<'a>,
    ext: &'a mut RenderExtSet,
}

impl<'a> RenderContext<'a> {
    /// The document being rendered.
    pub fn document(&self) -> &'a Document {
        self.shared.document
    }

    /// The active output format, e.g. `"html"`.
    pub fn format(&self) -> &str {
        self.shared.format
    }

    /// Render options for the active format.
    pub fn options(&self) -> &RenderOptions {
        self.shared.options
    }

    /// Extension set for renderer-specific state.
    pub fn ext(&mut self) -> &mut RenderExtSet {
        self.ext
    }

    /// Write one line ending unless the output is already at the start of a line.
    pub fn cr(&mut self, output: &mut Writer) {
        output.cr();
    }

    /// Render one node's shell using its registered renderer.
    ///
    /// # Panics
    ///
    /// Panics if `node` is not a valid [`NodeId`] in this document, if a leaf
    /// has no renderer for the active format, or if a renderer or formatting
    /// implementation panics.
    pub fn render_node(&mut self, node: NodeId, output: &mut Writer) {
        render_node(self.shared, self.ext, node, output);
    }

    /// Render all direct children of `node` in order.
    ///
    /// # Panics
    ///
    /// Panics if `node` is not a valid [`NodeId`] in this document, if a leaf
    /// descendant has no renderer for the active format, or if a renderer or
    /// formatting implementation panics.
    pub fn render_children(&mut self, node: NodeId, output: &mut Writer) {
        for &child in self.shared.document.children(node) {
            stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
                render_node(self.shared, self.ext, child, output);
            });
        }
    }
}

// recursive rendering nodes
fn render_node(shared: &RenderShared<'_>, ext: &mut RenderExtSet, id: NodeId, output: &mut Writer) {
    let node = shared.document.node(id);
    let Some(renderer) = shared
        .renderers
        .and_then(|renderers| renderers.get(&node.type_id()))
    else {
        // leaf but no renderer
        assert!(
            !node.children().is_empty(),
            "no {:?} renderer registered for leaf {} at {:?}",
            shared.format,
            node.name(),
            id,
        );

        // transparent downward traversal
        for &child in node.children() {
            stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
                render_node(shared, ext, child, output);
            });
        }
        return;
    };

    // render the shell (<h1>,<em>,...)
    let mut context = RenderContext { shared, ext };
    renderer.render(node, &mut context, output);
}

// --- builtin renderers ---

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

pub(crate) fn render_html_link(
    node: NodeRef<'_>,
    url: &str,
    title: Option<&str>,
    context: &mut RenderContext<'_>,
    output: &mut Writer,
) {
    let title = title.filter(|title| !title.is_empty());
    let extra = [("href", url), ("title", title.unwrap_or_default())];
    output.write_str("<a");
    write_html_attrs_with(
        output,
        node.attrs(),
        &extra[..1 + usize::from(title.is_some())],
    );
    output.write_char('>');
    context.render_children(node.id(), output);
    write_html_close(output, "a");
}

pub(crate) fn write_html_attrs(output: &mut Writer, attrs: &[HtmlAttribute]) {
    if attrs.is_empty() {
        return;
    }
    write_html_attrs_with(output, attrs, &[]);
}

// avoid attr clone
pub(crate) fn write_html_attrs_with(
    output: &mut Writer,
    attrs: &[HtmlAttribute],
    extra: &[(&str, &str)],
) {
    const LINEAR_SCAN_LIMIT: usize = 8;

    let count = attrs.len() + extra.len();
    let attrs = attrs
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .chain(extra.iter().copied());

    // small list, using linear scan. avoid hashmap overhead.
    // O(n^2), but very fast with small data.
    if count <= LINEAR_SCAN_LIMIT {
        for (index, (name, _)) in attrs.clone().enumerate() {
            // deduplication
            if attrs
                .clone()
                .take(index)
                .any(|(previous, _)| previous == name)
            {
                continue;
            }
            write_html_attr_group(
                output,
                name,
                attrs
                    .clone()
                    .filter_map(|(candidate, value)| (candidate == name).then_some(value)),
            );
        }
        return;
    }

    let mut values = HashMap::<&str, Vec<&str>>::new();
    let mut order = Vec::with_capacity(count);
    for (name, value) in attrs {
        values.entry(name).or_default().push(value);
        order.push(name);
    }
    for name in order {
        let Some(parts) = values.remove(name) else {
            continue;
        };
        write_html_attr_group(output, name, parts.into_iter());
    }
}

fn write_html_attr_group<'a>(
    output: &mut Writer,
    name: &str,
    values: impl Iterator<Item = &'a str>,
) {
    if name == "class" || name == "style" {
        output.write_char(' ');
        output.write_str(&escape_html(name));
        output.write_str("=\"");
        let separator = if name == "class" { ' ' } else { ';' };
        for (index, value) in values.enumerate() {
            if index != 0 {
                output.write_char(separator);
            }
            output.write_str(&escape_html(value));
        }
        output.write_char('"');
    } else {
        for value in values {
            output.write_char(' ');
            output.write_str(&escape_html(name));
            output.write_str("=\"");
            output.write_str(&escape_html(value));
            output.write_char('"');
        }
    }
}

pub(crate) fn write_html_open(output: &mut Writer, tag: &str, attrs: &[HtmlAttribute]) {
    output.write_char('<');
    output.write_str(tag);
    write_html_attrs(output, attrs);
    output.write_char('>');
}

pub(crate) fn write_html_close(output: &mut Writer, tag: &str) {
    output.write_str("</");
    output.write_str(tag);
    output.write_char('>');
}

pub(crate) fn write_html_self_close(
    output: &mut Writer,
    tag: &str,
    attrs: &[HtmlAttribute],
    xhtml: bool,
) {
    output.write_char('<');
    output.write_str(tag);
    write_html_attrs(output, attrs);
    if xhtml {
        output.write_str(" /");
    }
    output.write_char('>');
}

pub(crate) fn write_html_text(output: &mut Writer, value: &str) {
    output.write_escaped_html(value);
}

#[cfg(test)]
mod tests;
