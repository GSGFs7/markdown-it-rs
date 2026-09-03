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
use crate::parser::document::{Document, NodeRef};
use crate::parser::document_edit::EditBatch;
use crate::parser::document_transform::DocumentTransform;
use crate::parser::inline::builtin::InlineParserRule;
use crate::parser::inline::{Text, TextSpecial};
use crate::plugins::cmark::block::heading::ATXHeading;
use crate::plugins::cmark::block::lheading::SetextHeader;
use crate::plugins::cmark::inline::newline::Softbreak;
use crate::{MarkdownIt, Node, StructuralEvent};

// --- pub method ---

pub fn add(md: &mut MarkdownIt) {
    add_with_options(md, HeadingAnchorsOptions::default());
}

pub fn add_with_options(md: &mut MarkdownIt, options: HeadingAnchorsOptions) {
    md.ext.insert(options);
    md.add_rule::<AddHeadingAnchors>()
        .after::<InlineParserRule>();
}

/// Register heading anchors for an explicit arena-backed document pipeline.
pub fn add_document(md: &mut MarkdownIt) {
    add_document_with_options(md, HeadingAnchorsOptions::default());
}

/// Register heading anchors with per-parser runtime configuration for an
/// explicit arena-backed document pipeline.
pub fn add_document_with_options(md: &mut MarkdownIt, options: HeadingAnchorsOptions) {
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

// --- helper method ---

pub fn is_heading(node: &Node) -> bool {
    node.is::<ATXHeading>() || node.is::<SetextHeader>()
}

fn is_document_heading(node: NodeRef<'_>) -> bool {
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

    // collect ids from AST
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
        let mut state = HeadingAnchorState::new(options);
        root.walk(|node, _| {
            state.reserve_ids(is_heading(node), &node.attrs);
        });

        root.walk_mut(|node, _| {
            if !is_heading(node) {
                return;
            }

            let text = node.collect_text();
            let has_id = node.attrs.iter().any(|(name, _)| name == "id");
            match state.edit_for_heading(has_id, &text) {
                HeadingIdEdit::Keep => {}
                HeadingIdEdit::Remove => node.attrs.retain(|(name, _)| name != "id"),
                HeadingIdEdit::Set(slug) => {
                    node.attrs.retain(|(name, _)| name != "id");
                    node.attrs.push(("id".into(), slug));
                }
            }
        });
    }
}

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
        for event in document
            .events(document.root())
            .expect("root is always valid")
        {
            let node = event.node();
            match event {
                StructuralEvent::Enter(node) => {
                    state.reserve_ids(is_document_heading(node), node.attrs());
                    if is_document_heading(node) {
                        active_headings.push(headings.len());
                        headings.push(DocumentHeading::new(node));
                    }
                    append_document_text(node, &active_headings, &mut headings);
                }
                StructuralEvent::Leaf(node) => {
                    state.reserve_ids(is_document_heading(node), node.attrs());
                    if is_document_heading(node) {
                        headings.push(DocumentHeading::new(node));
                    } else {
                        append_document_text(node, &active_headings, &mut headings);
                    }
                }
                StructuralEvent::Exit(_) => {
                    if is_document_heading(node) {
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

    fn parser(add_existing_id: bool) -> MarkdownIt {
        let mut md = MarkdownIt::empty();
        crate::plugins::cmark::add(&mut md);
        if add_existing_id {
            md.add_rule::<AddExistingHeadingId>()
                .after::<InlineParserRule>();
        }
        md
    }

    fn render_both(
        input: &str,
        options: HeadingAnchorsOptions,
        add_existing_id: bool,
    ) -> (String, String) {
        let mut legacy = parser(add_existing_id);
        add_with_options(&mut legacy, options.clone());
        let legacy_html = legacy.parse(input).render();

        let document_parser = parser(add_existing_id);
        let mut transforms = MarkdownIt::empty();
        add_document_with_options(&mut transforms, options);
        let mut document = document_parser.parse_document(input);
        transforms.run_document_transforms(&mut document).unwrap();
        let document_html = document.into_legacy().render();

        (legacy_html, document_html)
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

    #[test]
    fn document_transform_matches_legacy_options_and_text_semantics() {
        fn custom(text: &str) -> String {
            format!("custom-{}", text.to_lowercase())
        }

        let cases = [
            (
                "# Test\n# Test\n\nTest\n====\n# Test-1",
                HeadingAnchorsOptions::default(),
                false,
            ),
            (
                "# !!!\n# ???",
                HeadingAnchorsOptions {
                    empty_slug: EmptySlugPolicy::Use("section".into()),
                    prefix: Some("doc-".into()),
                    ..HeadingAnchorsOptions::default()
                },
                false,
            ),
            ("# !!!", HeadingAnchorsOptions::default(), false),
            (
                "# A&amp;B",
                HeadingAnchorsOptions {
                    strategy: SlugStrategy::Custom(custom),
                    ..HeadingAnchorsOptions::default()
                },
                false,
            ),
            (
                "A&amp;B\ncontinued\n---",
                HeadingAnchorsOptions::default(),
                false,
            ),
            (
                "# Generated\n# Existing",
                HeadingAnchorsOptions::default(),
                true,
            ),
            (
                "# Existing",
                HeadingAnchorsOptions {
                    existing_id: ExistingIdPolicy::Override,
                    ..HeadingAnchorsOptions::default()
                },
                true,
            ),
        ];

        for (input, options, add_existing_id) in cases {
            let (legacy, document) = render_both(input, options, add_existing_id);
            assert_eq!(document, legacy, "{input:?}");
        }
    }

    #[test]
    fn document_configuration_is_isolated_per_parser() {
        let mut first = MarkdownIt::empty();
        add_document_with_options(
            &mut first,
            HeadingAnchorsOptions {
                prefix: Some("first-".into()),
                ..HeadingAnchorsOptions::default()
            },
        );
        let mut second = MarkdownIt::empty();
        add_document_with_options(
            &mut second,
            HeadingAnchorsOptions {
                prefix: Some("second-".into()),
                ..HeadingAnchorsOptions::default()
            },
        );
        let parser = parser(false);
        let mut first_document = parser.parse_document("# Heading");
        let mut second_document = parser.parse_document("# Heading");

        assert!(
            first_document
                .events(first_document.root())
                .unwrap()
                .filter(|event| is_document_heading(event.node()))
                .all(|event| event.node().attrs().iter().all(|(name, _)| name != "id"))
        );

        first.run_document_transforms(&mut first_document).unwrap();
        second
            .run_document_transforms(&mut second_document)
            .unwrap();

        assert_eq!(
            first_document.into_legacy().render(),
            "<h1 id=\"first-heading\">Heading</h1>\n"
        );
        assert_eq!(
            second_document.into_legacy().render(),
            "<h1 id=\"second-heading\">Heading</h1>\n"
        );
    }

    #[test]
    fn legacy_registration_does_not_populate_document_registry() {
        let mut md = MarkdownIt::empty();
        add(&mut md);

        assert!(
            !md.document_transforms
                .contains::<HeadingAnchorsDocumentTransform>()
        );
    }
}
