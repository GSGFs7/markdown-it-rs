//! Add id attribute (slug) to headings.
//!
//! ```rust
//! let md = &mut markdown_it::MarkdownIt::empty();
//! markdown_it::plugins::cmark::add(md);
//! markdown_it::plugins::extra::heading_anchors::add(md);
//!
//! assert_eq!(
//!     md.render("## An example heading"),
//!     "<h2 id=\"an-example-heading\">An example heading</h2>\n",
//! );
//! ```
use std::collections::{HashMap, HashSet};

use crate::document::edit::EditBatch;
use crate::document::transform::DocumentTransform;
use crate::document::{Document, NodeRef};
use crate::parser::inline::{Text, TextSpecial};
use crate::plugins::cmark::block::heading::ATXHeading;
use crate::plugins::cmark::block::lheading::SetextHeader;
use crate::plugins::cmark::inline::newline::Softbreak;
use crate::{MarkdownIt, StructuralEvent};

// --- pub method ---

pub fn add(md: &mut MarkdownIt) {
    add_with_options(md, HeadingAnchorsOptions::default());
}

/// Register heading anchors with per-parser configuration.
pub fn add_with_options(md: &mut MarkdownIt, options: HeadingAnchorsOptions) {
    md.add_document_transform_instance(HeadingAnchorsDocumentTransform::new(options));
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

pub fn is_heading(node: NodeRef<'_>) -> bool {
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

#[derive(Debug, Eq, PartialEq)]
enum HeadingIdEdit {
    Keep,
    Remove,
    Set(String),
}

struct HeadingAnchorState<'a> {
    options: &'a HeadingAnchorsOptions,
    used_ids: HashSet<String>,
    next_suffix: HashMap<String, usize>,
}

impl<'a> HeadingAnchorState<'a> {
    fn new(options: &'a HeadingAnchorsOptions) -> Self {
        Self {
            options,
            used_ids: HashSet::new(),
            next_suffix: HashMap::new(),
        }
    }

    // Reserve IDs already assigned by earlier rules. IDs on headings are
    // excluded when they are going to be overridden.
    fn reserve_ids<'b>(
        &mut self,
        is_heading: bool,
        attrs: impl IntoIterator<Item = &'b (String, String)>,
    ) {
        if is_heading && matches!(self.options.existing_id, ExistingIdPolicy::Override) {
            return;
        }
        self.used_ids.extend(
            attrs
                .into_iter()
                .filter(|(name, _)| name == "id")
                .map(|(_, value)| value.clone()),
        );
    }

    fn edit_for_heading(&mut self, has_id: bool, text: &str) -> HeadingIdEdit {
        if has_id && matches!(self.options.existing_id, ExistingIdPolicy::Keep) {
            return HeadingIdEdit::Keep;
        }

        let mut slug = self.options.strategy.slugify(text);
        if slug.is_empty() {
            match &self.options.empty_slug {
                EmptySlugPolicy::Skip => {
                    return if has_id {
                        HeadingIdEdit::Remove
                    } else {
                        HeadingIdEdit::Keep
                    };
                }
                EmptySlugPolicy::Use(fallback) => slug.clone_from(fallback),
            }
        }
        if slug.is_empty() {
            // if fallback is also empty
            return if has_id {
                HeadingIdEdit::Remove
            } else {
                HeadingIdEdit::Keep
            };
        }
        if let Some(prefix) = &self.options.prefix {
            slug.insert_str(0, prefix);
        }

        HeadingIdEdit::Set(unique_slug(slug, &mut self.used_ids, &mut self.next_suffix))
    }
}

// --- rule ---

/// Arena-backed heading anchors with owned runtime configuration.
#[derive(Debug, Default)]
pub struct HeadingAnchorsDocumentTransform {
    options: HeadingAnchorsOptions,
}

impl HeadingAnchorsDocumentTransform {
    pub fn new(options: HeadingAnchorsOptions) -> Self {
        Self { options }
    }

    pub fn options(&self) -> &HeadingAnchorsOptions {
        &self.options
    }
}

impl DocumentTransform for HeadingAnchorsDocumentTransform {
    const KEY: &'static str = "extra::heading_anchors";

    fn run(&self, document: &Document) -> EditBatch {
        let mut state = HeadingAnchorState::new(&self.options);
        let mut headings = Vec::<DocumentHeading>::new(); // regustration list
        let mut active_headings = Vec::<usize>::new(); // stack
        // Enter(heading)             -> preemption id
        //   Enter(inlineRoot)        -> `append_document_text` do none thing
        //     Leaf(Text "Hello ")    -> append "Hello " to active heading
        //     Enter(emphasis)        -> preemption id
        //       Leaf(Text "world")   -> append "world" (headings[0].text = "Hello world")
        //     Exit(emphasis)
        //   Exit(inlineRoot)
        // Exit(heading)              -> pop active heading
        for event in document.events(document.root()) {
            let node = event.node();
            match event {
                StructuralEvent::Enter(node) => {
                    state.reserve_ids(is_heading(node), node.attrs());
                    if is_heading(node) {
                        active_headings.push(headings.len());
                        headings.push(DocumentHeading::new(node));
                    }
                    append_document_text(node, &active_headings, &mut headings);
                }
                StructuralEvent::Leaf(node) => {
                    state.reserve_ids(is_heading(node), node.attrs());
                    if is_heading(node) {
                        headings.push(DocumentHeading::new(node));
                    } else {
                        append_document_text(node, &active_headings, &mut headings);
                    }
                }
                StructuralEvent::Exit(_) => {
                    if is_heading(node) {
                        let heading = active_headings
                            .pop()
                            .expect("balanced heading events have an active heading");
                        debug_assert_eq!(headings[heading].id, node.id());
                    }
                }
            }
        }

        let mut edits = EditBatch::new();
        for heading in headings {
            match state.edit_for_heading(heading.has_id, &heading.text) {
                HeadingIdEdit::Keep => {}
                HeadingIdEdit::Remove => edits.remove_attribute(heading.id, "id"),
                HeadingIdEdit::Set(slug) => edits.set_attribute(heading.id, "id", slug),
            }
        }
        edits
    }
}

struct DocumentHeading {
    id: crate::NodeId,
    has_id: bool,
    text: String,
}

impl DocumentHeading {
    fn new(node: NodeRef<'_>) -> Self {
        Self {
            id: node.id(),
            has_id: node.attrs().iter().any(|(name, _)| name == "id"),
            text: String::new(),
        }
    }
}

fn append_document_text(
    node: NodeRef<'_>,
    active_headings: &[usize],
    headings: &mut [DocumentHeading],
) {
    let content = if let Some(text) = node.cast::<Text>() {
        Some(text.content.as_str())
    } else if let Some(text) = node.cast::<TextSpecial>() {
        Some(text.content.as_str())
    } else if node.is::<Softbreak>() {
        Some("\n")
    } else {
        None
    };
    if let Some(content) = content {
        for &heading in active_headings {
            headings[heading].text.push_str(content);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::core::{CoreRule, DocumentCoreRule};
    use crate::parser::inline::builtin::InlineParserRule;

    struct AddExistingHeadingId;
    impl CoreRule for AddExistingHeadingId {
        fn document_rule() -> DocumentCoreRule {
            DocumentCoreRule::FinalizeDraft(|root, _| {
                let mut stack = vec![root];
                while let Some(node) = stack.pop() {
                    if (node.is::<ATXHeading>() || node.is::<SetextHeader>())
                        && node.children().iter().any(|child| {
                            child
                                .cast::<Text>()
                                .is_some_and(|text| text.content == "Existing")
                        })
                    {
                        node.attrs_mut().push(("id".into(), "generated".into()));
                    }
                    stack.extend(node.children_mut());
                }
            })
        }
    }

    fn parser(add_existing_id: bool) -> MarkdownIt {
        let mut md = MarkdownIt::empty();
        crate::plugins::cmark::add(&mut md);
        if add_existing_id {
            md.add_rule::<AddExistingHeadingId>()
                .after::<InlineParserRule>();
        }
        md
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
            md.render("# Test\n# Test\n\nTest\n===="),
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
            md.render("# Test\n# Test\n# Test\n# Test-1\n# Test-2\n# Test-1-1"),
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
            md.render("# !!!\n# ???"),
            "<h1 id=\"doc-section\">!!!</h1>\n\
             <h1 id=\"doc-section-1\">???</h1>\n"
        );
    }

    #[test]
    fn test_empty_slug_is_skipped_by_default() {
        let md = &mut crate::MarkdownIt::empty();
        crate::plugins::cmark::add(md);
        add(md);

        assert_eq!(md.render("# !!!"), "<h1>!!!</h1>\n");
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
            md.render("# A&amp;B"),
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
            md.render("# Generated\n# Existing"),
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
            md.render("# Existing"),
            "<h1 id=\"existing\">Existing</h1>\n"
        );
    }

    #[test]
    fn document_transform_matches_expected_output_options_and_text_semantics() {
        fn custom(text: &str) -> String {
            format!("custom-{}", text.to_lowercase())
        }

        let cases = [
            (
                "# Test\n# Test\n\nTest\n====\n# Test-1",
                HeadingAnchorsOptions::default(),
                false,
                "<h1 id=\"test\">Test</h1>\n<h1 id=\"test-1\">Test</h1>\n<h1 id=\"test-2\">Test</h1>\n<h1 id=\"test-1-1\">Test-1</h1>\n",
            ),
            (
                "# !!!\n# ???",
                HeadingAnchorsOptions {
                    empty_slug: EmptySlugPolicy::Use("section".into()),
                    prefix: Some("doc-".into()),
                    ..HeadingAnchorsOptions::default()
                },
                false,
                "<h1 id=\"doc-section\">!!!</h1>\n<h1 id=\"doc-section-1\">???</h1>\n",
            ),
            (
                "# !!!",
                HeadingAnchorsOptions::default(),
                false,
                "<h1>!!!</h1>\n",
            ),
            (
                "# A&amp;B",
                HeadingAnchorsOptions {
                    strategy: SlugStrategy::Custom(custom),
                    ..HeadingAnchorsOptions::default()
                },
                false,
                "<h1 id=\"custom-a&amp;b\">A&amp;B</h1>\n",
            ),
            (
                "A&amp;B\ncontinued\n---",
                HeadingAnchorsOptions::default(),
                false,
                "<h2 id=\"a-b-continued\">A&amp;B\ncontinued</h2>\n",
            ),
            (
                "# Generated\n# Existing",
                HeadingAnchorsOptions::default(),
                true,
                "<h1 id=\"generated-1\">Generated</h1>\n<h1 id=\"generated\">Existing</h1>\n",
            ),
            (
                "# Existing",
                HeadingAnchorsOptions {
                    existing_id: ExistingIdPolicy::Override,
                    ..HeadingAnchorsOptions::default()
                },
                true,
                "<h1 id=\"existing\">Existing</h1>\n",
            ),
        ];

        // Expected HTML captured from the pre-migration legacy parser.
        for (input, options, add_existing_id, expected) in cases {
            let mut md = parser(add_existing_id);
            add_with_options(&mut md, options);
            assert_eq!(md.render(input), expected, "{input:?}");
        }
    }

    #[test]
    fn document_configuration_is_isolated_per_parser() {
        let mut first = parser(false);
        add_with_options(
            &mut first,
            HeadingAnchorsOptions {
                prefix: Some("first-".into()),
                ..HeadingAnchorsOptions::default()
            },
        );
        let mut second = parser(false);
        add_with_options(
            &mut second,
            HeadingAnchorsOptions {
                prefix: Some("second-".into()),
                ..HeadingAnchorsOptions::default()
            },
        );
        let first_document = first.parse_document("# Heading");
        let second_document = second.parse_document("# Heading");

        assert_eq!(
            first.render_document(&first_document),
            "<h1 id=\"first-heading\">Heading</h1>\n"
        );
        assert_eq!(
            second.render_document(&second_document),
            "<h1 id=\"second-heading\">Heading</h1>\n"
        );
    }

    #[test]
    fn registration_populates_document_registry() {
        let mut md = MarkdownIt::empty();
        add(&mut md);

        assert!(
            md.document_transforms
                .contains::<HeadingAnchorsDocumentTransform>()
        );
    }
}
