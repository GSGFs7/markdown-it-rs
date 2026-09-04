//! Format-specific rendering for arena-backed documents.

use std::any::TypeId;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::{self, Write};
use std::hash::{BuildHasherDefault, Hasher};
use std::marker::PhantomData;

use crate::common::utils::escape_html;
use crate::parser::document::{Document, DocumentNode, InvalidNodeId, NodeId, NodeRef};
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
mod tests {
    use std::fmt::Write;

    use super::{
        DocumentNodeRenderer,
        DocumentRenderContext,
        DocumentRenderError,
        DocumentRendererRegistry,
        DocumentWriter,
    };
    use crate::parser::inline::Text;
    use crate::{Document, MarkdownIt, Node, NodeRef, NodeValue, RenderOptions};

    #[derive(Debug)]
    struct UnknownContainer;
    impl NodeValue for UnknownContainer {}

    #[derive(Debug)]
    struct UnknownLeaf(&'static str);
    impl NodeValue for UnknownLeaf {}

    struct UnknownLeafRenderer(&'static str);

    impl DocumentNodeRenderer<UnknownLeaf> for UnknownLeafRenderer {
        fn render(
            &self,
            _: NodeRef<'_>,
            value: &UnknownLeaf,
            _: &mut DocumentRenderContext<'_>,
            output: &mut DocumentWriter,
        ) -> Result<(), DocumentRenderError> {
            write!(output, "{}:{}", self.0, value.0)?;
            Ok(())
        }
    }

    struct DirectWriteAndCrRenderer;

    impl DocumentNodeRenderer<UnknownLeaf> for DirectWriteAndCrRenderer {
        fn render(
            &self,
            _: NodeRef<'_>,
            _: &UnknownLeaf,
            context: &mut DocumentRenderContext<'_>,
            output: &mut DocumentWriter,
        ) -> Result<(), DocumentRenderError> {
            output.write_str("first")?;
            context.cr(output)?;
            context.cr(output)?;
            output.write_str("second\n")?;
            context.cr(output)
        }
    }

    #[test]
    fn renders_minimal_html_directly_without_consuming_document() {
        let md = MarkdownIt::empty();
        let document = md.parse_document("hello <world>");

        assert_eq!(
            md.render_document(&document).unwrap(),
            "hello &lt;world&gt;\n"
        );
        assert_eq!(
            md.render_document_as(&document, "text").unwrap(),
            "hello <world>\n"
        );
        assert_eq!(
            md.render_document(&document).unwrap(),
            "hello &lt;world&gt;\n"
        );

        let nul = md.parse_document("\0");
        assert_eq!(md.render_document(&nul).unwrap(), "\u{FFFD}\n");
        assert_eq!(md.render_document_as(&nul, "text").unwrap(), "\u{FFFD}\n");
    }

    #[test]
    fn cr_observes_direct_renderer_writes_without_duplicate_line_endings() {
        let mut registry = DocumentRendererRegistry::new();
        registry.add::<UnknownLeaf, _>("html", DirectWriteAndCrRenderer);
        let document = Document::from_legacy("", Node::new(UnknownLeaf("unused")));

        assert_eq!(
            registry
                .render(&document, "html", &RenderOptions::default())
                .unwrap(),
            "first\nsecond\n"
        );
    }

    #[test]
    fn html_attrs_preserve_grouping_order_and_escaping_on_both_paths() {
        let small = vec![
            ("class".into(), "first".into()),
            ("id".into(), "one".into()),
            ("class".into(), "second".into()),
            ("style".into(), "color:<red>".into()),
            ("title".into(), "<&>".into()),
            ("style".into(), "display:block".into()),
            ("id".into(), "two".into()),
        ];
        let mut output = DocumentWriter::new();
        super::write_html_attrs(&mut output, &small).unwrap();
        assert_eq!(
            output.finish(),
            " class=\"first second\" id=\"one\" id=\"two\" style=\"color:&lt;red&gt;;display:block\" title=\"&lt;&amp;&gt;\""
        );

        let mut large = small;
        large.extend([("data-a".into(), "a".into()), ("data-b".into(), "b".into())]);
        let mut output = DocumentWriter::new();
        super::write_html_attrs(&mut output, &large).unwrap();
        assert_eq!(
            output.finish(),
            " class=\"first second\" id=\"one\" id=\"two\" style=\"color:&lt;red&gt;;display:block\" title=\"&lt;&amp;&gt;\" data-a=\"a\" data-b=\"b\""
        );
    }

    #[test]
    fn registered_paragraph_renderer_preserves_attributes() {
        let md = MarkdownIt::new();
        let mut root = md.parse("hello");
        root.children[0].attrs.extend([
            ("class".into(), "one".into()),
            ("class".into(), "two".into()),
        ]);
        let document = Document::from_legacy("hello", root);

        assert_eq!(
            md.render_document(&document).unwrap(),
            "<p class=\"one two\">hello</p>\n"
        );
    }

    #[test]
    fn commonmark_block_renderers_match_legacy_html() {
        let sources = [
            "# atx\n\nsetext\n------\n",
            "> quoted\n>\n> second\n",
            "> [label]: /destination\n",
            "1. first\n2. second\n\n7. seven\n",
            "- outer\n  - inner\n",
            "---\n",
            "    <indented> & code\n",
            "```rust extra\nfn main() { <tag> }\n```\n",
            "[label]: /destination \"title\"\n",
        ];

        for mut md in [
            MarkdownIt::new(),
            MarkdownIt::with_preset(crate::Preset::CommonMark),
        ] {
            md.render_options.lang_prefix = Some("lang-".into());
            for source in sources {
                let expected = md.parse(source).render();
                let document = md.parse_document(source);
                assert_eq!(
                    md.render_document(&document).unwrap(),
                    expected,
                    "direct renderer differs for {source:?} with options {:?}",
                    md.render_options
                );
            }
        }
    }

    #[test]
    fn block_renderers_preserve_document_transform_attributes() {
        let source = "# heading\n\n> quote\n\n3. item\n\n---\n\n    code\n\n```rs\nfenced\n```\n";

        let mut legacy = MarkdownIt::empty();
        crate::plugins::cmark::add(&mut legacy);
        crate::plugins::sourcepos::add(&mut legacy);
        let expected = legacy.render(source);

        let mut direct = MarkdownIt::empty();
        crate::plugins::cmark::add(&mut direct);
        crate::plugins::sourcepos::add_document(&mut direct);
        let mut document = direct.parse_document(source);
        direct.run_document_transforms(&mut document).unwrap();

        assert_eq!(direct.render_document(&document).unwrap(), expected);
    }

    #[test]
    fn commonmark_inline_renderers_match_legacy_html() {
        let sources = [
            "plain *em **strong** text* end",
            "`<code> & value`",
            "soft\nbreak and hard  \nbreak",
            "[label *em*](https://example.com/?a=1&b=2 \"a title\")",
            "![alt *em* <b>raw</b>](image.png \"image title\")",
            "<https://example.com/?a=1&b=2> <hello@example.com>",
        ];

        for xhtml_out in [false, true] {
            for breaks in [false, true] {
                let mut md = MarkdownIt::empty();
                crate::plugins::cmark::add(&mut md);
                crate::plugins::html::add(&mut md);
                md.render_options.xhtml_out = xhtml_out;
                md.render_options.breaks = breaks;

                for source in sources {
                    let expected = md.parse(source).render();
                    let document = md.parse_document(source);
                    assert_eq!(
                        md.render_document(&document).unwrap(),
                        expected,
                        "direct renderer differs for {source:?} with options {:?}",
                        md.render_options
                    );
                }
            }
        }
    }

    #[test]
    fn inline_renderers_preserve_document_transform_attributes() {
        let source = "**strong** [link](https://example.com) ![alt](image.png)  \nnext";

        let mut legacy = MarkdownIt::empty();
        crate::plugins::cmark::add(&mut legacy);
        crate::plugins::sourcepos::add(&mut legacy);
        let expected = legacy.render(source);

        let mut direct = MarkdownIt::empty();
        crate::plugins::cmark::add(&mut direct);
        crate::plugins::sourcepos::add_document(&mut direct);
        let mut document = direct.parse_document(source);
        direct.run_document_transforms(&mut document).unwrap();

        assert_eq!(direct.render_document(&document).unwrap(), expected);
    }

    #[test]
    fn unknown_container_transparently_renders_children() {
        let md = MarkdownIt::empty();
        let mut root = Node::new(UnknownContainer);
        root.children.push(Node::new(Text {
            content: "child".into(),
        }));
        let document = Document::from_legacy("", root);

        assert_eq!(md.render_document(&document).unwrap(), "child");
        assert_eq!(md.render_document_as(&document, "text").unwrap(), "child");
    }

    #[test]
    fn unknown_leaf_returns_structured_error() {
        let md = MarkdownIt::empty();
        let document = Document::from_legacy("", Node::new(UnknownLeaf("value")));

        assert!(matches!(
            md.render_document(&document),
            Err(DocumentRenderError::MissingRenderer {
                format,
                node_name,
                ..
            }) if format == "html" && node_name == std::any::type_name::<UnknownLeaf>()
        ));
    }

    #[test]
    fn custom_renderer_can_be_registered_and_overridden_per_format() {
        let document = Document::from_legacy("", Node::new(UnknownLeaf("value")));
        let mut registry = DocumentRendererRegistry::new();

        assert!(!registry.add::<UnknownLeaf, _>("plain", UnknownLeafRenderer("first")));
        assert!(registry.contains::<UnknownLeaf>("plain"));
        assert_eq!(
            registry
                .render(&document, "plain", &RenderOptions::default())
                .unwrap(),
            "first:value"
        );

        assert!(registry.add::<UnknownLeaf, _>("plain", UnknownLeafRenderer("second")));
        assert_eq!(
            registry
                .render(&document, "plain", &RenderOptions::default())
                .unwrap(),
            "second:value"
        );
        assert!(registry.remove::<UnknownLeaf>("plain"));
        assert!(!registry.contains::<UnknownLeaf>("plain"));
    }

    #[test]
    fn markdown_it_selects_and_isolates_renderer_formats() {
        let document = Document::from_legacy("", Node::new(UnknownLeaf("value")));
        let mut md = MarkdownIt::empty();
        md.add_document_renderer::<UnknownLeaf, _>("html", UnknownLeafRenderer("html"));
        md.add_document_renderer::<UnknownLeaf, _>("text", UnknownLeafRenderer("text"));

        assert_eq!(md.render_document(&document).unwrap(), "html:value");
        assert_eq!(
            md.render_document_as(&document, "text").unwrap(),
            "text:value"
        );

        md.add_document_renderer::<UnknownLeaf, _>("text", UnknownLeafRenderer("override"));
        assert_eq!(md.render_document(&document).unwrap(), "html:value");
        assert_eq!(
            md.render_document_as(&document, "text").unwrap(),
            "override:value"
        );

        assert!(matches!(
            md.render_document_as(&document, "missing"),
            Err(DocumentRenderError::MissingRenderer {
                format,
                node_name,
                ..
            }) if format == "missing" && node_name == std::any::type_name::<UnknownLeaf>()
        ));
    }
}
