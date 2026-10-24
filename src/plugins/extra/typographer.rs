//! Common textual replacements for dashes, ©, ™, …
//!
//! This plugin is also enabled by [`crate::plugins::extra::add`]. To enable
//! only typographer, register it directly:
//!
//! ```rust
//! let md = &mut markdown_it::MarkdownIt::empty();
//! markdown_it::plugins::cmark::add(md);
//! markdown_it::plugins::extra::typographer::add(md);
//!
//! let html = md.render("Hello world!.... This is the Right Way(TM) to markdown!!!!!");
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

use const_format::formatcp;
use regex::Regex;

use crate::MarkdownIt;
use crate::document::edit::EditBatch;
use crate::document::{Document, StructuralEvent};
use crate::parser::core::{CoreRule, CoreState};
use crate::plugins::cmark::inline::backticks::CodeInline;
use crate::plugins::cmark::inline::image::Image;

// JavaScript's \s includes BOM and excludes NEL, unlike regex's \s.
const JS_WHITESPACE: &str = r"[\s\u{FEFF}--\u{0085}]";

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
            Regex::new(formatcp!(
                r"(?m)(?P<pre>^|{JS_WHITESPACE})(?P<dash>--)(?P<post>{JS_WHITESPACE}|$)"
            ))
            .unwrap(),
            "$pre\u{2013}$post",
        ),
        (
            Regex::new(formatcp!(
                r"(?m)(?P<pre>^|[^-{JS_WHITESPACE}])(?P<dash>--)(?P<post>[^-{JS_WHITESPACE}]|$)"
            ))
            .unwrap(),
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
    // Alias, not type: custom `add_with` variants share only this alias.
    md.add_rule::<TypographerRule>()
        .after_named("inline")
        .after_named("footnote_tail")
        .before_named("smartquotes");
}

/// Applies typographic replacements.
#[derive(Default)]
pub struct TypographerRule;

impl CoreRule for TypographerRule {
    const NAMES: &'static [&'static str] = &["extra::typographer"];

    fn run(&self, state: &mut CoreState<'_>) {
        let document = &mut *state.document;
        let edits = edits(document);
        edits.commit(document);
    }
}

fn edits(document: &Document) -> EditBatch {
    let mut edits = EditBatch::new();
    let mut protected_depth = 0;
    for event in document.events(document.root()) {
        let node = match event {
            StructuralEvent::Enter(node) if node.is::<CodeInline>() || node.is::<Image>() => {
                protected_depth += 1;
                continue;
            }
            StructuralEvent::Exit(node) if node.is::<CodeInline>() || node.is::<Image>() => {
                protected_depth -= 1;
                continue;
            }
            StructuralEvent::Enter(node) | StructuralEvent::Leaf(node) => node,
            StructuralEvent::Exit(_) => continue,
        };

        if protected_depth > 0 {
            continue;
        }
        let Some(text) = document.text(node.id()) else {
            continue;
        };
        if let Some(replacement) = replace_text(text) {
            edits.replace_text(node.id(), 0..text.len(), replacement);
        }
    }
    edits
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
