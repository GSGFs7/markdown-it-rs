//! Format-specific rendering for arena-backed documents.

mod options;

use std::any::TypeId;
use std::cell::RefCell;
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
    DocumentNode,
    HtmlAttribute,
    NodeId,
    NodeRef,
    NodeValue,
    Root,
    StructuralEvent,
};

// --- protocol ---

/// Renderer for one payload type in one output format.
///
/// ```
/// use markdown_it::{
///     Document, DocumentNodeRenderer, DocumentRenderContext,
///     DocumentWriter, MarkdownIt, NodeDraft, NodeRef, NodeValue,
/// };
///
/// #[derive(Debug)]
/// struct Badge(&'static str);
/// impl NodeValue for Badge {}
///
/// struct BadgeRenderer;
/// impl DocumentNodeRenderer<Badge> for BadgeRenderer {
///     fn render(
///         &self,
///         _: NodeRef<'_>,
///         badge: &Badge,
///         _: &mut DocumentRenderContext<'_>,
///         output: &mut DocumentWriter,
///     ) {
///         output.write_str(badge.0);
///     }
/// }
///
/// let mut md = MarkdownIt::empty();
/// md.add_document_renderer::<Badge, _>("html", BadgeRenderer);
/// let document = Document::from_draft("", NodeDraft::new(Badge("new")));
/// assert_eq!(md.render_document(&document), "new");
/// ```
pub trait DocumentNodeRenderer<T: NodeValue>: Send + Sync + 'static {
    fn render(
        &self,
        node: NodeRef<'_>,
        value: &T,
        context: &mut DocumentRenderContext<'_>,
        output: &mut DocumentWriter,
    );
}

trait ErasedDocumentNodeRenderer: Send + Sync {
    fn render(
        &self,
        node: NodeRef<'_>,
        context: &mut DocumentRenderContext<'_>,
        output: &mut DocumentWriter,
    );
}

// --- machinery ---

type FormatRenderers = HashMap<
    TypeId,
    std::sync::Arc<dyn ErasedDocumentNodeRenderer>,
    BuildHasherDefault<TypeIdHasher>,
>;

struct TypedDocumentNodeRenderer<T, R> {
    renderer: R,
    marker: PhantomData<fn() -> T>,
}

impl<T, R> ErasedDocumentNodeRenderer for TypedDocumentNodeRenderer<T, R>
where
    T: NodeValue,
    R: DocumentNodeRenderer<T>,
{
    fn render(
        &self,
        node: NodeRef<'_>,
        context: &mut DocumentRenderContext<'_>,
        output: &mut DocumentWriter,
    ) {
        let value = node
            .cast::<T>()
            .expect("document renderer registry type and payload type must agree");
        self.renderer.render(node, value, context, output);
    }
}

// --- registry ---

/// Registry keyed by output format and concrete node payload type.
///
/// Adding the same pair again replaces the previous renderer and returns
/// `true`. This gives applications an explicit override mechanism.
#[derive(Default, Clone)]
pub struct DocumentRendererRegistry {
    formats: HashMap<String, FormatRenderers>,
}

impl fmt::Debug for DocumentRendererRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DocumentRendererRegistry")
            .field("formats", &self.formats.len())
            .field(
                "renderers",
                &self.formats.values().map(HashMap::len).sum::<usize>(),
            )
            .finish()
    }
}

impl DocumentRendererRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register or replace the renderer for `T` in `format`.
    pub fn add<T, R>(&mut self, format: impl Into<String>, renderer: R) -> bool
    where
        T: NodeValue,
        R: DocumentNodeRenderer<T>,
    {
        self.formats
            .entry(format.into())
            .or_default()
            .insert(
                TypeId::of::<T>(),
                std::sync::Arc::new(TypedDocumentNodeRenderer::<T, R> {
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

    /// Render an arena-backed document.
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
        let mut output = DocumentWriter::new();
        let shared = RenderShared {
            document,
            renderers: self.formats.get(format),
            format,
            options,
            scratch_nodes: RefCell::new(Vec::new()),
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
    scratch_nodes: RefCell<Vec<NodeId>>,
}

/// Output buffer supplied to [`DocumentNodeRenderer`] implementations.
///
/// The inherent [`write_str`](Self::write_str), [`write_char`](Self::write_char),
/// and [`write_fmt`](Self::write_fmt) methods return `()` and panic on a
/// formatting error. The type also implements [`fmt::Write`], whose methods
/// return [`fmt::Result`], for interoperability with generic code.
#[derive(Debug, Default)]
pub struct DocumentWriter {
    output: String,
}

impl DocumentWriter {
    fn new() -> Self {
        Self::default()
    }

    fn cr(&mut self) {
        if !self.output.is_empty() && !self.output.ends_with('\n') {
            self.output.push('\n');
        }
    }

    fn finish(self) -> String {
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

impl fmt::Write for DocumentWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.output.push_str(s);
        Ok(())
    }
}

/// Recursive services shared by node renderers for one render operation.
pub struct DocumentRenderContext<'a> {
    shared: &'a RenderShared<'a>,
    ext: &'a mut RenderExtSet,
}

impl<'a> DocumentRenderContext<'a> {
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

    // reduce memory overhead
    pub(crate) fn with_scratch_node_stack<R>(
        &self,
        f: impl FnOnce(&Document, &mut Vec<NodeId>) -> R,
    ) -> R {
        let mut stack = self.shared.scratch_nodes.borrow_mut();
        stack.clear();
        let result = f(self.shared.document, &mut stack);
        stack.clear();
        result
    }

    /// Write one line ending unless the output is already at the start of a line.
    pub fn cr(&mut self, output: &mut DocumentWriter) {
        output.cr();
    }

    /// Render one node's shell using its registered renderer.
    ///
    /// # Panics
    ///
    /// Panics if `node` is not a valid [`NodeId`] in this document, if a leaf
    /// has no renderer for the active format, or if a renderer or formatting
    /// implementation panics.
    pub fn render_node(&mut self, node: NodeId, output: &mut DocumentWriter) {
        render_node(self.shared, self.ext, node, output);
    }

    /// Render all direct children of `node` in order.
    ///
    /// # Panics
    ///
    /// Panics if `node` is not a valid [`NodeId`] in this document, if a leaf
    /// descendant has no renderer for the active format, or if a renderer or
    /// formatting implementation panics.
    pub fn render_children(&mut self, node: NodeId, output: &mut DocumentWriter) {
        for &child in self.shared.document.children(node) {
            stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
                render_node(self.shared, self.ext, child, output);
            });
        }
    }
}

// recursive rendering nodes
fn render_node(
    shared: &RenderShared<'_>,
    ext: &mut RenderExtSet,
    id: NodeId,
    output: &mut DocumentWriter,
) {
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
    let mut context = DocumentRenderContext { shared, ext };
    renderer.render(node, &mut context, output);
}

// --- builtin renderers ---

pub(crate) struct TransparentDocumentRenderer;

impl<T: NodeValue> DocumentNodeRenderer<T> for TransparentDocumentRenderer {
    fn render(
        &self,
        node: &DocumentNode,
        _: &T,
        context: &mut DocumentRenderContext<'_>,
        output: &mut DocumentWriter,
    ) {
        context.render_children(node.id(), output);
    }
}

pub(crate) struct EmptyDocumentRenderer;

impl<T: NodeValue> DocumentNodeRenderer<T> for EmptyDocumentRenderer {
    fn render(
        &self,
        _: &DocumentNode,
        _: &T,
        _: &mut DocumentRenderContext<'_>,
        _: &mut DocumentWriter,
    ) {
    }
}

pub(crate) struct HtmlTextDocumentRenderer;

impl<T> DocumentNodeRenderer<T> for HtmlTextDocumentRenderer
where
    T: NodeValue + AsRef<str>,
{
    fn render(
        &self,
        _: &DocumentNode,
        value: &T,
        _: &mut DocumentRenderContext<'_>,
        output: &mut DocumentWriter,
    ) {
        output.write_escaped_html(value.as_ref());
    }
}

pub(crate) struct PlainTextDocumentRenderer;

impl<T> DocumentNodeRenderer<T> for PlainTextDocumentRenderer
where
    T: NodeValue + AsRef<str>,
{
    fn render(
        &self,
        _: &DocumentNode,
        value: &T,
        _: &mut DocumentRenderContext<'_>,
        output: &mut DocumentWriter,
    ) {
        output.write_str(value.as_ref());
    }
}

pub(crate) struct PlainTextBlockDocumentRenderer;

impl<T: NodeValue> DocumentNodeRenderer<T> for PlainTextBlockDocumentRenderer {
    fn render(
        &self,
        node: &DocumentNode,
        _: &T,
        context: &mut DocumentRenderContext<'_>,
        output: &mut DocumentWriter,
    ) {
        context.cr(output);
        context.render_children(node.id(), output);
        context.cr(output);
    }
}

pub(crate) struct PlainTextBreakDocumentRenderer;

impl<T: NodeValue> DocumentNodeRenderer<T> for PlainTextBreakDocumentRenderer {
    fn render(
        &self,
        _: &DocumentNode,
        _: &T,
        context: &mut DocumentRenderContext<'_>,
        output: &mut DocumentWriter,
    ) {
        context.cr(output);
    }
}

/// Diagnostic tree renderer for the built-in document root.
///
/// It owns the complete structural traversal so custom descendant payloads do
/// not need a per-type `"debug"` registration.
pub(crate) struct DebugTreeDocumentRenderer;

impl DocumentNodeRenderer<Root> for DebugTreeDocumentRenderer {
    // in "# heading *em*" it like:
    // container type=markdown_it::document::root::Root id=NodeId(0:0) srcmap=0..14 attrs=[]
    //   container type=markdown_it::plugins::cmark::block::heading::ATXHeading id=NodeId(1:0) srcmap=0..14 attrs=[]
    //     leaf type=markdown_it::document::text::Text id=NodeId(2:0) srcmap=2..10 attrs=[]
    //       container type=markdown_it::plugins::cmark::inline::emphasis::Em id=NodeId(3:0) srcmap=10..14 attrs=[]
    //         leaf type=markdown_it::document::text::Text id=NodeId(11..13)
    fn render(
        &self,
        node: &DocumentNode,
        _: &Root,
        context: &mut DocumentRenderContext<'_>,
        output: &mut DocumentWriter,
    ) {
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
                "{kind} type={} id={:?} srcmap=",
                current.name(),
                current.id()
            );
            if let Some(srcmap) = current.srcmap() {
                let (start, end) = srcmap.get_byte_offsets();
                write!(output, "{start}..{end}");
            } else {
                output.write_char('-');
            }
            writeln!(output, " attrs={:?}", current.attrs());

            if matches!(event, StructuralEvent::Enter(_)) {
                depth += 1;
            }
        }
    }
}

pub(crate) struct HtmlBlockElementDocumentRenderer(pub(crate) &'static str);

impl<T: NodeValue> DocumentNodeRenderer<T> for HtmlBlockElementDocumentRenderer {
    fn render(
        &self,
        node: &DocumentNode,
        _: &T,
        context: &mut DocumentRenderContext<'_>,
        output: &mut DocumentWriter,
    ) {
        context.cr(output);
        write_html_open(output, self.0, node.attrs());
        context.render_children(node.id(), output);
        write_html_close(output, self.0);
        context.cr(output);
    }
}

pub(crate) fn write_html_attrs(output: &mut DocumentWriter, attrs: &[HtmlAttribute]) {
    const LINEAR_SCAN_LIMIT: usize = 8;

    // small list, using linear scan. avoid hashmap overhead.
    // O(n^2), but very fast with small data.
    if attrs.len() <= LINEAR_SCAN_LIMIT {
        for (index, (name, _)) in attrs.iter().enumerate() {
            // deduplication
            if attrs[..index].iter().any(|(previous, _)| previous == name) {
                continue;
            }
            write_html_attr_group(
                output,
                name,
                attrs
                    .iter()
                    .filter_map(|(candidate, value)| (candidate == name).then_some(value.as_str())),
            );
        }
        return;
    }

    let mut values = HashMap::<&str, Vec<&str>>::new();
    let mut order = Vec::with_capacity(attrs.len());
    for (name, value) in attrs {
        values.entry(name).or_default().push(value);
        order.push(name.as_str());
    }
    for name in order {
        let Some(parts) = values.remove(name) else {
            continue;
        };
        write_html_attr_group(output, name, parts.into_iter());
    }
}

fn write_html_attr_group<'a>(
    output: &mut DocumentWriter,
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

pub(crate) fn write_html_open(output: &mut DocumentWriter, tag: &str, attrs: &[HtmlAttribute]) {
    output.write_char('<');
    output.write_str(tag);
    write_html_attrs(output, attrs);
    output.write_char('>');
}

pub(crate) fn write_html_close(output: &mut DocumentWriter, tag: &str) {
    output.write_str("</");
    output.write_str(tag);
    output.write_char('>');
}

pub(crate) fn write_html_self_close(
    output: &mut DocumentWriter,
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

pub(crate) fn write_html_text(output: &mut DocumentWriter, value: &str) {
    output.write_escaped_html(value);
}

#[cfg(test)]
mod tests;
