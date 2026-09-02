//! Common textual replacements for dashes, ©, ™, …
//!
//! This plugin is also enabled by [`crate::plugins::extra::add`]. To enable
//! only typographer on the legacy tree pipeline, register it directly:
//!
//! ```rust
//! let md = &mut markdown_it::MarkdownIt::empty();
//! markdown_it::plugins::cmark::add(md);
//! markdown_it::plugins::extra::typographer::add(md);
//!
//! let html = md.parse("Hello world!.... This is the Right Way(TM) to markdown!!!!!").render();
//! assert_eq!(html.trim(), r#"<p>Hello world!.. This is the Right Way™ to markdown!!!</p>"#);
//! ```
//! In summary, these are the replacements that will be made when using this:
//!
//! ## Typography
//!
//! - Repeated dots (`...`) to ellipsis (`…`)
//!   except `?...` and `!...` which become `?..` and `!..` respectively
//! - `+-` to `±`
//! - Don't repeat `?` and `!` more than 3 times: `???`
//! - De-duplicate commas
//! - em and en dashes: `--` to `–` and `---` to `—`
//!
//! ## Common symbols (case insensitive)
//!
//! - Copyright: `(c)` to `©`
//! - Reserved: `(r)` to `®`
//! - Trademark: `(tm)` to `™`

use std::borrow::Cow;
use std::sync::LazyLock;

use regex::Regex;

use crate::parser::core::CoreRule;
use crate::parser::document::{Document, StructuralEvent};
use crate::parser::document_edit::EditBatch;
use crate::parser::document_transform::DocumentTransform;
use crate::parser::inline::Text;
use crate::parser::inline::builtin::InlineParserRule;
use crate::parser::main::MarkdownIt;
use crate::parser::node::Node;

static REPLACEMENTS: LazyLock<Box<[(Regex, &'static str)]>> = LazyLock::new(|| {
    Box::new([
        (Regex::new(r"\+-").unwrap(), "±"),
        (Regex::new(r"\.{2,}").unwrap(), "…"),
        (Regex::new(r"([?!])…").unwrap(), "$1.."),
        (Regex::new(r"([?!]){4,}").unwrap(), "$1$1$1"),
        (Regex::new(r",{2,}").unwrap(), ","),
        // These look a little different from the JS implementation because the
        // regex crate doesn't support look-behind and look-ahead patterns
        (
            Regex::new(r"(?m)(?P<pre>^|[^-])(?P<dash>---)(?P<post>[^-]|$)").unwrap(),
            "$pre\u{2014}$post",
        ),
        (
            Regex::new(r"(?m)(?P<pre>^|\s)(?P<dash>--)(?P<post>\s|$)").unwrap(),
            "$pre\u{2013}$post",
        ),
        (
            Regex::new(r"(?m)(?P<pre>^|[^-\s])(?P<dash>--)(?P<post>[^-\s]|$)").unwrap(),
            "$pre\u{2013}$post",
        ),
    ])
});
static SCOPED_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\((c|tm|r)\)").unwrap());
static RARE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\+-|\.\.|\?\?\?\?|!!!!|,,|--").unwrap());

fn replace_abbreviation(input: &str) -> &'static str {
    match input.to_lowercase().as_str() {
        "(c)" => "©",
        "(r)" => "®",
        "(tm)" => "™",
        _ => unreachable!("Got invalid abbreviation '{}'", input),
    }
}

pub fn add(md: &mut MarkdownIt) {
    md.add_rule::<TypographerRule>().after::<InlineParserRule>();
}

/// Register typographer for an explicit arena-backed document pipeline.
pub fn add_document(md: &mut MarkdownIt) {
    md.add_document_transform::<TypographerDocumentTransform>()
        .before::<super::smartquotes::ClassicSmartQuotesDocumentTransform>();
}

/// Arena-backed typographer transform.
pub struct TypographerDocumentTransform;

impl DocumentTransform for TypographerDocumentTransform {
    const KEY: &'static str = "extra::typographer";

    fn run(document: &Document) -> EditBatch {
        let mut edits = EditBatch::new();
        for event in document
            .events(document.root())
            .expect("root is always valid")
        {
            let node = match event {
                StructuralEvent::Enter(node) | StructuralEvent::Leaf(node) => node,
                StructuralEvent::Exit(_) => continue,
            };
            let Some(text) = node.cast::<Text>() else {
                continue;
            };
            if let Some(replacement) = replace_text(&text.content) {
                edits.replace_text(node.id(), 0..text.content.len(), replacement);
            }
        }
        edits
    }
}

pub struct TypographerRule;

impl CoreRule for TypographerRule {
    const NAMES: &'static [&'static str] = &["typographer"];

    fn run(root: &mut Node, _: &MarkdownIt) {
        root.walk_mut(|node, _| {
            let Some(text_node) = node.cast_mut::<Text>() else {
                return;
            };
            if let Some(replacement) = replace_text(&text_node.content) {
                text_node.content = replacement;
            }
        });
    }
}

fn replace_text(input: &str) -> Option<String> {
    let mut result = if SCOPED_RE.is_match(input) {
        Cow::Owned(
            SCOPED_RE
                .replace_all(input, |caps: &regex::Captures| {
                    replace_abbreviation(caps.get(0).unwrap().as_str()).to_owned()
                })
                .into_owned(),
        )
    } else {
        Cow::Borrowed(input)
    };

    if RARE_RE.is_match(&result) {
        for (pattern, replacement) in REPLACEMENTS.iter() {
            if let Cow::Owned(s) = pattern.replace_all(&result, *replacement) {
                result = Cow::Owned(s);

                // Dash replacements include their surrounding characters,
                // so adjacent matches can overlap.
                if let Cow::Owned(s) = pattern.replace_all(&result, *replacement) {
                    result = Cow::Owned(s);
                }
            }
        }
    }

    match result {
        Cow::Borrowed(_) => None,
        Cow::Owned(value) => Some(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::extra::smartquotes::ClassicSmartQuotesDocumentTransform;

    fn text_contents(document: &Document) -> String {
        document
            .events(document.root())
            .unwrap()
            .filter_map(|event| match event {
                StructuralEvent::Enter(node) | StructuralEvent::Leaf(node) => {
                    node.cast::<Text>().map(|text| text.content.as_str())
                }
                StructuralEvent::Exit(_) => None,
            })
            .collect()
    }

    #[test]
    fn document_registration_is_explicit() {
        let md = &mut MarkdownIt::empty();
        crate::plugins::cmark::add(md);
        add_document(md);
        assert!(!md.has_rule::<TypographerRule>());
        let mut document = md.parse_document("雪... (TM)");

        assert_eq!(text_contents(&document), "雪... (TM)");
        md.run_document_transforms(&mut document).unwrap();
        assert_eq!(document.into_legacy().render(), "<p>雪… ™</p>\n");
    }

    struct ObserveBetweenTransforms;

    impl DocumentTransform for ObserveBetweenTransforms {
        const KEY: &'static str = "test::observe-between-typographer-and-smartquotes";

        fn run(document: &Document) -> EditBatch {
            let mut edits = EditBatch::new();
            edits.set_attribute(
                document.root(),
                "observed-order",
                (text_contents(document) == "\"…\"").to_string(),
            );
            edits
        }
    }

    #[test]
    fn typographer_runs_before_smartquotes_when_registered_last() {
        let md = &mut MarkdownIt::empty();
        crate::plugins::cmark::add(md);
        super::super::smartquotes::add_document(md);
        md.add_document_transform::<ObserveBetweenTransforms>()
            .after::<TypographerDocumentTransform>()
            .before::<ClassicSmartQuotesDocumentTransform>();
        add_document(md);
        let mut document = md.parse_document(r#""...""#);

        md.run_document_transforms(&mut document).unwrap();

        assert!(
            document
                .node(document.root())
                .unwrap()
                .attrs()
                .iter()
                .any(|(name, value)| name == "observed-order" && value == "true")
        );
        assert_eq!(document.into_legacy().render(), "<p>“…”</p>\n");
    }
}
