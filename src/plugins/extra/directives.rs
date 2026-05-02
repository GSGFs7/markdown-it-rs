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
//! Custom renderers can be registered by [`add_render`]. They are matched by
//! directive kind and directive name, and receive the parsed attributes plus
//! the parsed node so they can render their own HTML.
//!
//! ```rust
//! use markdown_it::plugins::extra::directives::{self, DirectiveKind};
//! use markdown_it::{MarkdownIt, Node, Renderer};
//!
//! fn render_badge(
//!     kind: DirectiveKind,
//!     name: &str,
//!     attrs: &[(String, String)],
//!     _node: &Node,
//!     fmt: &mut dyn Renderer,
//! ) {
//!     assert_eq!(kind, DirectiveKind::Text);
//!     assert_eq!(name, "badge");
//!
//!     let label = attrs
//!         .iter()
//!         .find_map(|(key, value)| (key == "label").then_some(value.as_str()))
//!         .unwrap_or("");
//!
//!     fmt.open("mark", &[("class", "badge".to_owned())]);
//!     fmt.text(label);
//!     fmt.close("mark");
//! }
//!
//! let mut md = MarkdownIt::new();
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

use crate::parser::block::{BlockRule, BlockState};
use crate::parser::extset::{MarkdownItExt, NodeExt};
use crate::parser::inline::{InlineRule, InlineState};
use crate::{MarkdownIt, Node, NodeValue, Renderer};

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
        if render_custom(DirectiveKind::Text, &self.name, &self.attrs, node, fmt) {
            return;
        }

        let mut attrs: Vec<(&str, String)> =
            node.attrs.iter().map(|(k, v)| (*k, v.clone())).collect();
        attrs.push(("class", format!("directive {}", self.name)));
        for (k, v) in &self.attrs {
            attrs.push((k.as_str(), v.clone()));
        }

        fmt.open("span", &attrs);
        fmt.contents(&node.children);
        fmt.close("span");
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
        if render_custom(DirectiveKind::Leaf, &self.name, &self.attrs, node, fmt) {
            return;
        }

        let mut attrs: Vec<(&str, String)> =
            node.attrs.iter().map(|(k, v)| (*k, v.clone())).collect();
        attrs.push(("class", format!("directive {}", self.name)));
        for (k, v) in &self.attrs {
            attrs.push((k.as_str(), v.clone()));
        }

        fmt.cr();
        fmt.open("div", &attrs);
        fmt.contents(&node.children);
        fmt.close("div");
        fmt.cr();
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
        if render_custom(DirectiveKind::Container, &self.name, &self.attrs, node, fmt) {
            return;
        }

        let mut attrs: Vec<(&str, String)> =
            node.attrs.iter().map(|(k, v)| (*k, v.clone())).collect();
        attrs.push(("class", format!("directive {}", self.name)));
        for (k, v) in &self.attrs {
            attrs.push((k.as_str(), v.clone()));
        }

        fmt.cr();
        fmt.open("div", &attrs);
        fmt.contents(&node.children);
        fmt.close("div");
        fmt.cr();
    }
}

// --- scanner ---

impl InlineRule for TextDirective {
    const MARKER: char = ':';
    const NAMES: &'static [&'static str] = &["text_directive"];

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let src = &state.src[state.pos..state.pos_max];
        // avoid affect other directive
        if !src.starts_with(':') || src.starts_with("::") {
            return None;
        }
        if state.pos > 0 && state.src[..state.pos].ends_with(':') {
            return None;
        }

        let mut pos = 1;
        let (name, name_len) = parse_name(&src[pos..])?;
        pos += name_len;
        pos += src[pos..].len() - src[pos..].trim_start().len();
        let (attrs, attrs_len) = parse_attrs(&src[pos..])?;
        pos += attrs_len;

        let mut node = Node::new(TextDirective {
            name: name.clone(),
            attrs,
        });
        attach_render(&mut node, state.md, DirectiveKind::Text, &name);

        Some((node, pos))
    }
}

pub struct LeafDirectiveScanner;

impl BlockRule for LeafDirectiveScanner {
    const NAMES: &'static [&'static str] = &["leaf_directive"];

    fn run(state: &mut BlockState) -> Option<(Node, usize)> {
        // it should be a codeblocks
        if state.line_indent(state.line) >= state.md.max_indent {
            return None;
        }

        let line = state.get_line(state.line).trim_end();
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

        let mut node = Node::new(LeafDirective {
            name: name.clone(),
            attrs,
        });
        attach_render(&mut node, state.md, DirectiveKind::Leaf, &name);

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

    fn is_close(line: &str, marker_len: usize) -> bool {
        let line = line.trim_end();
        let len = line.bytes().take_while(|b| *b == b':').count();
        // marker lenght avail & no other chars
        len >= marker_len && line[len..].trim().is_empty()
    }
}

impl BlockRule for ContainerDirectiveScanner {
    const NAMES: &'static [&'static str] = &["container_directive"];

    fn check(state: &mut BlockState) -> Option<()> {
        Self::scan(state).map(|_| ())
    }

    fn run(state: &mut BlockState) -> Option<(Node, usize)> {
        let (marker_len, name, attrs) = Self::scan(state)?;

        let start_line = state.line;
        let mut next_line = start_line;
        let mut have_end_marker = false;
        // nest
        let mut marker_stack = vec![marker_len];

        // process nest directive
        // :::outer
        //   :::inner
        //     ...
        //   :::
        // :::
        loop {
            next_line += 1;
            if next_line >= state.line_max {
                break;
            }

            if !state.is_empty(next_line) && state.line_indent(next_line) < 0 {
                break;
            }

            if state.line_indent(next_line) < state.md.max_indent {
                let line = state.get_line(next_line);
                let current_marker_len = *marker_stack.last().unwrap();
                // check close
                if Self::is_close(line, current_marker_len) {
                    marker_stack.pop();
                    if marker_stack.is_empty() {
                        have_end_marker = true;
                        break;
                    }
                    continue;
                }

                // if find a new open mark
                if let Some((nested_marker_len, _, _)) = Self::scan_line(line) {
                    marker_stack.push(nested_marker_len);
                }
            }
        }

        // new node
        let mut directive_node = Node::new(ContainerDirective {
            name: name.clone(),
            attrs,
        });
        attach_render(
            &mut directive_node,
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
        state.md.block.tokenize(state);

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
/// The callback receives the directive kind, name, parsed attributes, parsed
/// node, and active renderer. Use [`Renderer::text`] for user-provided text and
/// reserve [`Renderer::text_raw`] for trusted HTML.
pub type DirectiveRenderFn = fn(DirectiveKind, &str, &[(String, String)], &Node, &mut dyn Renderer);

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

impl MarkdownItExt for DirectiveRenderers {}

#[derive(Clone, Copy)]
struct DirectiveRendererExt(DirectiveRenderFn);

impl Debug for DirectiveRendererExt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirectiveRendererExt").finish()
    }
}

impl NodeExt for DirectiveRendererExt {}

// --- helper method ---

// render the custom rule
// return true if render success, otherwise return false
fn render_custom(
    kind: DirectiveKind,
    name: &str,
    attrs: &[(String, String)],
    node: &Node,
    fmt: &mut dyn Renderer,
) -> bool {
    if let Some(render) = node.ext.get::<DirectiveRendererExt>() {
        render.0(kind, name, attrs, node, fmt);
        true
    } else {
        false
    }
}

// add a custom render
fn attach_render(node: &mut Node, md: &MarkdownIt, kind: DirectiveKind, name: &str) {
    let Some(renderers) = md.ext.get::<DirectiveRenderers>() else {
        return;
    };

    if let Some(render) = renderers.map.get(&(kind, name.to_owned())) {
        node.ext.insert(DirectiveRendererExt(*render));
    }
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

// --- pub method ---

pub fn add(md: &mut MarkdownIt) {
    md.inline.add_rule::<TextDirective>();
    md.block.add_rule::<LeafDirectiveScanner>();
    md.block.add_rule::<ContainerDirectiveScanner>();
}

/// Register a custom renderer for directives matching `kind` and `name`.
///
/// If no custom renderer is registered for a directive, the default HTML
/// renderer is used.
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
