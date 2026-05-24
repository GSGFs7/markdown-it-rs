//! Add id attribute (slug) to headings.
//!
//! ```rust
//! // it is recommended to use 3rd party slug implementation
//! //let slugify_fn = |s: &str| slug::slugify(s);
//! let slugify_fn = markdown_it::plugins::extra::heading_anchors::simple_slugify_fn;
//!
//! let md = &mut markdown_it::MarkdownIt::new();
//! markdown_it::plugins::cmark::add(md);
//! markdown_it::plugins::extra::heading_anchors::add(md, slugify_fn);
//!
//! assert_eq!(
//!     md.parse("## An example heading").render(),
//!     "<h2 id=\"an-example-heading\">An example heading</h2>\n",
//! );
//! ```
use std::collections::HashMap;
use std::fmt::Debug;

use crate::parser::core::CoreRule;
use crate::parser::extset::MarkdownItExt;
use crate::plugins::cmark::block::heading::ATXHeading;
use crate::plugins::cmark::block::lheading::SetextHeader;
use crate::{MarkdownIt, Node};

pub fn add(md: &mut MarkdownIt, slugify: fn(&str) -> String) {
    md.ext.insert(SlugifyFunction(slugify));
    md.add_rule::<AddHeadingAnchors>();
}

/// Simple built-in slugify function. It is added for testing and demonstration
/// purposes only, you should be using `slug`/`slugify` crate instead or your own impl.
pub fn simple_slugify_fn(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut last_was_hyphen = true;

    for c in s.chars() {
        if c.is_alphanumeric() {
            for lower in c.to_lowercase() {
                result.push(lower);
            }
            last_was_hyphen = false;
        } else if !last_was_hyphen {
            result.push('-');
            last_was_hyphen = true;
        }
    }

    if last_was_hyphen && !result.is_empty() {
        result.pop();
    }

    result
}

#[derive(Clone, Copy)]
struct SlugifyFunction(fn(&str) -> String);
impl MarkdownItExt for SlugifyFunction {}

impl Default for SlugifyFunction {
    fn default() -> Self {
        Self(simple_slugify_fn)
    }
}

impl Debug for SlugifyFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlugifyFunction").finish()
    }
}

pub struct AddHeadingAnchors;
impl CoreRule for AddHeadingAnchors {
    const NAMES: &'static [&'static str] = &["heading_anchors", "heading-anchors"];

    fn run(root: &mut Node, md: &MarkdownIt) {
        let slugify = md
            .ext
            .get::<SlugifyFunction>()
            .copied()
            .unwrap_or_default()
            .0;

        let mut used_ids: HashMap<String, u8> = HashMap::new();

        root.walk_mut(|node, _| {
            if node.is::<ATXHeading>() || node.is::<SetextHeader>() {
                let text = node.collect_text();
                let mut slug = slugify(&text);

                // uniqueness check
                if let Some(current_id) = used_ids.get(&slug) {
                    let original_slug = slug.clone();
                    slug = format!("{}-{}", original_slug, current_id + 1);
                    used_ids.insert(original_slug, current_id + 1);
                } else {
                    used_ids.insert(slug.clone(), 0);
                }

                node.attrs.push(("id", slug));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_slugify() {
        assert_eq!(simple_slugify_fn("Hello World!"), "hello-world");
        assert_eq!(simple_slugify_fn("Multiple   Spaces"), "multiple-spaces");
        assert_eq!(simple_slugify_fn("Äpfel"), "äpfel");
        assert_eq!(
            simple_slugify_fn("  leading and trailing  "),
            "leading-and-trailing"
        );
        assert_eq!(simple_slugify_fn("!!special--chars??"), "special-chars");
        assert_eq!(simple_slugify_fn("你好 world！"), "你好-world");
    }

    #[test]
    fn test_uniqueness() {
        let md = &mut crate::MarkdownIt::new();
        crate::plugins::cmark::add(md);
        add(md, simple_slugify_fn);

        let html = md.parse("# Test\n# Test\n# Test").render();
        assert!(html.contains("id=\"test\""));
        assert!(html.contains("id=\"test-1\""));
        assert!(html.contains("id=\"test-2\""));
    }
}
