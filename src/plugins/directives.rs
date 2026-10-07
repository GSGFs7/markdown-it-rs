//! Directive syntax.
//!
//! Supports text directives (`:name{key=value}`),
//! leaf directives (`::name{key=value}`),
//! and container directives:
//!
//! ```markdown
//! :::name{key=value}
//! content
//! :::
//! ```
//!
//! By default, text directives render to `<span class="directive name">`,
//! while leaf and container directives render to `<div class="directive name">`.
//! Parsed attributes are appended to the rendered HTML element.
//!
//! Custom renderers registered with [`add_render`] are matched by directive
//! kind and name and receive the parsed attributes and node.
//!
//! # Security
//!
//! This plugin does not sanitize directive attributes. Attribute names and
//! values are copied to the rendered HTML element after HTML escaping. Event
//! handlers, CSS, URLs, and other active attributes are not validated.
//!
//! Enable this plugin only for trusted input, or sanitize the final HTML with
//! a policy appropriate for your application. Custom renderers have the same
//! trust boundary: use [`DirectiveRenderer::text`] for user-provided text, validate
//! URL-bearing attributes separately, and reserve [`DirectiveRenderer::text_raw`] for
//! trusted HTML.
//!
//! ```rust
//! use markdown_it::plugins::directives::{self, DirectiveKind, DirectiveNode, DirectiveRenderer};
//! use markdown_it::MarkdownIt;
//!
//! fn render_badge(
//!     kind: DirectiveKind,
//!     name: &str,
//!     attrs: &[(String, String)],
//!     _node: DirectiveNode<'_>,
//!     fmt: &mut DirectiveRenderer<'_, '_>,
//! ) {
//!     assert_eq!(kind, DirectiveKind::Text);
//!     assert_eq!(name, "badge");
//!
//!     let label = attrs
//!         .iter()
//!         .find_map(|(key, value)| (key == "label").then_some(value.as_str()))
//!         .unwrap_or("");
//!
//!     fmt.open("mark", &[("class".into(), "badge".to_owned())]);
//!     fmt.text(label);
//!     fmt.close("mark");
//! }
//!
//! let mut md = MarkdownIt::empty();
//! markdown_it::plugins::cmark::add(&mut md);
//! directives::add(&mut md);
//! directives::add_render(&mut md, DirectiveKind::Text, "badge", render_badge);
//!
//! let html = md.parse("status: :badge{label=\"Beta\"}").render();
//! assert_eq!(
//!     html.trim(),
//!     r#"<p>status: <mark class="badge">Beta</mark></p>"#,
//! );
//! ```

use std::collections::HashMap;
use std::fmt::Debug;

use crate::common::sourcemap::SourcePos;
use crate::document::{NodeDraft, NodeRef};
use crate::parser::block::{BlockRule, BlockState, DocumentBlockRule};
use crate::parser::document_parser::{DocumentBlockState, DocumentInlineState};
use crate::parser::extset::{NodeExtSet, RenderExtSet};
use crate::parser::inline::probe::{InlineProbeContext, InlineProbeKind, InlineProbeResult};
use crate::parser::inline::{InlineRule, InlineState, LegacyInlineRule};
use crate::parser::node::HtmlAttribute;
use crate::render::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    TransparentDocumentRenderer,
    write_html_close,
    write_html_open,
    write_html_self_close,
    write_html_text,
};
use crate::{Document, DocumentWriter, MarkdownIt, Node, NodeValue, RenderOptions, Renderer};

// --- render ---

/// Parsed directive attributes.
pub type Attrs = Vec<(String, String)>;

#[derive(Debug, Clone)]
/// Inline directive parsed from `:name{key=value}`.
pub struct TextDirective {
    /// Directive name after the marker.
    pub name: String,
    /// Parsed directive attributes.
    pub attrs: Attrs,
}

impl NodeValue for TextDirective {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        render_directive(
            DirectiveKind::Text,
            &self.name,
            &self.attrs,
            DirectiveNode(DirectiveNodeInner::Legacy(node)),
            &mut DirectiveRenderer(DirectiveRendererInner::Legacy(fmt)),
        );
    }
}

#[derive(Debug, Clone)]
/// Block directive parsed from `::name{key=value}`.
pub struct LeafDirective {
    /// Directive name after the marker.
    pub name: String,
    /// Parsed directive attributes.
    pub attrs: Attrs,
}

impl NodeValue for LeafDirective {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        render_directive(
            DirectiveKind::Leaf,
            &self.name,
            &self.attrs,
            DirectiveNode(DirectiveNodeInner::Legacy(node)),
            &mut DirectiveRenderer(DirectiveRendererInner::Legacy(fmt)),
        );
    }
}

#[derive(Debug, Clone)]
/// Block directive parsed from a fenced `:::name` container.
pub struct ContainerDirective {
    /// Directive name after the opening marker.
    pub name: String,
    /// Parsed directive attributes.
    pub attrs: Attrs,
}

impl NodeValue for ContainerDirective {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        render_directive(
            DirectiveKind::Container,
            &self.name,
            &self.attrs,
            DirectiveNode(DirectiveNodeInner::Legacy(node)),
            &mut DirectiveRenderer(DirectiveRendererInner::Legacy(fmt)),
        );
    }
}

// --- scanner ---

impl LegacyInlineRule for TextDirective {
    const MARKER: char = ':';
    const NAMES: &'static [&'static str] = &["text_directive"];

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let src = &state.src[state.pos..state.pos_max];
        let preceded_by_colon = state.pos > 0 && state.src[..state.pos].ends_with(':');
        let (name, attrs, len) = scan_text_directive(src, preceded_by_colon)?;

        let mut node = Node::new(TextDirective {
            name: name.clone(),
            attrs,
        });
        attach_render(&mut node.ext, state.md, DirectiveKind::Text, &name);

        Some((node, len))
    }
}

impl InlineRule for TextDirective {
    const MARKER: char = ':';
    const NAMES: &'static [&'static str] = &["text_directive"];

    fn probe(context: &mut InlineProbeContext<'_>) -> InlineProbeResult {
        let (source, pos, _) = context.source_window();
        let preceded_by_colon = pos > 0 && source[..pos].ends_with(':');
        match scan_text_directive(context.remaining(), preceded_by_colon) {
            Some((_, _, len)) => InlineProbeResult::Match {
                len,
                kind: InlineProbeKind::Token,
            },
            None => InlineProbeResult::NoMatch,
        }
    }

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
        let preceded_by_colon = state.pos > 0 && state.src[..state.pos].ends_with(':');
        let (name, attrs, len) = scan_text_directive(state.remaining(), preceded_by_colon)?;

        let mut node = NodeDraft::new(TextDirective {
            name: name.clone(),
            attrs,
        });
        attach_render(
            node.ext_mut(),
            state.markdown_it(),
            DirectiveKind::Text,
            &name,
        );

        Some((Some(node), len))
    }
}

pub struct LeafDirectiveScanner;

impl BlockRule for LeafDirectiveScanner {
    const MARKERS: &'static [char] = &[':'];
    const NAMES: &'static [&'static str] = &["leaf_directive"];

    fn run(state: &mut BlockState) -> Option<(Node, usize)> {
        // it should be a codeblocks
        if state.line_indent(state.line) >= state.md.max_indent {
            return None;
        }

        let (name, attrs) = scan_leaf_directive(state.get_line(state.line))?;

        let mut node = Node::new(LeafDirective {
            name: name.clone(),
            attrs,
        });
        attach_render(&mut node.ext, state.md, DirectiveKind::Leaf, &name);

        Some((node, 1))
    }
}

impl DocumentBlockRule for LeafDirectiveScanner {
    fn run(state: &mut DocumentBlockState<'_>) -> Option<(NodeDraft, usize)> {
        // it should be a codeblocks
        if state.line_indent(state.line) >= state.md.max_indent {
            return None;
        }

        let (name, attrs) = scan_leaf_directive(state.get_line(state.line))?;

        let mut node = NodeDraft::new(LeafDirective {
            name: name.clone(),
            attrs,
        });
        attach_render(node.ext_mut(), state.md, DirectiveKind::Leaf, &name);

        Some((node, 1))
    }
}

pub struct ContainerDirectiveScanner;

impl ContainerDirectiveScanner {
    fn scan_line(line: &str) -> Option<(usize, String, Attrs)> {
        let line = line.trim_end();
        // lenght of ':'
        let marker_len = line.bytes().take_while(|b| *b == b':').count();
        // must greater than or equal to 3
        if marker_len < 3 {
            return None;
        }

        // skip marker
        let mut pos = marker_len;
        pos += line[pos..].len() - line[pos..].trim_start().len();
        // skip name
        let (name, name_len) = parse_name(&line[pos..])?;
        pos += name_len;
        pos += line[pos..].len() - line[pos..].trim_start().len();
        // skip attributes
        let (attrs, attrs_len) = parse_attrs(&line[pos..])?;
        pos += attrs_len;
        // no other chars
        if !line[pos..].trim().is_empty() {
            return None;
        }

        Some((marker_len, name, attrs))
    }

    fn scan(state: &mut BlockState) -> Option<(usize, String, Attrs)> {
        // it should be code blocks
        if state.line_indent(state.line) >= state.md.max_indent {
            return None;
        }

        Self::scan_line(state.get_line(state.line))
    }

    fn scan_document(state: &mut DocumentBlockState<'_>) -> Option<(usize, String, Attrs)> {
        // it should be code blocks
        if state.line_indent(state.line) >= state.md.max_indent {
            return None;
        }

        Self::scan_line(state.get_line(state.line))
    }

    // Find the matching fence while respecting nested directives and outdents.
    fn scan_end<'a>(
        start_line: usize,
        line_max: usize,
        marker_len: usize,
        max_indent: i32,
        get_line: impl Fn(usize) -> (&'a str, i32, bool),
    ) -> (usize, bool) {
        let mut marker_stack = vec![marker_len];
        for next_line in start_line + 1..line_max {
            let (line, indent, empty) = get_line(next_line);
            if !empty && indent < 0 {
                return (next_line, false);
            }
            if indent >= max_indent {
                continue;
            }
            if Self::is_close(line, *marker_stack.last().unwrap()) {
                marker_stack.pop();
                if marker_stack.is_empty() {
                    return (next_line, true);
                }
            } else if let Some((nested_marker_len, _, _)) = Self::scan_line(line) {
                marker_stack.push(nested_marker_len);
            }
        }
        (line_max, false)
    }

    fn is_close(line: &str, marker_len: usize) -> bool {
        let line = line.trim_end();
        let len = line.bytes().take_while(|b| *b == b':').count();
        // marker lenght avail & no other chars
        len >= marker_len && line[len..].trim().is_empty()
    }
}

impl BlockRule for ContainerDirectiveScanner {
    const MARKERS: &'static [char] = &[':'];
    const NAMES: &'static [&'static str] = &["container_directive"];

    fn check(state: &mut BlockState) -> Option<()> {
        Self::scan(state).map(|_| ())
    }

    fn run(state: &mut BlockState) -> Option<(Node, usize)> {
        let (marker_len, name, attrs) = Self::scan(state)?;

        let start_line = state.line;
        let (next_line, have_end_marker) = Self::scan_end(
            start_line,
            state.line_max,
            marker_len,
            state.md.max_indent,
            |line| {
                (
                    state.get_line(line),
                    state.line_indent(line),
                    state.is_empty(line),
                )
            },
        );

        // new node
        let mut directive_node = Node::new(ContainerDirective {
            name: name.clone(),
            attrs,
        });
        attach_render(
            &mut directive_node.ext,
            state.md,
            DirectiveKind::Container,
            &name,
        );

        // replace state
        let old_node = std::mem::replace(&mut state.node, directive_node);
        let old_line_max = state.line_max;

        // limit render behavior
        state.line = start_line + 1;
        state.line_max = next_line;

        // recursion tokenize
        state.md.block.tokenize_nested(state);

        // recover state
        state.line = start_line;
        state.line_max = old_line_max;

        let node = std::mem::replace(&mut state.node, old_node);
        Some((
            node,
            next_line - start_line + if have_end_marker { 1 } else { 0 },
        ))
    }
}

impl DocumentBlockRule for ContainerDirectiveScanner {
    fn check(state: &mut DocumentBlockState<'_>) -> Option<()> {
        Self::scan_document(state).map(|_| ())
    }

    fn run(state: &mut DocumentBlockState<'_>) -> Option<(NodeDraft, usize)> {
        let (marker_len, name, attrs) = Self::scan_document(state)?;

        let start_line = state.line;
        let (next_line, have_end_marker) = Self::scan_end(
            start_line,
            state.line_max,
            marker_len,
            state.md.max_indent,
            |line| {
                (
                    state.get_line(line),
                    state.line_indent(line),
                    state.is_empty(line),
                )
            },
        );

        // new node
        let mut directive_node = NodeDraft::new(ContainerDirective {
            name: name.clone(),
            attrs,
        });
        attach_render(
            directive_node.ext_mut(),
            state.md,
            DirectiveKind::Container,
            &name,
        );

        // replace state
        let old_node = std::mem::replace(&mut state.node, directive_node);
        let old_line_max = state.line_max;

        // limit render behavior
        state.line = start_line + 1;
        state.line_max = next_line;

        // recursion tokenize
        state.tokenize_nested();

        // recover state
        state.line = start_line;
        state.line_max = old_line_max;

        let node = std::mem::replace(&mut state.node, old_node);
        Some((
            node,
            next_line - start_line + if have_end_marker { 1 } else { 0 },
        ))
    }
}

// --- custom ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Directive variant used when registering and invoking custom renderers.
pub enum DirectiveKind {
    /// Inline text directive, e.g. `:name{key=value}`.
    Text,
    /// Leaf block directive, e.g. `::name{key=value}`.
    Leaf,
    /// Container block directive, e.g. `:::name{key=value} ... :::`.
    Container,
}

/// Custom renderer callback for a directive.
///
/// Receives the directive kind, name, parsed attributes, node, and renderer.
/// Use [`DirectiveRenderer::text`] for user-provided text and
/// [`DirectiveRenderer::text_raw`] for trusted HTML.
///
/// Migrating from the old signature: use `DirectiveNode<'_>` and
/// `DirectiveRenderer<'_, '_>`, and render children with
/// `fmt.contents(node.children())`.
pub type DirectiveRenderFn =
    fn(DirectiveKind, &str, &[(String, String)], DirectiveNode<'_>, &mut DirectiveRenderer<'_, '_>);

/// Read-only view of a directive node, usable from both parsing pipelines.
///
/// Render children with `fmt.contents(node.children())`.
#[derive(Clone, Copy, Debug)]
pub struct DirectiveNode<'a>(DirectiveNodeInner<'a>);

#[derive(Clone, Copy, Debug)]
enum DirectiveNodeInner<'a> {
    Legacy(&'a Node),
    Document(&'a Document, NodeRef<'a>),
}

impl<'a> DirectiveNode<'a> {
    pub fn name(self) -> &'static str {
        match self.0 {
            DirectiveNodeInner::Legacy(node) => node.name(),
            DirectiveNodeInner::Document(_, node) => node.name(),
        }
    }

    pub fn is<T: NodeValue>(self) -> bool {
        self.cast::<T>().is_some()
    }

    pub fn cast<T: NodeValue>(self) -> Option<&'a T> {
        match self.0 {
            DirectiveNodeInner::Legacy(node) => node.cast::<T>(),
            DirectiveNodeInner::Document(_, node) => node.cast::<T>(),
        }
    }

    pub fn attrs(self) -> &'a [HtmlAttribute] {
        match self.0 {
            DirectiveNodeInner::Legacy(node) => &node.attrs,
            DirectiveNodeInner::Document(_, node) => node.attrs(),
        }
    }

    pub fn srcmap(self) -> Option<SourcePos> {
        match self.0 {
            DirectiveNodeInner::Legacy(node) => node.srcmap,
            DirectiveNodeInner::Document(_, node) => node.srcmap(),
        }
    }

    pub fn ext(self) -> &'a NodeExtSet {
        match self.0 {
            DirectiveNodeInner::Legacy(node) => &node.ext,
            DirectiveNodeInner::Document(_, node) => node.ext(),
        }
    }

    pub fn children(self) -> impl ExactSizeIterator<Item = Self> + 'a {
        let len = match self.0 {
            DirectiveNodeInner::Legacy(node) => node.children.len(),
            DirectiveNodeInner::Document(_, node) => node.children().len(),
        };
        (0..len).map(move |index| match self.0 {
            DirectiveNodeInner::Legacy(node) => {
                Self(DirectiveNodeInner::Legacy(&node.children[index]))
            }
            DirectiveNodeInner::Document(document, node) => Self(DirectiveNodeInner::Document(
                document,
                document.node(node.children()[index]),
            )),
        })
    }
}

/// HTML rendering services for directive callbacks in either pipeline.
pub struct DirectiveRenderer<'r, 'd>(DirectiveRendererInner<'r, 'd>);

enum DirectiveRendererInner<'r, 'd> {
    Legacy(&'r mut dyn Renderer),
    Document(&'r mut DocumentRenderContext<'d>, &'r mut DocumentWriter),
}

impl DirectiveRenderer<'_, '_> {
    pub fn options(&self) -> Option<&RenderOptions> {
        match &self.0 {
            DirectiveRendererInner::Legacy(fmt) => fmt.options(),
            DirectiveRendererInner::Document(context, _) => Some(context.options()),
        }
    }

    pub fn is_xhtml(&self) -> bool {
        self.options().is_some_and(|options| options.xhtml_out)
    }

    pub fn open(&mut self, tag: &str, attrs: &[HtmlAttribute]) {
        match &mut self.0 {
            DirectiveRendererInner::Legacy(fmt) => fmt.open(tag, attrs),
            DirectiveRendererInner::Document(_, output) => write_html_open(output, tag, attrs),
        }
    }

    pub fn close(&mut self, tag: &str) {
        match &mut self.0 {
            DirectiveRendererInner::Legacy(fmt) => fmt.close(tag),
            DirectiveRendererInner::Document(_, output) => write_html_close(output, tag),
        }
    }

    pub fn self_close(&mut self, tag: &str, attrs: &[HtmlAttribute]) {
        match &mut self.0 {
            DirectiveRendererInner::Legacy(fmt) => fmt.self_close(tag, attrs),
            DirectiveRendererInner::Document(context, output) => {
                write_html_self_close(output, tag, attrs, context.options().xhtml_out);
            }
        }
    }

    pub fn contents<'a>(&mut self, nodes: impl IntoIterator<Item = DirectiveNode<'a>>) {
        for node in nodes {
            stacker::maybe_grow(64 * 1024, 1024 * 1024, || match (&mut self.0, node.0) {
                (DirectiveRendererInner::Legacy(fmt), DirectiveNodeInner::Legacy(node)) => {
                    fmt.contents(std::slice::from_ref(node));
                }
                (
                    DirectiveRendererInner::Document(context, output),
                    DirectiveNodeInner::Document(document, node),
                ) => {
                    assert!(
                        std::ptr::eq(context.document(), document),
                        "directive node belongs to another document"
                    );
                    context.render_node(node.id(), output);
                }
                _ => panic!("directive node belongs to another rendering pipeline"),
            });
        }
    }

    pub fn cr(&mut self) {
        match &mut self.0 {
            DirectiveRendererInner::Legacy(fmt) => fmt.cr(),
            DirectiveRendererInner::Document(context, output) => context.cr(output),
        }
    }

    pub fn softbreak(&mut self) {
        if self.options().is_some_and(|options| options.breaks) {
            self.self_close("br", &[]);
        }
        self.cr();
    }

    pub fn text(&mut self, text: &str) {
        match &mut self.0 {
            DirectiveRendererInner::Legacy(fmt) => fmt.text(text),
            DirectiveRendererInner::Document(_, output) => write_html_text(output, text),
        }
    }

    pub fn text_raw(&mut self, text: &str) {
        match &mut self.0 {
            DirectiveRendererInner::Legacy(fmt) => fmt.text_raw(text),
            DirectiveRendererInner::Document(_, output) => output.write_str(text),
        }
    }

    pub fn ext(&mut self) -> &mut RenderExtSet {
        match &mut self.0 {
            DirectiveRendererInner::Legacy(fmt) => fmt.ext(),
            DirectiveRendererInner::Document(context, _) => context.ext(),
        }
    }
}

#[derive(Default)]
struct DirectiveRenderers {
    map: HashMap<(DirectiveKind, String), DirectiveRenderFn>,
}

impl Debug for DirectiveRenderers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirectiveRenderers")
            .field("len", &self.map.len())
            .finish()
    }
}

#[derive(Clone, Copy)]
struct DirectiveRendererExt(DirectiveRenderFn);

impl Debug for DirectiveRendererExt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirectiveRendererExt").finish()
    }
}

// --- helper method ---

fn attach_render(ext: &mut NodeExtSet, md: &MarkdownIt, kind: DirectiveKind, name: &str) {
    if let Some(renderers) = md.ext.get::<DirectiveRenderers>()
        && let Some(render) = renderers.map.get(&(kind, name.to_owned()))
    {
        ext.insert(DirectiveRendererExt(*render));
    }
}

// scan a text directive from the current inline position
fn scan_text_directive(src: &str, preceded_by_colon: bool) -> Option<(String, Attrs, usize)> {
    // avoid affect other directive
    if !src.starts_with(':') || src.starts_with("::") {
        return None;
    }
    if preceded_by_colon {
        return None;
    }

    let mut pos = 1;
    let (name, name_len) = parse_name(&src[pos..])?;
    pos += name_len;
    pos += src[pos..].len() - src[pos..].trim_start().len();
    let (attrs, attrs_len) = parse_attrs(&src[pos..])?;
    pos += attrs_len;

    Some((name, attrs, pos))
}

// scan a leaf directive from one block line
fn scan_leaf_directive(line: &str) -> Option<(String, Attrs)> {
    let line = line.trim_end();
    if !line.starts_with("::") || line.starts_with(":::") {
        return None;
    }

    let mut pos = 2;
    pos += line[pos..].len() - line[pos..].trim_start().len();
    let (name, name_len) = parse_name(&line[pos..])?;
    pos += name_len;
    pos += line[pos..].len() - line[pos..].trim_start().len();
    let (attrs, attrs_len) = parse_attrs(&line[pos..])?;
    pos += attrs_len;
    if !line[pos..].trim().is_empty() {
        return None;
    }

    Some((name, attrs))
}

// parse directive name
fn parse_name(src: &str) -> Option<(String, usize)> {
    let mut end = 0;
    for (idx, ch) in src.char_indices() {
        let is_valid = if idx == 0 {
            ch.is_ascii_alphabetic()
        } else {
            ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'
        };

        if !is_valid {
            break;
        }
        end = idx + ch.len_utf8();
    }

    if end == 0 {
        None
    } else {
        Some((src[..end].to_owned(), end))
    }
}

// parse directive attribute
fn parse_attrs(src: &str) -> Option<(Attrs, usize)> {
    // if not start with '{' return
    if !src.starts_with('{') {
        return Some((Vec::new(), 0));
    }

    let mut attrs = Vec::new();
    // src iterator
    let mut chars = src.char_indices().skip(1);
    let mut end_pos = None;

    // get next char
    while let Some((idx, ch)) = chars.next() {
        // if find the end
        if ch == '}' {
            end_pos = Some(idx + 1);
            break;
        }
        // skip space
        if ch.is_whitespace() {
            continue;
        }

        // shorthand support: #id, .class
        if ch == '#' || ch == '.' {
            let key = if ch == '#' { "id" } else { "class" };
            let mut value = String::new();
            // view the next but do not consume it
            while let Some((_, c)) = chars.clone().next() {
                // support .class1.class2#id
                if c.is_whitespace() || c == '}' || c == '.' || c == '#' {
                    // if it's a bad shorthand
                    break;
                }

                value.push(c);
                chars.next();
            }
            attrs.push((key.to_owned(), value));
            continue;
        }

        // regular key=value
        let mut has_equals = false;

        // process key
        let mut key = String::new();
        key.push(ch);
        while let Some((_, c)) = chars.clone().next() {
            // find the end of key, key="xxx"
            if c == '=' {
                has_equals = true;
                chars.next();
                break;
            }
            // can't be split with space, ke y="xxx"
            if c.is_whitespace() || c == '}' {
                break;
            }
            key.push(c);
            chars.next();
        }

        // process value
        let mut value = String::new();
        if has_equals {
            if let Some((_, c)) = chars.next() {
                // skip quote, key="value"
                if c == '"' || c == '\'' {
                    let quote = c;
                    while let Some((_, c)) = chars.next() {
                        // quote close
                        if c == quote {
                            break;
                        }
                        // skip escape
                        if c == '\\' {
                            if let Some((_, next_c)) = chars.next() {
                                value.push(next_c);
                            }
                        } else {
                            value.push(c);
                        }
                    }
                } else {
                    // without quote, key=value
                    value.push(c);
                    while let Some((_, c)) = chars.clone().next() {
                        if c.is_whitespace() || c == '}' {
                            break;
                        }
                        value.push(c);
                        chars.next();
                    }
                }
            }
        }

        attrs.push((key, value));
    }

    end_pos.map(|pos| (attrs, pos))
}

// --- shared rendering ---

fn render_directive(
    kind: DirectiveKind,
    name: &str,
    attrs: &Attrs,
    node: DirectiveNode<'_>,
    fmt: &mut DirectiveRenderer<'_, '_>,
) {
    if let Some(render) = node.ext().get::<DirectiveRendererExt>() {
        render.0(kind, name, attrs, node, fmt);
        return;
    }

    let block = kind != DirectiveKind::Text;
    let tag = if block { "div" } else { "span" };
    let mut html_attrs = node.attrs().to_vec();
    html_attrs.push(("class".into(), format!("directive {name}")));
    html_attrs.extend_from_slice(attrs);

    if block {
        fmt.cr();
    }
    fmt.open(tag, &html_attrs);
    fmt.contents(node.children());
    fmt.close(tag);
    if block {
        fmt.cr();
    }
}

struct DirectiveDocumentRenderer;

impl DocumentNodeRenderer<TextDirective> for DirectiveDocumentRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        value: &TextDirective,
        context: &mut DocumentRenderContext<'_>,
        output: &mut DocumentWriter,
    ) {
        render_directive(
            DirectiveKind::Text,
            &value.name,
            &value.attrs,
            DirectiveNode(DirectiveNodeInner::Document(context.document(), node)),
            &mut DirectiveRenderer(DirectiveRendererInner::Document(context, output)),
        );
    }
}

impl DocumentNodeRenderer<LeafDirective> for DirectiveDocumentRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        value: &LeafDirective,
        context: &mut DocumentRenderContext<'_>,
        output: &mut DocumentWriter,
    ) {
        render_directive(
            DirectiveKind::Leaf,
            &value.name,
            &value.attrs,
            DirectiveNode(DirectiveNodeInner::Document(context.document(), node)),
            &mut DirectiveRenderer(DirectiveRendererInner::Document(context, output)),
        );
    }
}

impl DocumentNodeRenderer<ContainerDirective> for DirectiveDocumentRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        value: &ContainerDirective,
        context: &mut DocumentRenderContext<'_>,
        output: &mut DocumentWriter,
    ) {
        render_directive(
            DirectiveKind::Container,
            &value.name,
            &value.attrs,
            DirectiveNode(DirectiveNodeInner::Document(context.document(), node)),
            &mut DirectiveRenderer(DirectiveRendererInner::Document(context, output)),
        );
    }
}

// --- pub method ---

pub fn add(md: &mut MarkdownIt) {
    md.inline.add_migrated_rule::<TextDirective>();
    md.block.add_rule::<LeafDirectiveScanner>();
    md.block.add_document_rule::<LeafDirectiveScanner>();
    md.block.add_rule::<ContainerDirectiveScanner>();
    md.block.add_document_rule::<ContainerDirectiveScanner>();
    md.add_document_renderer::<TextDirective, _>("html", DirectiveDocumentRenderer);
    md.add_document_renderer::<LeafDirective, _>("html", DirectiveDocumentRenderer);
    md.add_document_renderer::<ContainerDirective, _>("html", DirectiveDocumentRenderer);
    md.add_document_renderer::<TextDirective, _>("text", TransparentDocumentRenderer);
    md.add_document_renderer::<LeafDirective, _>("text", TransparentDocumentRenderer);
    md.add_document_renderer::<ContainerDirective, _>("text", TransparentDocumentRenderer);
}

/// Register a custom renderer for directives matching `kind` and `name`.
///
/// If no custom renderer is registered for a directive, the default HTML
/// renderer is used. Neither renderer path performs sanitization; use trusted
/// input or sanitize the final HTML.
pub fn add_render(
    md: &mut MarkdownIt,
    kind: DirectiveKind,
    name: impl Into<String>,
    render: DirectiveRenderFn,
) {
    md.ext
        .get_or_insert_default::<DirectiveRenderers>()
        .map
        .insert((kind, name.into()), render);
}

#[cfg(test)]
mod tests {
    use markdown_it::MarkdownIt;
    use markdown_it::plugins::directives::{
        self,
        Attrs,
        DirectiveKind,
        DirectiveNode,
        DirectiveRenderer,
        TextDirective,
    };

    use crate as markdown_it;

    fn render(src: &str) -> String {
        let mut md = MarkdownIt::empty();
        markdown_it::plugins::cmark::add(&mut md);
        directives::add(&mut md);

        md.parse(src).render().trim().to_owned()
    }

    fn render_with(src: &str, configure: impl FnOnce(&mut MarkdownIt)) -> String {
        let mut md = MarkdownIt::empty();
        markdown_it::plugins::cmark::add(&mut md);
        directives::add(&mut md);
        configure(&mut md);

        md.parse(src).render().trim().to_owned()
    }

    fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> &'a str {
        attrs
            .iter()
            .find_map(|(key, value)| (key == name).then_some(value.as_str()))
            .unwrap_or("")
    }

    fn render_badge(
        kind: DirectiveKind,
        name: &str,
        attrs: &[(String, String)],
        _node: DirectiveNode<'_>,
        fmt: &mut DirectiveRenderer<'_, '_>,
    ) {
        assert_eq!(kind, DirectiveKind::Text);
        assert_eq!(name, "badge");

        let html_attrs = [
            ("data-kind".into(), format!("{kind:?}")),
            ("data-tone".into(), attr(attrs, "tone").to_owned()),
        ];
        fmt.open("mark", &html_attrs);
        fmt.text(attr(attrs, "label"));
        fmt.close("mark");
    }

    fn render_leaf_callout(
        kind: DirectiveKind,
        name: &str,
        attrs: &[(String, String)],
        node: DirectiveNode<'_>,
        fmt: &mut DirectiveRenderer<'_, '_>,
    ) {
        assert_eq!(kind, DirectiveKind::Leaf);
        assert_eq!(name, "callout");
        assert!(node.children().len() == 0);

        let html_attrs = [("data-name".into(), name.to_owned())];
        fmt.cr();
        fmt.open("aside", &html_attrs);
        fmt.open("strong", &[]);
        fmt.text(attr(attrs, "title"));
        fmt.close("strong");
        fmt.close("aside");
        fmt.cr();
    }

    fn render_panel(
        kind: DirectiveKind,
        name: &str,
        attrs: &[(String, String)],
        node: DirectiveNode<'_>,
        fmt: &mut DirectiveRenderer<'_, '_>,
    ) {
        assert_eq!(kind, DirectiveKind::Container);
        assert_eq!(name, "panel");

        let html_attrs = [("data-title".into(), attr(attrs, "title").to_owned())];
        fmt.cr();
        fmt.open("section", &html_attrs);
        fmt.contents(node.children());
        fmt.close("section");
        fmt.cr();
    }

    #[test]
    fn text_directive() {
        let html = render("hello :name{a=\"b\"} world");
        assert_eq!(
            html,
            "<p>hello <span class=\"directive name\" a=\"b\"></span> world</p>"
        );
    }

    #[test]
    fn text_directive_requires_name_immediately_after_colon() {
        let html = render("Note: warning");
        assert_eq!(html, "<p>Note: warning</p>");
    }

    #[test]
    fn text_directive_does_not_start_after_another_colon() {
        let html = render(":::bad trailing");
        assert_eq!(html, "<p>:::bad trailing</p>");
    }

    #[test]
    fn leaf_directive() {
        let html = render("::name{cia=\"llo\"}");
        assert_eq!(html, "<div class=\"directive name\" cia=\"llo\"></div>");
    }

    #[test]
    fn container_directive() {
        let html = render(":::name{cia=\"llo\"}\nworld\n:::");
        assert_eq!(
            html,
            "<div class=\"directive name\" cia=\"llo\">\n<p>world</p>\n</div>"
        );
    }

    #[test]
    fn container_directive_nested() {
        let html = render(":::name\n:::child\nhello\n:::\n:::");
        assert_eq!(
            html,
            "<div class=\"directive name\">\n<div class=\"directive child\">\n<p>hello</p>\n</div>\n</div>"
        );
    }

    #[test]
    fn container_directive_nested_with_longer_marker() {
        let html = render(":::name\n::::child\nhello\n::::\n:::");
        assert_eq!(
            html,
            "<div class=\"directive name\">\n<div class=\"directive child\">\n<p>hello</p>\n</div>\n</div>"
        );
    }

    #[test]
    fn container_directive_closed_by_longer_marker() {
        let html = render(":::name\nhello\n::::");
        assert_eq!(html, "<div class=\"directive name\">\n<p>hello</p>\n</div>");
    }

    #[test]
    fn container_directive_respects_outdent_in_list() {
        let html = render("- :::name\n  hello\noutside");
        assert_eq!(
            html,
            "<ul>\n<li>\n<div class=\"directive name\">\n<p>hello</p>\n</div>\n</li>\n</ul>\n<p>outside</p>"
        );
    }

    #[test]
    fn directive_shorthand_attributes() {
        let html = render(":name{#my-id .my-class}");
        assert_eq!(
            html,
            "<p><span class=\"directive name my-class\" id=\"my-id\"></span></p>"
        );
    }

    #[test]
    fn directive_quoted_attributes() {
        let html = render(":name{title=\"Ciallo World\"}");
        assert_eq!(
            html,
            "<p><span class=\"directive name\" title=\"Ciallo World\"></span></p>"
        );
    }

    #[test]
    fn directive_boolean_attributes() {
        let html = render(":name{disabled}");
        assert_eq!(
            html,
            "<p><span class=\"directive name\" disabled=\"\"></span></p>"
        );
    }

    #[test]
    fn text_directive_uses_registered_custom_render() {
        let html = render_with("hello :badge{label=\"Beta\" tone=\"new\"} world", |md| {
            directives::add_render(md, DirectiveKind::Text, "badge", render_badge);
        });

        assert_eq!(
            html,
            "<p>hello <mark data-kind=\"Text\" data-tone=\"new\">Beta</mark> world</p>"
        );
    }

    #[test]
    fn leaf_directive_uses_registered_custom_render() {
        let html = render_with("::callout{title=\"Heads-up\"}", |md| {
            directives::add_render(md, DirectiveKind::Leaf, "callout", render_leaf_callout);
        });

        assert_eq!(
            html,
            "<aside data-name=\"callout\"><strong>Heads-up</strong></aside>"
        );
    }

    #[test]
    fn container_directive_uses_registered_custom_render_and_children() {
        let html = render_with(":::panel{title=\"Intro\"}\nCiallo **world**\n:::", |md| {
            directives::add_render(md, DirectiveKind::Container, "panel", render_panel);
        });

        assert_eq!(
            html,
            "<section data-title=\"Intro\">\n<p>Ciallo <strong>world</strong></p>\n</section>"
        );
    }

    #[test]
    fn default_renderer_passes_user_attributes_through_without_sanitizing() {
        let cases = [
            (
                r#":name{title="safe" class="admin" id="root" onclick="alert(1)" style="color:red" data-directive-kind="evil" data-directive-name="evil"}"#,
                r#"<p><span class="directive name admin" title="safe" id="root" onclick="alert(1)" style="color:red" data-directive-kind="evil" data-directive-name="evil"></span></p>"#,
            ),
            (
                r#"::name{title="safe" onclick="alert(1)" style="color:red"}"#,
                r#"<div class="directive name" title="safe" onclick="alert(1)" style="color:red"></div>"#,
            ),
            (
                r#":::name{title="safe" onclick="alert(1)" style="color:red"}
body
:::"#,
                r#"<div class="directive name" title="safe" onclick="alert(1)" style="color:red">
<p>body</p>
</div>"#,
            ),
        ];
        for (source, expected) in cases {
            assert_eq!(render(source), expected, "source: {source}");
        }
    }

    #[test]
    fn default_renderer_preserves_node_attributes() {
        let html = render_with(":name", markdown_it::plugins::sourcepos::add);

        assert!(html.contains(r#"<span data-sourcepos=""#));
        assert!(html.contains(r#"class="directive name""#));
    }

    #[test]
    fn directive_attribute_values_are_html_escaped() {
        assert_eq!(
            render(r#":name{title="&<>\""}"#),
            r#"<p><span class="directive name" title="&amp;&lt;&gt;&quot;"></span></p>"#
        );
    }

    fn parse_text_attrs(source: &str) -> Attrs {
        let mut md = MarkdownIt::empty();
        markdown_it::plugins::cmark::add(&mut md);
        directives::add(&mut md);

        let ast = md.parse(source);
        let mut result = None;
        ast.walk(|node, _| {
            if result.is_none()
                && let Some(directives) = node.cast::<TextDirective>()
            {
                result = Some(directives.attrs.clone());
            }
        });

        result.expect("expected a text directive")
    }

    #[test]
    fn directive_attributes_remain_available_in_ast() {
        assert_eq!(
            parse_text_attrs(r#":badge{label="Beta" onclick="alert(1)"}"#),
            vec![
                ("label".to_owned(), "Beta".to_owned()),
                ("onclick".to_owned(), "alert(1)".to_owned()),
            ]
        );
    }

    #[test]
    fn registered_renderer_escapes_text_and_ignores_unknown_attributes() {
        let html = render_with(
            r#":badge{label="<img src=x onerror=alert(1)>" tone="new" onclick="alert(1)"}"#,
            |md| directives::add_render(md, DirectiveKind::Text, "badge", render_badge),
        );

        assert_eq!(
            html,
            r#"<p><mark data-kind="Text" data-tone="new">&lt;img src=x onerror=alert(1)&gt;</mark></p>"#,
        );
    }
}
