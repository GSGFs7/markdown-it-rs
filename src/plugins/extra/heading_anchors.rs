//! Add id attribute (slug) to headings.
//!
//! ```rust
//! let md = &mut markdown_it::MarkdownIt::empty();
//! markdown_it::plugins::cmark::add(md);
//! markdown_it::plugins::extra::heading_anchors::add(md);
//!
//! assert_eq!(
//!     md.parse("## An example heading").render(),
//!     "<h2 id=\"an-example-heading\">An example heading</h2>\n",
//! );
//! ```
use std::collections::{HashMap, HashSet};

use crate::parser::core::CoreRule;
use crate::parser::inline::builtin::InlineParserRule;
use crate::plugins::cmark::block::heading::ATXHeading;
use crate::plugins::cmark::block::lheading::SetextHeader;
use crate::{MarkdownIt, Node};

// --- pub method ---

pub fn add(md: &mut MarkdownIt) {
    add_with_options(md, HeadingAnchorsOptions::default());
}

pub fn add_with_options(md: &mut MarkdownIt, options: HeadingAnchorsOptions) {
    md.ext.insert(options);
    md.add_rule::<AddHeadingAnchors>()
        .after::<InlineParserRule>();
}

// --- config ---

#[derive(Clone, Copy, Debug, Default)]
pub enum SlugStrategy {
    #[default]
    Simple,
    GitHub,
    Custom(fn(&str) -> String),
}

impl SlugStrategy {
    fn slugify(self, text: &str) -> String {
        match self {
            Self::Simple => simple_slugify_fn(text),
            Self::GitHub => github_slugify_fn(text),
            Self::Custom(slugify) => slugify(text),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub enum ExistingIdPolicy {
    #[default]
    Keep,
    Override,
}

/// how to process empty slug. such as "# !!!"
#[derive(Clone, Debug, Default)]
pub enum EmptySlugPolicy {
    /// not add id attr
    #[default]
    Skip,
    /// use a string as a default slug. such as "section"
    Use(String),
}

#[derive(Clone, Debug, Default)]
pub struct HeadingAnchorsOptions {
    pub strategy: SlugStrategy,
    pub existing_id: ExistingIdPolicy,
    pub empty_slug: EmptySlugPolicy,
    /// add a prefix? if set "doc-": "# hello" -> `id="doc-hello"`
    pub prefix: Option<String>,
}

// --- slugify function ---

/// Convert every run of non-alphanumeric characters to a single hyphen.
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

/// Generate slugs using GitHub-style heading rules.
///
/// Most punctuation, symbols, and control characters are removed. ASCII
/// spaces become hyphens, while existing hyphens and connector punctuation
/// such as underscores are preserved.
pub fn github_slugify_fn(s: &str) -> String {
    use unicode_general_category::GeneralCategory::*;
    use unicode_general_category::get_general_category;

    let mut result = String::with_capacity(s.len());

    for c in s.chars() {
        if c == ' ' {
            result.push('-');
        } else if c == '-'
            || c.is_alphabetic()
            || matches!(
                get_general_category(c),
                UppercaseLetter
                    | LowercaseLetter
                    | TitlecaseLetter
                    | ModifierLetter
                    | OtherLetter
                    | NonspacingMark
                    | SpacingMark
                    | EnclosingMark
                    | DecimalNumber
                    | LetterNumber
                    | ConnectorPunctuation
            )
        {
            // UTF-8 may has many bytes
            result.extend(c.to_lowercase());
        }
    }

    result
}

// --- helper method ---

pub fn is_heading(node: &Node) -> bool {
    node.is::<ATXHeading>() || node.is::<SetextHeader>()
}

pub fn unique_slug(
    slug: String,
    used_ids: &mut HashSet<String>,
    next_suffix: &mut HashMap<String, usize>,
) -> String {
    if used_ids.insert(slug.clone()) {
        return slug;
    }

    // get the usize
    let suffix = next_suffix.entry(slug.clone()).or_insert(1);
    loop {
        let candidate = format!("{slug}-{suffix}");
        *suffix += 1;
        if used_ids.insert(candidate.clone()) {
            return candidate;
        }
    }
}

// --- rule ---

pub struct AddHeadingAnchors;
impl CoreRule for AddHeadingAnchors {
    const NAMES: &'static [&'static str] = &["heading_anchors", "heading-anchors"];

    fn run(root: &mut Node, md: &MarkdownIt) {
        let options = md
            .ext
            .get::<HeadingAnchorsOptions>()
            .expect("heading anchor options must be registered with the rule");

        // Reserve IDs already assigned by earlier rules. IDs on headings are
        // excluded when they are going to be overridden.
        let mut used_ids = HashSet::new();
        root.walk(|node, _| {
            if is_heading(node) && matches!(options.existing_id, ExistingIdPolicy::Override) {
                return;
            }

            used_ids.extend(
                node.attrs
                    .iter()
                    .filter(|(name, _)| *name == "id")
                    .map(|(_, value)| value.clone()),
            );
        });
        let mut next_suffix = HashMap::new();

        root.walk_mut(|node, _| {
            if !is_heading(node) {
                return;
            }

            if node.attrs.iter().any(|(name, _)| *name == "id") {
                match options.existing_id {
                    ExistingIdPolicy::Keep => return,
                    ExistingIdPolicy::Override => {
                        node.attrs.retain(|(name, _)| *name != "id");
                    }
                }
            }

            let text = node.collect_text();
            let mut slug = options.strategy.slugify(&text);
            if slug.is_empty() {
                match &options.empty_slug {
                    EmptySlugPolicy::Skip => return,
                    EmptySlugPolicy::Use(fallback) => slug.clone_from(fallback),
                }
            }
            if slug.is_empty() {
                return;
            }
            if let Some(prefix) = &options.prefix {
                slug.insert_str(0, prefix);
            }

            let slug = unique_slug(slug, &mut used_ids, &mut next_suffix);
            node.attrs.push(("id".into(), slug));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AddExistingHeadingId;
    impl CoreRule for AddExistingHeadingId {
        fn run(root: &mut Node, _: &MarkdownIt) {
            root.walk_mut(|node, _| {
                if is_heading(node) && node.collect_text() == "Existing" {
                    node.attrs.push(("id".into(), "generated".into()));
                }
            });
        }
    }

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
    fn test_github_slugify() {
        assert_eq!(github_slugify_fn("Hello,  World!"), "hello--world");
        assert_eq!(
            github_slugify_fn("under_score-and-hyphen"),
            "under_score-and-hyphen"
        );
        assert_eq!(github_slugify_fn("你好，world 💃"), "你好world-");
        assert_eq!(github_slugify_fn("a\tb\u{a0}c"), "abc");
        assert_eq!(github_slugify_fn("a‿b ²"), "a‿b-");
    }

    #[test]
    fn test_atx_and_setext_heading_uniqueness() {
        let md = &mut crate::MarkdownIt::empty();
        crate::plugins::cmark::add(md);
        add(md);

        assert_eq!(
            md.parse("# Test\n# Test\n\nTest\n====").render(),
            "<h1 id=\"test\">Test</h1>\n\
             <h1 id=\"test-1\">Test</h1>\n\
             <h1 id=\"test-2\">Test</h1>\n"
        );
    }

    #[test]
    fn test_uniqueness_handles_slug_suffix_collisions() {
        let md = &mut crate::MarkdownIt::empty();
        crate::plugins::cmark::add(md);
        add(md);

        assert_eq!(
            md.parse("# Test\n# Test\n# Test\n# Test-1\n# Test-2\n# Test-1-1")
                .render(),
            "<h1 id=\"test\">Test</h1>\n\
             <h1 id=\"test-1\">Test</h1>\n\
             <h1 id=\"test-2\">Test</h1>\n\
             <h1 id=\"test-1-1\">Test-1</h1>\n\
             <h1 id=\"test-2-1\">Test-2</h1>\n\
             <h1 id=\"test-1-1-1\">Test-1-1</h1>\n"
        );
    }

    #[test]
    fn test_empty_slug_fallback_and_prefix() {
        let md = &mut crate::MarkdownIt::empty();
        crate::plugins::cmark::add(md);
        add_with_options(
            md,
            HeadingAnchorsOptions {
                empty_slug: EmptySlugPolicy::Use("section".into()),
                prefix: Some("doc-".into()),
                ..HeadingAnchorsOptions::default()
            },
        );

        assert_eq!(
            md.parse("# !!!\n# ???").render(),
            "<h1 id=\"doc-section\">!!!</h1>\n\
             <h1 id=\"doc-section-1\">???</h1>\n"
        );
    }

    #[test]
    fn test_empty_slug_is_skipped_by_default() {
        let md = &mut crate::MarkdownIt::empty();
        crate::plugins::cmark::add(md);
        add(md);

        assert_eq!(md.parse("# !!!").render(), "<h1>!!!</h1>\n");
    }

    #[test]
    fn test_custom_slug_strategy_and_special_text() {
        fn custom(text: &str) -> String {
            format!("custom-{}", text.to_lowercase())
        }

        let md = &mut crate::MarkdownIt::empty();
        crate::plugins::cmark::add(md);
        add_with_options(
            md,
            HeadingAnchorsOptions {
                strategy: SlugStrategy::Custom(custom),
                ..HeadingAnchorsOptions::default()
            },
        );

        assert_eq!(
            md.parse("# A&amp;B").render(),
            "<h1 id=\"custom-a&amp;b\">A&amp;B</h1>\n"
        );
    }

    #[test]
    fn test_existing_id_is_kept_and_reserved() {
        let md = &mut crate::MarkdownIt::empty();
        crate::plugins::cmark::add(md);
        md.add_rule::<AddExistingHeadingId>()
            .after::<InlineParserRule>();
        add(md);

        assert_eq!(
            md.parse("# Generated\n# Existing").render(),
            "<h1 id=\"generated-1\">Generated</h1>\n\
             <h1 id=\"generated\">Existing</h1>\n"
        );
    }

    #[test]
    fn test_existing_id_is_overridden() {
        let md = &mut crate::MarkdownIt::empty();
        crate::plugins::cmark::add(md);
        md.add_rule::<AddExistingHeadingId>()
            .after::<InlineParserRule>();
        add_with_options(
            md,
            HeadingAnchorsOptions {
                existing_id: ExistingIdPolicy::Override,
                ..HeadingAnchorsOptions::default()
            },
        );

        assert_eq!(
            md.parse("# Existing").render(),
            "<h1 id=\"existing\">Existing</h1>\n"
        );
    }
}
