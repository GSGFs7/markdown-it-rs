//! Format-specific rendering for arena-backed documents.

use std::any::TypeId;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::{self, Write};
use std::hash::{BuildHasherDefault, Hasher};
use std::marker::PhantomData;

use crate::common::utils::escape_html;
use crate::document::{Document, DocumentNode, InvalidNodeId, NodeId, NodeRef, StructuralEvent};
use crate::parser::core::Root;
use crate::parser::extset::RenderExtSet;
use crate::parser::node::{HtmlAttribute, NodeValue};
use crate::parser::render_options::RenderOptions;

// --- error ---

/// Error produced while rendering an arena-backed [`Document`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentRenderError {
    /// A renderer attempted to visit an invalid or stale node ID.
    InvalidNodeId(InvalidNodeId),
    /// A leaf payload has no renderer for the requested format.
    MissingRenderer {
        format: String,
        node: NodeId,
        node_name: &'static str,
    },
    /// The output writer rejected a write.
    Write,
}

impl fmt::Display for DocumentRenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidNodeId(source) => source.fmt(f),
            Self::MissingRenderer {
                format,
                node,
                node_name,
            } => write!(
                f,
                "no {format:?} renderer registered for leaf {node_name} at {node:?}"
            ),
            Self::Write => f.write_str("renderer output writer rejected a write"),
        }
    }
}

impl std::error::Error for DocumentRenderError {}

impl From<InvalidNodeId> for DocumentRenderError {
    fn from(value: InvalidNodeId) -> Self {
        Self::InvalidNodeId(value)
    }
}

impl From<fmt::Error> for DocumentRenderError {
    fn from(_: fmt::Error) -> Self {
        Self::Write
    }
}

// --- protocol ---

/// Renderer for one payload type in one output format.
///
/// ```
/// use std::fmt::Write;
///
/// use markdown_it::{
///     Document, DocumentNodeRenderer, DocumentRenderContext, DocumentRenderError,
///     DocumentWriter, MarkdownIt, Node, NodeRef, NodeValue,
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
///     ) -> Result<(), DocumentRenderError> {
///         output.write_str(badge.0)?;
///         Ok(())
///     }
/// }
///
/// let mut md = MarkdownIt::empty();
/// md.add_document_renderer::<Badge, _>("html", BadgeRenderer);
/// let document = Document::from_legacy("", Node::new(Badge("new")));
/// assert_eq!(md.render_document(&document)?, "new");
/// # Ok::<(), DocumentRenderError>(())
/// ```
pub trait DocumentNodeRenderer<T: NodeValue>: Send + Sync + 'static {
    fn render(
        &self,
        node: NodeRef<'_>,
        value: &T,
        context: &mut DocumentRenderContext<'_>,
        output: &mut DocumentWriter,
    ) -> Result<(), DocumentRenderError>;
}

trait ErasedDocumentNodeRenderer: Send + Sync {
    fn render(
        &self,
        node: NodeRef<'_>,
        context: &mut DocumentRenderContext<'_>,
        output: &mut DocumentWriter,
    ) -> Result<(), DocumentRenderError>;
}

// --- machinery ---

#[derive(Default)]
struct TypeIdHasher(u64);

impl Hasher for TypeIdHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        // Fallback for a future TypeId Hash implementation that does not use
        // one of the integer-specific Hasher methods.
        let mut hash = self.0 ^ 0xcbf2_9ce4_8422_2325;
        for &byte in bytes {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        self.0 = hash;
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = self.0.rotate_left(5) ^ value;
    }

    fn write_u128(&mut self, value: u128) {
        self.write_u64(value as u64);
        self.write_u64((value >> 64) as u64);
    }
}

type FormatRenderers =
    HashMap<TypeId, Box<dyn ErasedDocumentNodeRenderer>, BuildHasherDefault<TypeIdHasher>>;

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
    ) -> Result<(), DocumentRenderError> {
        let value = node
            .cast::<T>()
            .expect("document renderer registry type and payload type must agree");
        self.renderer.render(node, value, context, output)
    }
}

// --- registry ---

/// Registry keyed by output format and concrete node payload type.
///
/// Adding the same pair again replaces the previous renderer and returns
/// `true`. This gives applications an explicit override mechanism.
#[derive(Default)]
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
                Box::new(TypedDocumentNodeRenderer::<T, R> {
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

    /// Render a document without rebuilding the legacy tree.
    pub fn render(
        &self,
        document: &Document,
        format: &str,
        options: &RenderOptions,
    ) -> Result<String, DocumentRenderError> {
        let mut ext = RenderExtSet::new();
        let mut output = DocumentWriter::new();
        let shared = RenderShared {
            document,
            renderers: self.formats.get(format),
            format,
            options,
            scratch_nodes: RefCell::new(Vec::new()),
        };
        render_node(&shared, &mut ext, document.root(), &mut output)?;
        Ok(output.finish())
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
/// It implements [`fmt::Write`], so renderers can continue to use `write!`,
/// `write_str`, and `write_char`. Keeping the buffer concrete lets
/// [`DocumentRenderContext::cr`] inspect its actual last byte without tracking
/// shared line state after every write.
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
}

impl Write for DocumentWriter {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.output.push_str(value);
        Ok(())
    }
}

/// Recursive services shared by node renderers for one render operation.
pub struct DocumentRenderContext<'a> {
    shared: &'a RenderShared<'a>,
    ext: &'a mut RenderExtSet,
}

impl DocumentRenderContext<'_> {
    pub fn document(&self) -> &Document {
        self.shared.document
    }

    pub fn format(&self) -> &str {
        self.shared.format
    }

    pub fn options(&self) -> &RenderOptions {
        self.shared.options
    }

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
    pub fn cr(&mut self, output: &mut DocumentWriter) -> Result<(), DocumentRenderError> {
        output.cr();
        Ok(())
    }

    pub fn render_node(
        &mut self,
        node: NodeId,
        output: &mut DocumentWriter,
    ) -> Result<(), DocumentRenderError> {
        render_node(self.shared, self.ext, node, output)
    }

    pub fn render_children(
        &mut self,
        node: NodeId,
        output: &mut DocumentWriter,
    ) -> Result<(), DocumentRenderError> {
        for &child in self.shared.document.children(node)? {
            stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
                render_node(self.shared, self.ext, child, output)
            })?;
        }
        Ok(())
    }
}

// recursive rendering nodes
fn render_node(
    shared: &RenderShared<'_>,
    ext: &mut RenderExtSet,
    id: NodeId,
    output: &mut DocumentWriter,
) -> Result<(), DocumentRenderError> {
    let node = shared.document.node(id)?;
    let Some(renderer) = shared
        .renderers
        .and_then(|renderers| renderers.get(&node.type_id()))
    else {
        if node.children().is_empty() {
            // leaf but no renderer
            return Err(DocumentRenderError::MissingRenderer {
                format: shared.format.to_owned(),
                node: id,
                node_name: node.name(),
            });
        }
        // transparent downward traversal
        for &child in node.children() {
            stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
                render_node(shared, ext, child, output)
            })?;
        }
        return Ok(());
    };

    // render the shell (<h1>,<em>,...)
    let mut context = DocumentRenderContext { shared, ext };
    renderer.render(node, &mut context, output)
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
    ) -> Result<(), DocumentRenderError> {
        context.render_children(node.id(), output)
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
    ) -> Result<(), DocumentRenderError> {
        Ok(())
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
    ) -> Result<(), DocumentRenderError> {
        output.write_str(&escape_html(value.as_ref()))?;
        Ok(())
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
    ) -> Result<(), DocumentRenderError> {
        output.write_str(value.as_ref())?;
        Ok(())
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
    ) -> Result<(), DocumentRenderError> {
        context.cr(output)?;
        context.render_children(node.id(), output)?;
        context.cr(output)
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
    ) -> Result<(), DocumentRenderError> {
        context.cr(output)
    }
}

/// Diagnostic tree renderer for the built-in document root.
///
/// It owns the complete structural traversal so custom descendant payloads do
/// not need a per-type `"debug"` registration.
pub(crate) struct DebugTreeDocumentRenderer;

impl DocumentNodeRenderer<Root> for DebugTreeDocumentRenderer {
    // in "# heading *em*" it like:
    // container type=markdown_it::parser::core::root::Root id=NodeId(0:0) srcmap=0..14 attrs=[]
    //   container type=markdown_it::plugins::cmark::block::heading::ATXHeading id=NodeId(1:0) srcmap=0..14 attrs=[]
    //     leaf type=markdown_it::parser::inline::builtin::skip_text::Text id=NodeId(2:0) srcmap=2..10 attrs=[]
    //       container type=markdown_it::plugins::cmark::inline::emphasis::Em id=NodeId(3:0) srcmap=10..14 attrs=[]
    //         leaf type=markdown_it::parser::inline::builtin::skip_text::Text id=NodeId(11..13)
    fn render(
        &self,
        node: &DocumentNode,
        _: &Root,
        context: &mut DocumentRenderContext<'_>,
        output: &mut DocumentWriter,
    ) -> Result<(), DocumentRenderError> {
        let mut depth = 0;
        for event in context.document().events(node.id())? {
            let (kind, current) = match event {
                StructuralEvent::Enter(current) => ("container", current),
                StructuralEvent::Leaf(current) => ("leaf", current),
                StructuralEvent::Exit(_) => {
                    depth -= 1;
                    continue;
                }
            };

            for _ in 0..depth {
                output.write_str("  ")?;
            }
            write!(
                output,
                "{kind} type={} id={:?} srcmap=",
                current.name(),
                current.id()
            )?;
            if let Some(srcmap) = current.srcmap() {
                let (start, end) = srcmap.get_byte_offsets();
                write!(output, "{start}..{end}")?;
            } else {
                output.write_char('-')?;
            }
            writeln!(output, " attrs={:?}", current.attrs())?;

            if matches!(event, StructuralEvent::Enter(_)) {
                depth += 1;
            }
        }
        Ok(())
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
    ) -> Result<(), DocumentRenderError> {
        context.cr(output)?;
        write!(output, "<{}", self.0)?;
        write_html_attrs(output, node.attrs())?;
        output.write_char('>')?;
        context.render_children(node.id(), output)?;
        write!(output, "</{}>", self.0)?;
        context.cr(output)
    }
}

pub(crate) fn write_html_attrs(
    output: &mut DocumentWriter,
    attrs: &[HtmlAttribute],
) -> Result<(), DocumentRenderError> {
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
            )?;
        }
        return Ok(());
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
        write_html_attr_group(output, name, parts.into_iter())?;
    }
    Ok(())
}

fn write_html_attr_group<'a>(
    output: &mut DocumentWriter,
    name: &str,
    values: impl Iterator<Item = &'a str>,
) -> Result<(), DocumentRenderError> {
    if name == "class" || name == "style" {
        write!(output, " {}=\"", escape_html(name))?;
        let separator = if name == "class" { ' ' } else { ';' };
        for (index, value) in values.enumerate() {
            if index != 0 {
                output.write_char(separator)?;
            }
            output.write_str(&escape_html(value))?;
        }
        output.write_char('"')?;
    } else {
        for value in values {
            write!(output, " {}=\"{}\"", escape_html(name), escape_html(value))?;
        }
    }
    Ok(())
}

pub(crate) fn write_html_open(
    output: &mut DocumentWriter,
    tag: &str,
    attrs: &[HtmlAttribute],
) -> Result<(), DocumentRenderError> {
    write!(output, "<{tag}")?;
    write_html_attrs(output, attrs)?;
    output.write_char('>')?;
    Ok(())
}

pub(crate) fn write_html_close(
    output: &mut DocumentWriter,
    tag: &str,
) -> Result<(), DocumentRenderError> {
    write!(output, "</{tag}>")?;
    Ok(())
}

pub(crate) fn write_html_self_close(
    output: &mut DocumentWriter,
    tag: &str,
    attrs: &[HtmlAttribute],
    xhtml: bool,
) -> Result<(), DocumentRenderError> {
    write!(output, "<{tag}")?;
    write_html_attrs(output, attrs)?;
    if xhtml {
        output.write_str(" /")?;
    }
    output.write_char('>')?;
    Ok(())
}

pub(crate) fn write_html_text(
    output: &mut DocumentWriter,
    value: &str,
) -> Result<(), DocumentRenderError> {
    output.write_str(&escape_html(value))?;
    Ok(())
}

#[cfg(test)]
mod tests;
