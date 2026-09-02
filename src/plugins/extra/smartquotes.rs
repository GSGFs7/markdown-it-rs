//! Replaces `"` and `'` quotes with typographic quotation marks.
//!
//! The implementation consumes a lazy text projection of the AST and records
//! byte-based text edits. The tree is mutated only after the complete edit
//! batch has been validated.

use std::collections::HashMap;

use crate::common::utils::is_punct_char;
use crate::parser::core::CoreRule;
use crate::parser::inline::Text;
use crate::parser::inline::builtin::InlineParserRule;
use crate::parser::main::MarkdownIt;
use crate::parser::node::Node;
use crate::parser::text::{
    TextBoundary as LegacyTextBoundary,
    TextEditBatch,
    TextEvent as LegacyTextEvent,
    TextNodeKey,
    TextProjection as LegacyTextProjection,
    TextProjectionKind as LegacyTextProjectionKind,
};
use crate::plugins::cmark::block::paragraph::Paragraph;
use crate::plugins::cmark::inline::newline::{Hardbreak, Softbreak};
use crate::plugins::html::html_inline::HtmlInline;
use crate::{
    Document,
    DocumentTransform,
    EditBatch,
    NodeId,
    NodeRef,
    TextBoundary,
    TextEvent,
    TextProjection,
    TextProjectionKind,
};

const APOSTROPHE: char = '\u{2019}';
const SINGLE_QUOTE: char = '\'';
const DOUBLE_QUOTE: char = '"';
const SPACE: char = ' ';

/// Add smartquotes with the "classic" quote set of `‘`, `’`, `“`, and `”`.
pub fn add(md: &mut MarkdownIt) {
    add_with::<'‘', '’', '“', '”'>(md);
}

pub fn add_with<
    const OPEN_SINGLE_QUOTE: char,
    const CLOSE_SINGLE_QUOTE: char,
    const OPEN_DOUBLE_QUOTE: char,
    const CLOSE_DOUBLE_QUOTE: char,
>(
    md: &mut MarkdownIt,
) {
    md.add_rule::<SmartQuotesRule<
        OPEN_SINGLE_QUOTE,
        CLOSE_SINGLE_QUOTE,
        OPEN_DOUBLE_QUOTE,
        CLOSE_DOUBLE_QUOTE,
    >>()
    .after::<InlineParserRule>();
}

/// Register the classic smartquotes transform for explicit arena-backed
/// document pipelines.
///
/// Unlike [`add`], this does not register the legacy core rule, and
/// [`MarkdownIt::parse_document`] does not run it automatically. Call
/// [`MarkdownIt::run_document_transforms`] after parsing.
pub fn add_document(md: &mut MarkdownIt) {
    add_document_with::<'‘', '’', '“', '”'>(md);
}

/// Register a custom smartquotes transform for explicit arena-backed
/// document pipelines.
pub fn add_document_with<
    const OPEN_SINGLE_QUOTE: char,
    const CLOSE_SINGLE_QUOTE: char,
    const OPEN_DOUBLE_QUOTE: char,
    const CLOSE_DOUBLE_QUOTE: char,
>(
    md: &mut MarkdownIt,
) {
    md.add_document_transform::<SmartQuotesDocumentTransform<
        OPEN_SINGLE_QUOTE,
        CLOSE_SINGLE_QUOTE,
        OPEN_DOUBLE_QUOTE,
        CLOSE_DOUBLE_QUOTE,
    >>();
}

/// The classic smartquotes transform used by [`add_document`].
pub type ClassicSmartQuotesDocumentTransform = SmartQuotesDocumentTransform<'‘', '’', '“', '”'>;

/// Arena-backed smartquotes with a compile-time quote set.
#[derive(Default)]
pub struct SmartQuotesDocumentTransform<
    const OPEN_SINGLE_QUOTE: char,
    const CLOSE_SINGLE_QUOTE: char,
    const OPEN_DOUBLE_QUOTE: char,
    const CLOSE_DOUBLE_QUOTE: char,
>;

impl<
    const OPEN_SINGLE_QUOTE: char,
    const CLOSE_SINGLE_QUOTE: char,
    const OPEN_DOUBLE_QUOTE: char,
    const CLOSE_DOUBLE_QUOTE: char,
> DocumentTransform
    for SmartQuotesDocumentTransform<
        OPEN_SINGLE_QUOTE,
        CLOSE_SINGLE_QUOTE,
        OPEN_DOUBLE_QUOTE,
        CLOSE_DOUBLE_QUOTE,
    >
{
    const KEY: &'static str = "extra::smartquotes";

    fn run(&self, document: &Document) -> EditBatch {
        document_edits_with::<
            OPEN_SINGLE_QUOTE,
            CLOSE_SINGLE_QUOTE,
            OPEN_DOUBLE_QUOTE,
            CLOSE_DOUBLE_QUOTE,
        >(document)
    }
}

fn document_edits_with<
    const OPEN_SINGLE_QUOTE: char,
    const CLOSE_SINGLE_QUOTE: char,
    const OPEN_DOUBLE_QUOTE: char,
    const CLOSE_DOUBLE_QUOTE: char,
>(
    document: &Document,
) -> EditBatch {
    let mut edits = EditBatch::new();
    let projection = TextProjection::new(document_smartquotes_projection);
    let events = document
        .text_events(projection)
        .filter_map(document_relevant_event);
    transform_events::<
        NodeId,
        _,
        _,
        OPEN_SINGLE_QUOTE,
        CLOSE_SINGLE_QUOTE,
        OPEN_DOUBLE_QUOTE,
        CLOSE_DOUBLE_QUOTE,
    >(events, |node, byte_offset, replacement| {
        edits.replace_char(node, byte_offset..byte_offset + 1, replacement);
    });
    edits
}

#[derive(PartialEq, Eq, Debug, Clone, Copy, Hash)]
enum QuoteType {
    Single,
    Double,
}

#[derive(Clone, Copy)]
struct QuoteMarker<K> {
    node: K,
    byte_offset: usize,
    quote_type: QuoteType,
    level: u32,
}

struct QuoteStack<K> {
    markers: Vec<QuoteMarker<K>>,
    positions: HashMap<(u32, QuoteType), Vec<usize>>,
}

impl<K> Default for QuoteStack<K> {
    fn default() -> Self {
        Self {
            markers: Vec::new(),
            positions: HashMap::new(),
        }
    }
}

impl<K: Copy> QuoteStack<K> {
    fn push(&mut self, marker: QuoteMarker<K>) {
        let position = self.markers.len();
        self.positions
            .entry((marker.level, marker.quote_type))
            .or_default()
            .push(position);
        self.markers.push(marker);
    }

    fn latest(&self, level: u32, quote_type: QuoteType) -> Option<(usize, QuoteMarker<K>)> {
        let position = *self.positions.get(&(level, quote_type))?.last()?;
        Some((position, self.markers[position]))
    }

    fn truncate_for_level(&mut self, level: u32) {
        while self
            .markers
            .last()
            .is_some_and(|marker| marker.level > level)
        {
            self.pop();
        }
    }

    fn truncate(&mut self, len: usize) {
        while self.markers.len() > len {
            self.pop();
        }
    }

    fn pop(&mut self) {
        let marker = self.markers.pop().unwrap();
        let key = (marker.level, marker.quote_type);
        let positions = self.positions.get_mut(&key).unwrap();
        positions.pop();
    }
}

/// smart quotes related events, generated by `TextEvent`
#[derive(Clone, Copy)]
enum RelevantEvent<K> {
    Char {
        node: Option<K>,
        byte_offset: usize,
        ch: char,
        writable: bool,
        nesting_level: u32,
    },
    Space,
    HardBoundary,
}

impl<K> RelevantEvent<K> {
    fn ch(self) -> char {
        match self {
            Self::Char { ch, .. } => ch,
            Self::Space | Self::HardBoundary => SPACE,
        }
    }
}

pub struct SmartQuotesRule<
    const OPEN_SINGLE_QUOTE: char,
    const CLOSE_SINGLE_QUOTE: char,
    const OPEN_DOUBLE_QUOTE: char,
    const CLOSE_DOUBLE_QUOTE: char,
>;

impl<
    const OPEN_SINGLE_QUOTE: char,
    const CLOSE_SINGLE_QUOTE: char,
    const OPEN_DOUBLE_QUOTE: char,
    const CLOSE_DOUBLE_QUOTE: char,
> CoreRule
    for SmartQuotesRule<
        OPEN_SINGLE_QUOTE,
        CLOSE_SINGLE_QUOTE,
        OPEN_DOUBLE_QUOTE,
        CLOSE_DOUBLE_QUOTE,
    >
{
    const NAMES: &'static [&'static str] = &["smartquotes"];

    fn run(root: &mut Node, _: &MarkdownIt) {
        let projection = LegacyTextProjection::new(smartquotes_projection);
        let mut events = projection.events(root);
        let mut edits = TextEditBatch::new(smartquotes_projection);
        transform_events::<
            TextNodeKey,
            _,
            _,
            OPEN_SINGLE_QUOTE,
            CLOSE_SINGLE_QUOTE,
            OPEN_DOUBLE_QUOTE,
            CLOSE_DOUBLE_QUOTE,
        >(
            std::iter::from_fn(|| next_relevant(&mut events)),
            |node, offset, replacement| {
                edits.replace_char(node, offset..offset + 1, replacement);
            },
        );

        edits
            .commit(root)
            .expect("smartquotes creates valid non-overlapping text edits");
    }
}

#[allow(clippy::too_many_arguments)]
fn process_quote<
    K: Copy,
    F: FnMut(K, usize, char),
    const OPEN_SINGLE_QUOTE: char,
    const CLOSE_SINGLE_QUOTE: char,
    const OPEN_DOUBLE_QUOTE: char,
    const CLOSE_DOUBLE_QUOTE: char,
>(
    node: K,
    byte_offset: usize,
    ch: char,
    level: u32,
    previous: char,
    next: char,
    quote_stack: &mut QuoteStack<K>,
    replace: &mut F,
) {
    let quote_type = if ch == SINGLE_QUOTE {
        QuoteType::Single
    } else {
        QuoteType::Double
    };
    let (can_open, can_close) = can_open_or_close(quote_type, previous, next);

    if !can_open && !can_close {
        if quote_type == QuoteType::Single {
            replace(node, byte_offset, APOSTROPHE);
        }
        return;
    }

    if can_close {
        if let Some((position, opener)) = quote_stack.latest(level, quote_type) {
            let (open_quote, close_quote) = match quote_type {
                QuoteType::Single => (OPEN_SINGLE_QUOTE, CLOSE_SINGLE_QUOTE),
                QuoteType::Double => (OPEN_DOUBLE_QUOTE, CLOSE_DOUBLE_QUOTE),
            };
            replace(opener.node, opener.byte_offset, open_quote);
            replace(node, byte_offset, close_quote);
            quote_stack.truncate(position);
            return;
        }
    }

    if can_open {
        quote_stack.push(QuoteMarker {
            node,
            byte_offset,
            quote_type,
            level,
        });
    } else if can_close && quote_type == QuoteType::Single {
        replace(node, byte_offset, APOSTROPHE);
    }
}

fn transform_events<
    K: Copy,
    I: Iterator<Item = RelevantEvent<K>>,
    F: FnMut(K, usize, char),
    const OPEN_SINGLE_QUOTE: char,
    const CLOSE_SINGLE_QUOTE: char,
    const OPEN_DOUBLE_QUOTE: char,
    const CLOSE_DOUBLE_QUOTE: char,
>(
    mut events: I,
    mut replace: F,
) {
    let mut current = events.next();
    let mut previous = SPACE;
    let mut quote_stack = QuoteStack::default();

    while let Some(event) = current {
        current = events.next();
        let next_ch = current.map(RelevantEvent::ch).unwrap_or(SPACE);

        let RelevantEvent::Char {
            node,
            byte_offset,
            ch,
            writable,
            nesting_level,
        } = event
        else {
            previous = SPACE;
            if matches!(event, RelevantEvent::HardBoundary) {
                quote_stack.truncate(0);
            }
            continue;
        };

        if writable {
            quote_stack.truncate_for_level(nesting_level);
            let node = node.expect("writable text events always carry a node ID");
            if ch == SINGLE_QUOTE || ch == DOUBLE_QUOTE {
                process_quote::<
                    K,
                    F,
                    OPEN_SINGLE_QUOTE,
                    CLOSE_SINGLE_QUOTE,
                    OPEN_DOUBLE_QUOTE,
                    CLOSE_DOUBLE_QUOTE,
                >(
                    node,
                    byte_offset,
                    ch,
                    nesting_level,
                    previous,
                    next_ch,
                    &mut quote_stack,
                    &mut replace,
                );
            }
        }
        previous = ch;
    }
}

/// next releted event
fn next_relevant(
    events: &mut impl Iterator<Item = LegacyTextEvent>,
) -> Option<RelevantEvent<TextNodeKey>> {
    loop {
        match events.next()? {
            LegacyTextEvent::Char {
                node,
                byte_offset,
                ch,
                writable,
                nesting_level,
            } => {
                return Some(RelevantEvent::Char {
                    node,
                    byte_offset,
                    ch,
                    writable,
                    nesting_level,
                });
            }
            LegacyTextEvent::Boundary(LegacyTextBoundary::Space) => {
                return Some(RelevantEvent::Space);
            }
            LegacyTextEvent::Enter { nesting_level } | LegacyTextEvent::Exit { nesting_level } => {
                let _ = nesting_level;
            }
        }
    }
}

/// classify
fn smartquotes_projection(node: &Node) -> LegacyTextProjectionKind<'_> {
    if let Some(text) = node.cast::<Text>() {
        // normal text, editable
        LegacyTextProjectionKind::Writable(text)
    } else if let Some(html) = node.cast::<HtmlInline>() {
        // HTML, not editable (protect quotes for <a href="...">)
        LegacyTextProjectionKind::ReadOnly(&html.content)
    } else if node.is::<Paragraph>() || node.is::<Hardbreak>() || node.is::<Softbreak>() {
        // boundary, process stack content
        LegacyTextProjectionKind::Boundary(LegacyTextBoundary::Space)
    } else {
        // other rule, do nothing
        LegacyTextProjectionKind::Transparent
    }
}

fn document_relevant_event(event: TextEvent) -> Option<RelevantEvent<NodeId>> {
    match event {
        TextEvent::Char {
            node,
            byte_offset,
            ch,
            writable,
            nesting_level,
        } => Some(RelevantEvent::Char {
            node: Some(node),
            byte_offset,
            ch,
            writable,
            nesting_level,
        }),
        TextEvent::Boundary(TextBoundary::Space) => Some(RelevantEvent::Space),
        TextEvent::Boundary(TextBoundary::Hard) => Some(RelevantEvent::HardBoundary),
        TextEvent::Enter { .. } | TextEvent::Exit { .. } => None,
    }
}

fn document_smartquotes_projection(node: NodeRef<'_>) -> TextProjectionKind<'_> {
    if let Some(text) = node.cast::<Text>() {
        TextProjectionKind::Writable(&text.content)
    } else if let Some(html) = node.cast::<HtmlInline>() {
        TextProjectionKind::ReadOnly(&html.content)
    } else if node.is::<Paragraph>() || node.is::<Hardbreak>() || node.is::<Softbreak>() {
        TextProjectionKind::Boundary(TextBoundary::Space)
    } else {
        TextProjectionKind::Transparent
    }
}

fn can_open_or_close(quote_type: QuoteType, last_char: char, next_char: char) -> (bool, bool) {
    let is_double = quote_type == QuoteType::Double;
    if next_char == DOUBLE_QUOTE && is_double && last_char.is_ascii_digit() {
        return (false, false);
    }

    let is_last_punctuation = last_char.is_ascii_punctuation() || is_punct_char(last_char);
    let is_next_punctuation = next_char.is_ascii_punctuation() || is_punct_char(next_char);
    let is_last_whitespace = last_char.is_whitespace();
    let is_next_whitespace = next_char.is_whitespace();

    let can_open =
        !is_next_whitespace && (!is_next_punctuation || is_last_whitespace || is_last_punctuation);
    let can_close =
        !is_last_whitespace && (!is_last_punctuation || is_next_whitespace || is_next_punctuation);

    if can_open && can_close {
        return (is_last_punctuation, is_next_punctuation);
    }
    (can_open, can_close)
}

#[cfg(test)]
mod tests {
    use crate::{Document, DocumentTransform, EditBatch, StructuralEvent};

    #[test]
    fn smartquotes_basics() {
        let md = &mut crate::MarkdownIt::empty();
        crate::plugins::cmark::add(md);
        crate::plugins::extra::smartquotes::add(md);
        let html = md.parse(r#"'hello' "world""#).render();
        assert_eq!(html.trim(), r#"<p>‘hello’ “world”</p>"#);
    }

    #[test]
    fn smartquotes_shouldnt_affect_html() {
        let md = &mut crate::MarkdownIt::empty();
        crate::plugins::cmark::add(md);
        crate::plugins::html::html_inline::add(md);
        crate::plugins::extra::smartquotes::add(md);
        let html = md.parse(r#"<a href="hello"></a>"#).render();
        assert_eq!(html.trim(), r#"<p><a href="hello"></a></p>"#);
    }

    #[test]
    fn smartquotes_should_work_with_typographer() {
        let md = &mut crate::MarkdownIt::empty();
        crate::plugins::cmark::add(md);
        crate::plugins::html::html_inline::add(md);
        crate::plugins::extra::typographer::add(md);
        crate::plugins::extra::smartquotes::add(md);
        let html = md.parse("\"**...**\"").render();
        assert_eq!(html.trim(), "<p>“<strong>…</strong>”</p>");
    }

    #[test]
    fn unicode_before_quotes_uses_byte_offsets() {
        let md = &mut crate::MarkdownIt::empty();
        crate::plugins::cmark::add(md);
        crate::plugins::extra::smartquotes::add(md);
        assert_eq!(md.parse("雪 \"雨\"").render(), "<p>雪 “雨”</p>\n");
    }

    #[test]
    fn document_registration_is_explicit_and_supports_custom_quote_sets() {
        type LegacyClassic = super::SmartQuotesRule<'‘', '’', '“', '”'>;

        let md = &mut crate::MarkdownIt::empty();
        crate::plugins::cmark::add(md);
        super::add_document_with::<'‹', '›', '«', '»'>(md);
        assert!(!md.has_rule::<LegacyClassic>());

        let mut document = md.parse_document(r#"'hello' "world""#);
        assert_eq!(
            document
                .events(document.root())
                .unwrap()
                .filter_map(|event| match event {
                    StructuralEvent::Leaf(node) => node
                        .cast::<crate::parser::inline::Text>()
                        .map(|text| text.content.as_str()),
                    StructuralEvent::Enter(_) | StructuralEvent::Exit(_) => None,
                })
                .collect::<String>(),
            r#"'hello' "world""#
        );

        md.run_document_transforms(&mut document).unwrap();
        assert_eq!(document.into_legacy().render(), "<p>‹hello› «world»</p>\n");
    }

    #[derive(Default)]
    struct ObserveSmartQuotes;

    impl DocumentTransform for ObserveSmartQuotes {
        const KEY: &'static str = "test::observe-smartquotes";

        fn run(&self, document: &Document) -> EditBatch {
            let transformed = document
                .events(document.root())
                .unwrap()
                .any(|event| match event {
                    StructuralEvent::Leaf(node) => node
                        .cast::<crate::parser::inline::Text>()
                        .is_some_and(|text| text.content.contains('“')),
                    StructuralEvent::Enter(_) | StructuralEvent::Exit(_) => false,
                });
            let mut edits = EditBatch::new();
            edits.set_attribute(
                document.root(),
                "observed-smartquotes",
                transformed.to_string(),
            );
            edits
        }
    }

    #[test]
    fn registered_transform_supports_type_ordering() {
        let md = &mut crate::MarkdownIt::empty();
        crate::plugins::cmark::add(md);
        md.add_document_transform::<ObserveSmartQuotes>()
            .after::<super::ClassicSmartQuotesDocumentTransform>();
        super::add_document(md);
        let mut document = md.parse_document(r#""world""#);

        md.run_document_transforms(&mut document).unwrap();

        assert!(
            document
                .node(document.root())
                .unwrap()
                .attrs()
                .iter()
                .any(|(name, value)| name == "observed-smartquotes" && value == "true")
        );
    }

    #[test]
    fn legacy_registration_does_not_populate_document_registry() {
        let md = &mut crate::MarkdownIt::empty();
        crate::plugins::cmark::add(md);
        super::add(md);

        assert!(
            !md.document_transforms
                .contains::<super::ClassicSmartQuotesDocumentTransform>()
        );
        assert_eq!(
            md.parse_document(r#""world""#).into_legacy().render(),
            "<p>“world”</p>\n"
        );
    }
}
