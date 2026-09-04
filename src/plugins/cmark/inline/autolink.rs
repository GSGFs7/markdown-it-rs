//! Autolinks
//!
//! `<https://example.org>`
//!
//! <https://spec.commonmark.org/0.30/#autolinks>
use std::sync::LazyLock;

use regex::Regex;

use crate::parser::document::NodeRef;
use crate::parser::document_renderer::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    DocumentRenderError,
    TransparentDocumentRenderer,
    write_html_close,
    write_html_open,
};
use crate::parser::inline::{InlineRule, InlineState, TextSpecial};
use crate::parser::main::MarkdownIt;
use crate::parser::node::{Node, NodeValue};
use crate::parser::renderer::Renderer;

#[derive(Debug)]
pub struct Autolink {
    pub url: String,
}

struct AutolinkDocumentRenderer;

impl DocumentNodeRenderer<Autolink> for AutolinkDocumentRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        link: &Autolink,
        context: &mut DocumentRenderContext<'_>,
        output: &mut crate::DocumentWriter,
    ) -> Result<(), DocumentRenderError> {
        let mut attrs = node.attrs().clone();
        attrs.push(("href".into(), link.url.clone()));
        write_html_open(output, "a", &attrs)?;
        context.render_children(node.id(), output)?;
        write_html_close(output, "a")
    }
}

impl NodeValue for Autolink {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        let mut attrs = node.attrs.clone();
        attrs.push(("href".into(), self.url.clone()));

        fmt.open("a", &attrs);
        fmt.contents(&node.children);
        fmt.close("a");
    }
}

pub fn add(md: &mut MarkdownIt) {
    md.inline.add_rule::<AutolinkScanner>();
    md.add_document_renderer::<Autolink, _>("html", AutolinkDocumentRenderer);
    md.add_document_renderer::<Autolink, _>("text", TransparentDocumentRenderer);
}

static AUTOLINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([a-zA-Z][a-zA-Z0-9+.\-]{1,31}):([^<>\x00-\x20]*)$").unwrap());

static EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*)$").unwrap()
});

#[doc(hidden)]
pub struct AutolinkScanner;
impl InlineRule for AutolinkScanner {
    const MARKER: char = '<';
    const NAMES: &'static [&'static str] = &["autolink"];

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let mut chars = state.src[state.pos..state.pos_max].chars();
        if chars.next().unwrap() != '<' {
            return None;
        }

        let mut pos = state.pos + 2;

        loop {
            match chars.next() {
                Some('<') | None => return None,
                Some('>') => break,
                Some(x) => pos += x.len_utf8(),
            }
        }

        let url = &state.src[state.pos + 1..pos - 1];
        let is_autolink = AUTOLINK_RE.is_match(url);
        let is_email = EMAIL_RE.is_match(url);

        if !is_autolink && !is_email {
            return None;
        }

        let full_url = if is_autolink {
            state.md.link_formatter.normalize_link(url)
        } else {
            state
                .md
                .link_formatter
                .normalize_link(&("mailto:".to_owned() + url))
        };

        state.md.link_formatter.validate_link(&full_url)?;

        let content = state.md.link_formatter.normalize_link_text(url);

        let mut inner_node = Node::new(TextSpecial {
            content: content.clone(),
            markup: content,
            info: "autolink",
        });
        inner_node.srcmap = state.get_map(state.pos + 1, pos - 1);

        let mut node = Node::new(Autolink { url: full_url });
        node.children.push(inner_node);

        Some((node, pos - state.pos))
    }
}
