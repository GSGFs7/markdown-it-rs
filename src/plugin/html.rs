use crate::render::{
    RenderOptions,
    Writer,
    write_html_close,
    write_html_open,
    write_html_self_close,
};

/// Small HTML tree builder passed to high-level renderers.
///
/// Text and attribute values are escaped when they are added. The tree is
/// emitted by [`render`](Self::render).
#[derive(Debug, Default)]
pub struct Html {
    nodes: Vec<HtmlNode>,
}

impl Html {
    /// Create an empty HTML fragment.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a child element.
    ///
    /// The element is emitted when it is dropped at the end of the statement:
    /// ```ignore
    /// html.element("span").class("emoji").text("🚀");
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if `tag` is not a valid ASCII HTML tag name.
    pub fn element(&mut self, tag: impl Into<String>) -> HtmlElement<'_> {
        HtmlElement::new(&mut self.nodes, tag.into())
    }

    /// Append text. It is escaped when the fragment is rendered.
    pub fn text(&mut self, text: impl AsRef<str>) -> &mut Self {
        self.nodes.push(HtmlNode::Text(text.as_ref().to_owned()));
        self
    }

    /// Append markup verbatim. The caller is responsible for escaping it.
    pub fn raw(&mut self, html: impl AsRef<str>) -> &mut Self {
        self.nodes.push(HtmlNode::Raw(html.as_ref().to_owned()));
        self
    }

    /// Render this fragment to an HTML string with default options.
    pub fn render(&self) -> String {
        self.render_with_options(&RenderOptions::default())
    }

    /// Render this fragment to an HTML string using the supplied options.
    ///
    /// Void elements use XHTML self-closing syntax when
    /// [`RenderOptions::xhtml_out`] is enabled.
    pub fn render_with_options(&self, options: &RenderOptions) -> String {
        let mut output = Writer::default();
        self.render_into(&mut output, options);
        output.finish()
    }

    /// Render this fragment directly into an existing writer.
    ///
    /// This avoids allocating an intermediate `String` when the fragment is
    /// part of a larger rendering pass.
    pub fn render_into(&self, output: &mut Writer, options: &RenderOptions) {
        render_html_nodes(&self.nodes, output, options.xhtml_out);
    }
}

/// One element under construction.
///
/// Chained calls return `&mut Self`, and the element is appended to its parent
/// when the value is dropped.
#[derive(Debug)]
pub struct HtmlElement<'a> {
    target: &'a mut Vec<HtmlNode>,
    tag: String,
    attrs: Vec<(String, String)>,
    children: Vec<HtmlNode>,
    void: bool,
}

impl<'a> HtmlElement<'a> {
    fn new(target: &'a mut Vec<HtmlNode>, tag: String) -> Self {
        validate_tag(&tag);

        Self {
            target,
            tag,
            attrs: Vec::new(),
            children: Vec::new(),
            void: false,
        }
    }

    /// Add an attribute. It is escaped when the fragment is rendered, and the
    /// name must be a valid HTML attribute name.
    ///
    /// # Panics
    ///
    /// Panics if `name` is not a valid HTML attribute name.
    pub fn attr(&mut self, name: impl Into<String>, value: impl AsRef<str>) -> &mut Self {
        let name = name.into();
        validate_attr_name(&name);
        self.attrs.push((name, value.as_ref().to_owned()));
        self
    }

    /// Add a `class` attribute.
    pub fn class(&mut self, class: impl AsRef<str>) -> &mut Self {
        self.attr("class", class)
    }

    /// Mark this element as void, e.g. `<img>` or `<br>`.
    pub fn void(&mut self) -> &mut Self {
        self.void = true;
        self
    }

    /// Add a child element.
    ///
    /// # Panics
    ///
    /// Panics if `tag` is not a valid ASCII HTML tag name.
    pub fn element(&mut self, tag: impl Into<String>) -> HtmlElement<'_> {
        HtmlElement::new(&mut self.children, tag.into())
    }

    /// Append text. It is escaped when the fragment is rendered.
    pub fn text(&mut self, text: impl AsRef<str>) -> &mut Self {
        self.children.push(HtmlNode::Text(text.as_ref().to_owned()));
        self
    }

    /// Append markup verbatim. The caller is responsible for escaping it.
    pub fn raw(&mut self, html: impl AsRef<str>) -> &mut Self {
        self.children.push(HtmlNode::Raw(html.as_ref().to_owned()));
        self
    }
}

impl Drop for HtmlElement<'_> {
    fn drop(&mut self) {
        self.target.push(HtmlNode::Element {
            tag: std::mem::take(&mut self.tag),
            attrs: std::mem::take(&mut self.attrs),
            children: std::mem::take(&mut self.children),
            void: self.void,
        });
    }
}

#[derive(Debug)]
enum HtmlNode {
    Element {
        tag: String,
        attrs: Vec<(String, String)>,
        children: Vec<HtmlNode>,
        void: bool,
    },
    Text(String),
    Raw(String),
}

fn render_html_nodes(nodes: &[HtmlNode], output: &mut Writer, xhtml_out: bool) {
    for node in nodes {
        match node {
            HtmlNode::Element {
                tag,
                attrs,
                children,
                void,
            } => {
                if *void {
                    write_html_self_close(output, tag, attrs, xhtml_out);
                } else {
                    write_html_open(output, tag, attrs);
                    render_html_nodes(children, output, xhtml_out);
                    write_html_close(output, tag);
                }
            }
            HtmlNode::Text(text) => output.write_escaped_html(text),
            HtmlNode::Raw(html) => output.write_str(html),
        }
    }
}

pub(super) fn validate_tag(tag: &str) {
    assert!(
        !tag.is_empty()
            && tag
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':')),
        "invalid HTML tag name: {tag:?}"
    );
}

fn validate_attr_name(name: &str) {
    assert!(
        !name.is_empty()
            && name.chars().all(|ch| {
                ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':' | '.' | '@' | '#')
            }),
        "invalid HTML attribute name: {name:?}"
    );
}

pub(super) fn render_html_fragment(
    render: impl FnOnce(&mut Html),
    options: &RenderOptions,
    output: &mut Writer,
) {
    let mut html = Html::new();
    render(&mut html);
    html.render_into(output, options);
}
