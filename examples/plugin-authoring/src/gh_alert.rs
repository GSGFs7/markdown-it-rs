//! GitHub-style alert plugin.
//!
//! A transform runs after inline parsing and rewrites blockquotes whose first
//! line is an alert marker such as `[!NOTE]` into [`Alert`] containers. The
//! marker line is removed and the rest of the blockquote becomes the body.
//!
//! Supported kinds are `NOTE`, `TIP`, `IMPORTANT`, `WARNING`, and `CAUTION`.
//! HTML rendering adds a title and CSS classes; text rendering keeps the title.
//!
//! ```markdown
//! > [!WARNING]
//! > Mind the gap.
//! ```
//!
//! # What it demonstrates
//!
//! - Rewriting an existing node while preserving its children.
//! - Scheduling a transform after inline parsing and before dependent rules.
//! - Updating attributes and source maps with `EditBatch`.
//! - Providing both HTML and plain-text renderers for one node type.
//!
//! # Test coverage
//!
//! - `marker_becomes_titled_alert_with_body`: markers become titled alerts and
//!   keep their inline formatting and multiple paragraphs.
//! - `marker_only_alert_has_no_empty_paragraph`: a marker-only alert leaves no
//!   empty paragraph behind.
//! - `every_kind_maps_to_its_class_and_title`: all five kinds map to the right
//!   CSS class and title; markers match case-insensitively.
//! - `non_marker_and_misplaced_markers_stay_blockquotes`: unknown kinds, bodies
//!   on the marker line, and markers after the first line are not alerts.
//! - `alerts_nest_inside_lists_and_blockquotes`: alerts work inside lists.
//! - `multiple_alerts_in_one_document_do_not_share_state`: multiple alerts in
//!   one document remain independent.
//! - `text_format_keeps_title_and_body`: plain-text output preserves the title
//!   and body.
//! - `later_core_rules_observe_the_rewritten_alert`: rule dependencies let a
//!   later rule see the rewritten alert regardless of registration order.
//! - `rewriting_keeps_source_maps_of_container_and_body` (with `sourcemap`):
//!   the container and body keep their coordinates, and the body paragraph is
//!   trimmed to the content that survives marker removal.

use markdown_it::MarkdownIt;
#[cfg(feature = "sourcemap")]
use markdown_it::common::sourcemap::SourcePos;
use markdown_it::common::utils::escape_html;
use markdown_it::document::edit::EditBatch;
use markdown_it::document::{Document, Node, NodeId, Text};
use markdown_it::plugin::PluginSpec;
use markdown_it::plugins::cmark::block::blockquote::Blockquote;
use markdown_it::plugins::cmark::block::paragraph::Paragraph;
use markdown_it::plugins::cmark::inline::newline::{Hardbreak, Softbreak};
use markdown_it::render::{NodeRenderer, RenderContext, Writer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

impl AlertKind {
    fn from_marker(text: &str) -> Option<Self> {
        let kind = text.trim_end().strip_prefix("[!")?.strip_suffix(']')?;
        let kind = kind.to_ascii_lowercase();
        Some(match kind.as_str() {
            "note" => Self::Note,
            "tip" => Self::Tip,
            "important" => Self::Important,
            "warning" => Self::Warning,
            "caution" => Self::Caution,
            _ => return None,
        })
    }

    /// CSS class suffix used in `markdown-alert-{name}`.
    fn name(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Tip => "tip",
            Self::Important => "important",
            Self::Warning => "warning",
            Self::Caution => "caution",
        }
    }

    /// Human-readable title shown at the top of the alert.
    fn title(self) -> &'static str {
        match self {
            Self::Note => "Note",
            Self::Tip => "Tip",
            Self::Important => "Important",
            Self::Warning => "Warning",
            Self::Caution => "Caution",
        }
    }

    #[cfg(test)]
    const ALL: [Self; 5] = [
        Self::Note,
        Self::Tip,
        Self::Important,
        Self::Warning,
        Self::Caution,
    ];
}

/// Payload of an alert container.
///
/// The container keeps the body's original children; only the marker line is
/// removed.
#[derive(Debug)]
pub struct Alert(pub AlertKind);

struct Rewrite {
    blockquote: NodeId,
    marker: NodeId,
    /// Soft or hard break directly after the marker line.
    marker_break: Option<NodeId>,
    /// Paragraph holding the marker. It is removed when nothing follows the
    /// marker in the same paragraph.
    paragraph: NodeId,
    body_is_empty: bool,
    kind: AlertKind,
    /// Byte offsets of the marker paragraph and of the body that survives it.
    #[cfg(feature = "sourcemap")]
    trimmed_map: Option<(usize, usize)>,
}

fn rewrite_alerts(document: &mut Document, _md: &MarkdownIt) {
    let rewrites = collect_rewrites(document);
    if rewrites.is_empty() {
        return;
    }

    let mut edits = EditBatch::new();
    for rewrite in rewrites {
        edits.replace_value(rewrite.blockquote, Alert(rewrite.kind));
        edits.set_attribute(
            rewrite.blockquote,
            "class",
            format!("markdown-alert markdown-alert-{}", rewrite.kind.name()),
        );
        if rewrite.body_is_empty {
            // The marker paragraph holds nothing else; drop it whole.
            document.remove_node(rewrite.paragraph);
        } else {
            document.remove_node(rewrite.marker);
            if let Some(marker_break) = rewrite.marker_break {
                document.remove_node(marker_break);
            }
            // Removing the marker line does not move existing maps, so the
            // paragraph would still claim the marker's source range.
            #[cfg(feature = "sourcemap")]
            if let Some((start, end)) = rewrite.trimmed_map {
                edits.set_source_map(rewrite.paragraph, Some(SourcePos::new(start, end)));
            }
        }
    }
    edits.commit(document);
}

fn collect_rewrites(document: &markdown_it::document::Document) -> Vec<Rewrite> {
    let mut found = Vec::new();
    document.walk(document.root(), |id| {
        let node = document.node(id);
        if !node.is::<Blockquote>() {
            return;
        }
        let Some(&paragraph) = node.children().first() else {
            return;
        };
        let paragraph_node = document.node(paragraph);
        if !paragraph_node.is::<Paragraph>() {
            return;
        }
        let Some(&marker) = paragraph_node.children().first() else {
            return;
        };
        let Some(text) = document.value::<Text>(marker) else {
            return;
        };
        let Some(kind) = AlertKind::from_marker(&text.content) else {
            return;
        };
        // Only a break may follow the marker on its line; anything else means
        // the body started on the marker line, which GH also rejects.
        let marker_break = paragraph_node.children().get(1).copied().filter(|&next| {
            let node = document.node(next);
            node.is::<Softbreak>() || node.is::<Hardbreak>()
        });
        let body_is_empty =
            paragraph_node.children().len() == if marker_break.is_some() { 2 } else { 1 };
        #[cfg(feature = "sourcemap")]
        let trimmed_map = if body_is_empty {
            None
        } else {
            let first_body = paragraph_node.children()[1 + usize::from(marker_break.is_some())];
            document
                .node(paragraph)
                .srcmap()
                .zip(document.node(first_body).srcmap())
                .map(|(end, start)| (start.get_byte_offsets().0, end.get_byte_offsets().1))
        };
        found.push(Rewrite {
            blockquote: id,
            marker,
            marker_break,
            paragraph,
            body_is_empty,
            kind,
            #[cfg(feature = "sourcemap")]
            trimmed_map,
        });
    });
    found
}

struct AlertRenderer;

impl NodeRenderer<Alert> for AlertRenderer {
    fn render(
        &self,
        node: &Node,
        alert: &Alert,
        context: &mut RenderContext<'_>,
        output: &mut Writer,
    ) {
        context.cr(output);
        // No shared HTML helpers are exported, so attributes are written here.
        output.write_str("<div");
        write_attrs(output, node.attrs());
        output.write_char('>');
        context.cr(output);
        output.write_str("<p class=\"markdown-alert-title\">");
        output.write_str(&escape_html(alert.0.title()));
        output.write_str("</p>");
        context.cr(output);
        context.render_children(node.id(), output);
        output.write_str("</div>");
        context.cr(output);
    }
}

struct AlertTextRenderer;

impl NodeRenderer<Alert> for AlertTextRenderer {
    fn render(
        &self,
        node: &Node,
        alert: &Alert,
        context: &mut RenderContext<'_>,
        output: &mut Writer,
    ) {
        context.cr(output);
        output.write_str(alert.0.title());
        output.write_char(':');
        context.cr(output);
        context.render_children(node.id(), output);
        context.cr(output);
    }
}

fn write_attrs(output: &mut Writer, attrs: &[(String, String)]) {
    for (name, value) in attrs {
        output.write_char(' ');
        output.write_str(name);
        output.write_str("=\"");
        output.write_str(&escape_html(value));
        output.write_char('"');
    }
}

/// Build the plugin.
pub fn plugin() -> PluginSpec {
    PluginSpec::new("gh-alert")
        .transform("rewrite-alerts", rewrite_alerts)
        .with(PluginSpec::custom("gh-alert", |md| {
            md.add_renderer::<Alert, _>("html", AlertRenderer);
            md.add_renderer::<Alert, _>("text", AlertTextRenderer);
        }))
}

/// Install the rewrite and its renderers on a parser.
pub fn install(md: &mut MarkdownIt) {
    plugin().install(md);
}

#[cfg(test)]
mod tests {
    use markdown_it::MarkdownIt;
    use markdown_it::document::Document;
    use markdown_it::parser::core::{CoreRule, CoreState};

    use super::*;

    fn parser() -> MarkdownIt {
        let mut md = MarkdownIt::new();
        markdown_it::plugins::cmark::add(&mut md);
        install(&mut md);
        md
    }

    fn html(source: &str) -> String {
        parser().render(source)
    }

    /// The alert payload and node of a document with exactly one top-level block.
    fn alert(document: &Document) -> (&Alert, NodeId) {
        let container = document.children(document.root())[0];
        (document.value::<Alert>(container).unwrap(), container)
    }

    #[test]
    fn marker_becomes_titled_alert_with_body() {
        assert_eq!(
            html("> [!NOTE]\n> Useful information.\n"),
            "<div class=\"markdown-alert markdown-alert-note\">\n\
             <p class=\"markdown-alert-title\">Note</p>\n\
             <p>Useful information.</p>\n\
             </div>\n"
        );
    }

    #[test]
    fn body_keeps_inline_and_block_content() {
        assert_eq!(
            html("> [!WARNING]\n> keep *calm* and `breathe`\n>\n> second paragraph\n"),
            "<div class=\"markdown-alert markdown-alert-warning\">\n\
             <p class=\"markdown-alert-title\">Warning</p>\n\
             <p>keep <em>calm</em> and <code>breathe</code></p>\n\
             <p>second paragraph</p>\n\
             </div>\n"
        );
    }

    #[test]
    fn marker_only_alert_has_no_empty_paragraph() {
        let document = parser().parse("> [!TIP]\n");
        let (alert, container) = alert(&document);
        assert_eq!(alert.0, AlertKind::Tip);
        assert!(document.children(container).is_empty());
        assert_eq!(
            html("> [!TIP]\n"),
            "<div class=\"markdown-alert markdown-alert-tip\">\n\
             <p class=\"markdown-alert-title\">Tip</p>\n\
             </div>\n"
        );
    }

    #[test]
    fn every_kind_maps_to_its_class_and_title() {
        for kind in AlertKind::ALL {
            let source = format!("> [!{}]\n> body\n", kind.name().to_ascii_uppercase());
            let render = html(&source);
            assert!(
                render.contains(&format!("markdown-alert markdown-alert-{}", kind.name())),
                "{render}"
            );
            assert!(
                render.contains(&format!(
                    "<p class=\"markdown-alert-title\">{}</p>",
                    kind.title()
                )),
                "{render}"
            );
        }
        // GH matches the marker case-insensitively.
        assert!(html("> [!note]\n> body\n").contains("markdown-alert-note"));
    }

    #[test]
    fn non_marker_and_misplaced_markers_stay_blockquotes() {
        // Unknown kind.
        assert_eq!(
            html("> [!DANGER]\n> body\n"),
            "<blockquote>\n<p>[!DANGER]\nbody</p>\n</blockquote>\n"
        );
        // Body on the marker line.
        assert_eq!(
            html("> [!NOTE] body\n"),
            "<blockquote>\n<p>[!NOTE] body</p>\n</blockquote>\n"
        );
        // Marker not on the first line.
        assert_eq!(
            html("> intro\n> [!NOTE]\n"),
            "<blockquote>\n<p>intro\n[!NOTE]</p>\n</blockquote>\n"
        );
        // Ordinary blockquote.
        assert_eq!(
            html("> quoted\n"),
            "<blockquote>\n<p>quoted</p>\n</blockquote>\n"
        );
    }

    #[test]
    fn alerts_nest_inside_lists_and_blockquotes() {
        assert_eq!(
            html("- > [!CAUTION]\n  > nested body\n"),
            "<ul>\n<li>\n<div class=\"markdown-alert markdown-alert-caution\">\n\
             <p class=\"markdown-alert-title\">Caution</p>\n\
             <p>nested body</p>\n\
             </div>\n</li>\n</ul>\n"
        );
    }

    #[test]
    fn multiple_alerts_in_one_document_do_not_share_state() {
        let render = html("> [!NOTE]\n> one\n\n> [!WARNING]\n> two\n");
        assert!(render.contains("markdown-alert-note"));
        assert!(render.contains("markdown-alert-warning"));
        assert_eq!(render.matches("markdown-alert-title").count(), 2);
    }

    #[test]
    fn text_format_keeps_title_and_body() {
        let document = parser().parse("> [!IMPORTANT]\n> act now\n");
        assert_eq!(
            parser().render_document(&document, "text"),
            "Important:\nact now\n"
        );
    }

    #[test]
    fn later_core_rules_observe_the_rewritten_alert() {
        #[derive(Default)]
        struct Observe;
        impl CoreRule for Observe {
            fn run(&self, state: &mut CoreState<'_>) {
                let document = &*state.document;
                let alerts: Vec<_> = document
                    .children(document.root())
                    .iter()
                    .filter(|&&id| document.node(id).is::<Alert>())
                    .copied()
                    .collect();
                assert_eq!(alerts.len(), 1);
                let mut edits = EditBatch::new();
                edits.set_attribute(alerts[0], "data-observed", "yes");
                edits.commit(state.document);
            }
        }

        let mut md = MarkdownIt::new();
        markdown_it::plugins::cmark::add(&mut md);
        // Registered first on purpose: constraints must order it after the transform.
        md.add_rule::<Observe>()
            .after_mark(markdown_it::plugin::transform_plugin_mark("gh-alert"));
        install(&mut md);
        assert!(
            md.render("> [!NOTE]\n> body\n").contains(
                "<div class=\"markdown-alert markdown-alert-note\" data-observed=\"yes\">"
            )
        );
    }

    #[cfg(feature = "sourcemap")]
    #[test]
    fn rewriting_keeps_source_maps_of_container_and_body() {
        let document = parser().parse("> [!NOTE]\n> body\n");
        let container = document.children(document.root())[0];
        let body = document.children(container)[0];
        let offsets = |id| document.node(id).srcmap().unwrap().get_byte_offsets();
        // The blockquote keeps its own span; the body paragraph is trimmed to
        // the surviving text instead of still claiming the marker's line.
        assert_eq!(offsets(container), (0, 16));
        assert_eq!(offsets(body), (12, 16));
        assert_eq!(offsets(document.children(body)[0]), (12, 16));
    }
}
