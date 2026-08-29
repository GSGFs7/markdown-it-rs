//! GFM-style alerts, like:
//!
//! ```markdown
//! > [!NOTE]
//! > Some useful information here.
//! ```
//!
//! Rendered as a `<div class="markdown-alert markdown-alert-note">` block with a
//! `<p class="markdown-alert-title">` header. Supported types are `NOTE`, `TIP`,
//! `IMPORTANT`, `WARNING`, and `CAUTION`; the marker must be uppercase and occupy
//! the first line of the blockquote, otherwise it is left as plain text.
//!
//! The plugin emits GitHub-compatible class names but ships no styles. The
//! following minimal stylesheet adds the colored left border and title used by
//! [GitHub alerts](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#alerts):
//!
//! ```css
//! .markdown-alert {
//!   margin: 16px 0;
//!   padding: 8px 16px;
//!   border-left: 4px solid;
//!   border-radius: 6px;
//! }
//! .markdown-alert > :last-child { margin-bottom: 0; }
//! .markdown-alert-title { margin: 0 0 8px; font-weight: 600; }
//!
//! .markdown-alert-note { border-left-color: #0969da; }
//! .markdown-alert-note .markdown-alert-title { color: #0969da; }
//! .markdown-alert-tip { border-left-color: #1a7f37; }
//! .markdown-alert-tip .markdown-alert-title { color: #1a7f37; }
//! .markdown-alert-important { border-left-color: #8250df; }
//! .markdown-alert-important .markdown-alert-title { color: #8250df; }
//! .markdown-alert-warning { border-left-color: #9a6700; }
//! .markdown-alert-warning .markdown-alert-title { color: #9a6700; }
//! .markdown-alert-caution { border-left-color: #cf222e; }
//! .markdown-alert-caution .markdown-alert-title { color: #cf222e; }
//! ```
//!
//! GitHub also places a decorative Octicon SVG before each title. Those icons
//! are intentionally omitted here. You can add them yourself via renderer.
//!
//! Enabled via [`crate::plugins::extra::add`], or standalone:
//!
//! ```
//! let md = &mut markdown_it::MarkdownIt::empty();
//! markdown_it::plugins::cmark::add(md);
//! markdown_it::plugins::extra::alert::add(md);
//!
//! let html = md.parse("> [!WARNING]\n> Danger!").render();
//! assert!(html.contains("markdown-alert-warning"));
//! ```

use crate::parser::core::CoreRule;
use crate::parser::inline::Text;
use crate::parser::inline::builtin::InlineParserRule;
use crate::plugins::cmark::block::blockquote::Blockquote;
use crate::plugins::cmark::block::paragraph::Paragraph;
use crate::plugins::cmark::inline::newline::{Hardbreak, Softbreak};
use crate::{MarkdownIt, Node, NodeValue, Renderer};

#[derive(Debug, Clone, Copy)]
pub enum AlertKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

impl AlertKind {
    fn parse(marker: &str) -> Option<Self> {
        match marker {
            "[!NOTE]" => Some(Self::Note),
            "[!TIP]" => Some(Self::Tip),
            "[!IMPORTANT]" => Some(Self::Important),
            "[!WARNING]" => Some(Self::Warning),
            "[!CAUTION]" => Some(Self::Caution),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Tip => "tip",
            Self::Important => "important",
            Self::Warning => "warning",
            Self::Caution => "caution",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Note => "Note",
            Self::Tip => "Tip",
            Self::Important => "Important",
            Self::Warning => "Warning",
            Self::Caution => "Caution",
        }
    }
}

#[derive(Debug)]
pub struct Alert {
    pub kind: AlertKind,
}

impl NodeValue for Alert {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        let mut attrs = node.attrs.clone();
        attrs.push(("class".into(), "markdown-alert".into()));
        attrs.push((
            "class".into(),
            format!("markdown-alert-{}", self.kind.name()),
        ));

        fmt.cr();
        fmt.open("div", &attrs);
        fmt.cr();

        fmt.open("p", &[("class".into(), "markdown-alert-title".into())]);
        fmt.text(self.kind.title());
        fmt.close("p");
        fmt.cr();

        fmt.contents(&node.children);
        fmt.close("div");
        fmt.cr();
    }
}

pub struct AlertScanner;

impl AlertScanner {
    fn process(node: &mut Node) {
        if !node.is::<Blockquote>() {
            return;
        }

        let Some(paragraph) = node.children.first_mut() else {
            return;
        };
        if !paragraph.is::<Paragraph>() {
            return;
        }

        let Some(text) = paragraph.children.first().and_then(|n| n.cast::<Text>()) else {
            return;
        };
        let Some(kind) = AlertKind::parse(&text.content) else {
            return;
        };

        // the marker must occupy the first line
        if paragraph.children.len() > 1
            && !paragraph.children[1].is::<Softbreak>()
            && !paragraph.children[1].is::<Hardbreak>()
        {
            return;
        }

        paragraph.children.remove(0);
        if paragraph
            .children
            .first()
            .is_some_and(|n| n.is::<Softbreak>() || n.is::<Hardbreak>())
        {
            paragraph.children.remove(0);
        }
        if paragraph.children.is_empty() {
            node.children.remove(0);
        }

        node.replace(Alert { kind });
    }
}

impl CoreRule for AlertScanner {
    const NAMES: &'static [&'static str] = &["alerts", "github_alerts"];

    fn run(root: &mut Node, _md: &crate::MarkdownIt) {
        root.walk_mut(|node, depth| {
            // must in the top level
            if depth == 1 {
                Self::process(node);
            }
        });
    }
}

pub fn add(md: &mut MarkdownIt) {
    md.add_rule::<AlertScanner>().after::<InlineParserRule>();
}

#[cfg(test)]
mod test {
    fn render(input: &str) -> String {
        let md = &mut crate::MarkdownIt::empty();
        crate::plugins::cmark::add(md);
        super::add(md);
        md.parse(input).render()
    }

    fn assert_render(input: &str, expected: &str) {
        assert_eq!(render(input), expected);
    }

    #[test]
    fn supports_all_github_alert_types() {
        let cases = [
            ("NOTE", "note", "Note"),
            ("TIP", "tip", "Tip"),
            ("IMPORTANT", "important", "Important"),
            ("WARNING", "warning", "Warning"),
            ("CAUTION", "caution", "Caution"),
        ];
        for (marker, class, title) in cases {
            let input = format!(">[!{marker}]\n>content");
            let expected = format!(
                "<div class=\"markdown-alert markdown-alert-{class}\">\n<p class=\"markdown-alert-title\">{title}</p>\n<p>content</p>\n</div>\n"
            );
            assert_eq!(render(&input), expected, "failed for {marker}");
        }
    }

    #[test]
    fn preserves_inline_markdown() {
        assert_render(
            "> [!NOTE]\n> Use **bold**, `code`, and [links](https://example.com).",
            "<div class=\"markdown-alert markdown-alert-note\">\n<p class=\"markdown-alert-title\">Note</p>\n<p>Use <strong>bold</strong>, <code>code</code>, and <a href=\"https://example.com\">links</a>.</p>\n</div>\n",
        );
    }

    #[test]
    fn preserves_block_content() {
        assert_render(
            "> [!NOTE]\n> First paragraph.\n>\n> - one\n> - two",
            "<div class=\"markdown-alert markdown-alert-note\">\n<p class=\"markdown-alert-title\">Note</p>\n<p>First paragraph.</p>\n<ul>\n<li>one</li>\n<li>two</li>\n</ul>\n</div>\n",
        );
    }

    #[test]
    fn unknown_type_remains_blockquote() {
        assert_render(
            "> [!UNKNOWN]\n> content",
            "<blockquote>\n<p>[!UNKNOWN]\ncontent</p>\n</blockquote>\n",
        );
    }

    #[test]
    fn marker_must_occupy_first_line() {
        assert_render(
            "> [!NOTE] content",
            "<blockquote>\n<p>[!NOTE] content</p>\n</blockquote>\n",
        );
    }

    #[test]
    fn marker_after_other_text_is_not_recognized() {
        assert_render(
            "> before\n> [!NOTE]\n> after",
            "<blockquote>\n<p>before\n[!NOTE]\nafter</p>\n</blockquote>\n",
        );
    }

    #[test]
    fn lowercase_marker_is_not_recognized() {
        assert_render(
            "> [!note]\n> content",
            "<blockquote>\n<p>[!note]\ncontent</p>\n</blockquote>\n",
        );
    }

    #[test]
    fn supports_empty_alert() {
        assert_render(
            "> [!NOTE]",
            "<div class=\"markdown-alert markdown-alert-note\">\n<p class=\"markdown-alert-title\">Note</p>\n</div>\n",
        );
    }

    #[test]
    fn alert_inside_list_is_not_recognized() {
        let html = render("- item\n\n    > [!NOTE]\n    > nested");

        assert!(!html.contains("markdown-alert"));
        assert!(html.contains("[!NOTE]"));
        assert!(html.contains("<blockquote>"));
    }

    #[test]
    fn nested_blockquote_alert_is_not_recognized() {
        let html = render("> outer\n>\n> > [!NOTE]\n> > nested");

        assert!(!html.contains("markdown-alert"));
        assert!(html.contains("[!NOTE]"));
    }

    #[test]
    fn ordinary_blockquote_is_unchanged() {
        assert_render(
            "> ordinary quote",
            "<blockquote>\n<p>ordinary quote</p>\n</blockquote>\n",
        );
    }

    #[test]
    fn extra_plugin_enables_alerts() {
        let md = &mut crate::MarkdownIt::empty();
        crate::plugins::cmark::add(md);
        crate::plugins::extra::add(md);

        let html = md.parse("> [!NOTE]\n> content").render();

        assert!(html.contains("markdown-alert-note"));
    }
}
