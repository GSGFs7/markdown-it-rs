use std::collections::HashMap;
use std::fmt::Debug;

use crate::common::utils::escape_html;
use crate::parser::extset::RenderExtSet;
use crate::parser::node::{HtmlAttribute, Node};
use crate::parser::render_options::RenderOptions;

/// Each node outputs its HTML using this API.
///
/// Renderer is a struct that walks through AST and collects HTML from each node
/// into internal buffer.
pub trait Renderer {
    fn options(&self) -> Option<&RenderOptions> {
        None
    }

    /// Whether this renderer emits XHTML-compatible output.
    fn is_xhtml(&self) -> bool {
        self.options().is_some_and(|options| options.xhtml_out)
    }

    fn softbreak(&mut self) {
        let breaks = self.options().is_some_and(|options| options.breaks);
        if breaks {
            self.self_close("br", &[]);
            self.cr();
        } else {
            self.cr();
        }
    }

    /// Write opening html tag with attributes, e.g. `<a href="url">`.
    fn open(&mut self, tag: &str, attrs: &[HtmlAttribute]);
    /// Write closing html tag, e.g. `</a>`.
    fn close(&mut self, tag: &str);
    /// Write self-closing html tag with attributes, e.g. `<img src="url"/>`.
    fn self_close(&mut self, tag: &str, attrs: &[HtmlAttribute]);
    /// Loop through child nodes and render each one.
    fn contents(&mut self, nodes: &[Node]);
    /// Write line break (`\n`). Default renderer ignores it if last char in the buffer is `\n` already.
    fn cr(&mut self);
    /// Write plain text with escaping, `<div>` -> `&lt;div&gt;`.
    fn text(&mut self, text: &str);
    /// Write plain text without escaping, `<div>` -> `<div>`.
    fn text_raw(&mut self, text: &str);
    /// Extension set to store custom stuff.
    fn ext(&mut self) -> &mut RenderExtSet;
}

#[derive(Debug)]
/// Default HTML/XHTML renderer.
pub(crate) struct HTMLRenderer<'a> {
    result: String,
    ext: RenderExtSet,
    options: &'a RenderOptions,
}

impl<'a> HTMLRenderer<'a> {
    pub fn new(options: &'a RenderOptions) -> Self {
        Self {
            result: String::new(),
            ext: RenderExtSet::new(),
            options,
        }
    }

    pub fn render(&mut self, node: &Node) {
        node.node_value.render(node, self);
    }

    fn make_attr(&mut self, name: &str, value: &str) {
        self.result.push(' ');
        self.result.push_str(&escape_html(name));
        self.result.push('=');
        self.result.push('"');
        self.result.push_str(&escape_html(value));
        self.result.push('"');
    }

    fn make_attrs(&mut self, attrs: &[HtmlAttribute]) {
        let mut attr_hash = HashMap::new();
        let mut attr_order = Vec::with_capacity(attrs.len());

        for (name, value) in attrs {
            let name = name.as_str();
            attr_hash
                .entry(name)
                .or_insert_with(Vec::new)
                .push(value.as_str());
            attr_order.push(name);
        }

        for name in attr_order {
            let Some(value) = attr_hash.remove(name) else {
                continue;
            };

            if name == "class" {
                self.make_attr(name, &value.join(" "));
            } else if name == "style" {
                self.make_attr(name, &value.join(";"));
            } else {
                for v in value {
                    self.make_attr(name, v);
                }
            }
        }
    }
}

impl<'a> From<HTMLRenderer<'a>> for String {
    fn from(f: HTMLRenderer) -> Self {
        #[cold]
        fn replace_null(input: String) -> String {
            input.replace('\0', "\u{FFFD}")
        }

        if f.result.contains('\0') {
            // U+0000 must be replaced with U+FFFD as per commonmark spec,
            // we do it at the very end in order to avoid messing with byte offsets
            // for source maps (since "\0".len() != "\u{FFFD}".len())
            replace_null(f.result)
        } else {
            f.result
        }
    }
}

impl<'a> Renderer for HTMLRenderer<'a> {
    // cover this to provide the options
    fn options(&self) -> Option<&RenderOptions> {
        Some(self.options)
    }

    fn open(&mut self, tag: &str, attrs: &[HtmlAttribute]) {
        self.result.push('<');
        self.result.push_str(tag);
        self.make_attrs(attrs);
        self.result.push('>');
    }

    fn close(&mut self, tag: &str) {
        self.result.push('<');
        self.result.push('/');
        self.result.push_str(tag);
        self.result.push('>');
    }

    fn self_close(&mut self, tag: &str, attrs: &[HtmlAttribute]) {
        self.result.push('<');
        self.result.push_str(tag);
        self.make_attrs(attrs);
        if self.is_xhtml() {
            self.result.push(' ');
            self.result.push('/');
        }
        self.result.push('>');
    }

    fn contents(&mut self, nodes: &[Node]) {
        for node in nodes.iter() {
            stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
                self.render(node);
            });
        }
    }

    fn cr(&mut self) {
        // only push '\n' if last character isn't it
        match self.result.as_bytes().last() {
            Some(b'\n') | None => {}
            Some(_) => self.result.push('\n'),
        }
    }

    fn text(&mut self, text: &str) {
        self.result.push_str(&escape_html(text));
    }

    fn text_raw(&mut self, text: &str) {
        self.result.push_str(text);
    }

    fn ext(&mut self) -> &mut RenderExtSet {
        &mut self.ext
    }
}
