//! Replaces `"` and `'` quotes with typographic quotation marks.
//!
//! The implementation consumes a lazy text projection of the AST and records
//! byte-based text edits. The tree is mutated only after the complete edit
//! batch has been validated.

use std::collections::HashMap;

use crate::common::utils::{is_markdown_whitespace, is_punct_char};
use crate::document::TextSpecial;
use crate::parser::core::{CoreRule, CoreState};
use crate::parser::inline::helpers::full_link::IndependentLabelSource;
use crate::plugins::cmark::block::heading::ATXHeading;
use crate::plugins::cmark::block::lheading::SetextHeader;
use crate::plugins::cmark::block::list::ListItem;
use crate::plugins::cmark::block::paragraph::Paragraph;
use crate::plugins::cmark::inline::backticks::CodeInline;
use crate::plugins::cmark::inline::image::Image;
use crate::plugins::cmark::inline::newline::{Hardbreak, Softbreak};
use crate::plugins::extra::tables::TableCell;
use crate::plugins::html::html_inline::HtmlInline;
use crate::{
    Document,
    EditBatch,
    MarkdownIt,
    NodeId,
    NodeRef,
    TextBoundary,
    TextEvent,
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

/// Register smartquotes with a custom compile-time quote set.
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
    >>().after_named("inline").after_named("footnote_tail");
}

/// The classic smartquotes rule used by [`add`].
pub type ClassicSmartQuotesRule = SmartQuotesRule<'‘', '’', '“', '”'>;

/// Smartquotes with a compile-time quote set.
#[derive(Default)]
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
    const NAMES: &'static [&'static str] = &["extra::smartquotes", "smartquotes"];

    fn run(&self, state: &mut CoreState<'_>) {
        let document = &mut *state.document;
        let edits = edits_with::<
            OPEN_SINGLE_QUOTE,
            CLOSE_SINGLE_QUOTE,
            OPEN_DOUBLE_QUOTE,
            CLOSE_DOUBLE_QUOTE,
        >(document);
        edits.commit(document);
    }
}

fn edits_with<
    const OPEN_SINGLE_QUOTE: char,
    const CLOSE_SINGLE_QUOTE: char,
    const OPEN_DOUBLE_QUOTE: char,
    const CLOSE_DOUBLE_QUOTE: char,
>(
    document: &Document,
) -> EditBatch {
    let mut edits = EditBatch::new();
    let mut protected_depth = 0;
    let mut opaque_image = None;
    let events = document
        .text_events(smartquotes_projection)
        .filter_map(|event| {
            // JS sees one image token whose content is the original label, not
            // the parsed alt children. Their breaks and nesting are local to alt.
            if let Some(image) = opaque_image {
                return match event {
                    TextEvent::Char { node, .. } if node == image => {
                        relevant_event(document, event)
                    }
                    TextEvent::Exit { node, .. } if node == image => {
                        opaque_image = None;
                        None
                    }
                    _ => None,
                };
            }
            if let TextEvent::Enter { node, .. } = event
                && document.node(node).is::<Image>()
                && document
                    .node(node)
                    .ext()
                    .contains::<IndependentLabelSource>()
            {
                opaque_image = Some(node);
                return None;
            }
            match event {
                TextEvent::Enter { node, .. } | TextEvent::Exit { node, .. }
                    if document.node(node).is::<CodeInline>()
                        || document.node(node).is::<Image>() =>
                {
                    if matches!(event, TextEvent::Enter { .. }) {
                        protected_depth += 1;
                    } else {
                        protected_depth -= 1;
                    }
                    return None;
                }
                _ => {}
            }

            let mut event = relevant_event(document, event)?;
            // Code and alt text provide context but cannot be edited.
            if protected_depth > 0
                && let RelevantEvent::Char { writable, .. } = &mut event
            {
                *writable = false;
            }
            Some(event)
        });

    let mut special_replacements: HashMap<NodeId, Vec<(usize, char)>> = HashMap::new();
    transform_events::<
        NodeId,
        _,
        _,
        OPEN_SINGLE_QUOTE,
        CLOSE_SINGLE_QUOTE,
        OPEN_DOUBLE_QUOTE,
        CLOSE_DOUBLE_QUOTE,
    >(events, |node, byte_offset, replacement| {
        if document.node(node).is::<TextSpecial>() {
            special_replacements
                .entry(node)
                .or_default()
                .push((byte_offset, replacement));
        } else {
            edits.replace_char(node, byte_offset..byte_offset + 1, replacement);
        }
    });
    for (node, mut replacements) in special_replacements {
        let text = document.node(node).cast::<TextSpecial>().unwrap();
        replacements.sort_unstable_by_key(|&(offset, _)| offset);

        let mut content = String::with_capacity(text.content.len());
        let mut end = 0;
        for (offset, replacement) in replacements {
            content.push_str(&text.content[end..offset]);
            content.push(replacement);
            end = offset + 1;
        }
        content.push_str(&text.content[end..]);
        edits.replace_value(
            node,
            TextSpecial {
                content,
                markup: text.markup.clone(),
                info: text.info,
            },
        );
    }
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
    Level(u32),
}

impl<K> RelevantEvent<K> {
    fn ch(self) -> char {
        match self {
            Self::Char { ch, .. } => ch,
            Self::Space | Self::HardBoundary | Self::Level(_) => SPACE,
        }
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
        if let RelevantEvent::Level(level) = event {
            quote_stack.truncate_for_level(level);
            current = events.next();
            continue;
        }
        current = events.next();
        // Structural tokens trim opener state without affecting adjacent-
        // character context; apply their level after the current character.
        let mut next_level = u32::MAX;
        while let Some(RelevantEvent::Level(level)) = current {
            next_level = next_level.min(level);
            current = events.next();
        }
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
            quote_stack.truncate_for_level(next_level);
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
        quote_stack.truncate_for_level(next_level);
    }
}

fn relevant_event(document: &Document, event: TextEvent) -> Option<RelevantEvent<NodeId>> {
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
        TextEvent::Exit { node, .. }
            if matches!(
                smartquotes_projection(document, document.node(node)),
                TextProjectionKind::Boundary(TextBoundary::Hard)
            ) =>
        {
            Some(RelevantEvent::HardBoundary)
        }
        TextEvent::Exit { nesting_level, .. } => Some(RelevantEvent::Level(nesting_level)),
        TextEvent::Enter { .. } => None,
    }
}

fn smartquotes_projection<'a>(document: &'a Document, node: NodeRef<'a>) -> TextProjectionKind<'a> {
    if node.is::<Image>()
        && let Some(source) = node.ext().get::<IndependentLabelSource>()
    {
        TextProjectionKind::ReadOnly(&source.0)
    } else if let Some(text) = document.text(node.id()) {
        // normal text, editable
        TextProjectionKind::Writable(text)
    } else if let Some(text) = document.value::<TextSpecial>(node.id()) {
        if text.info == "autolink" {
            // JS smartquotes processes automatic link labels as normal text.
            TextProjectionKind::Writable(&text.content)
        } else {
            // Escapes and entities provide context but remain literal.
            TextProjectionKind::ReadOnly(&text.content)
        }
    } else if let Some(html) = document.value::<HtmlInline>(node.id()) {
        // HTML, not editable (protect quotes for <a href="...">)
        TextProjectionKind::ReadOnly(&html.content)
    } else if node.is::<Paragraph>()
        || node.is::<ATXHeading>()
        || node.is::<SetextHeader>()
        || node.is::<TableCell>()
        || node.is::<ListItem>()
    {
        // Reset quotes between blocks; tight list items have no Paragraph wrapper.
        TextProjectionKind::Boundary(TextBoundary::Hard)
    } else if node.is::<Hardbreak>() || node.is::<Softbreak>() {
        TextProjectionKind::Boundary(TextBoundary::Space)
    } else {
        // other rule, do nothing
        TextProjectionKind::Transparent
    }
}

fn can_open_or_close(quote_type: QuoteType, last_char: char, next_char: char) -> (bool, bool) {
    let is_double = quote_type == QuoteType::Double;
    if next_char == DOUBLE_QUOTE && is_double && last_char.is_ascii_digit() {
        return (false, false);
    }

    // markdown-it inspects adjacent UTF-16 code units with charCodeAt.
    // Supplementary characters appear as surrogates, not punctuation/symbols.
    let is_last_punctuation = last_char.len_utf16() == 1
        && (last_char.is_ascii_punctuation() || is_punct_char(last_char));
    let is_next_punctuation = next_char.len_utf16() == 1
        && (next_char.is_ascii_punctuation() || is_punct_char(next_char));
    let is_last_whitespace = is_markdown_whitespace(last_char);
    let is_next_whitespace = is_markdown_whitespace(next_char);

    let can_open =
        !is_next_whitespace && (!is_next_punctuation || is_last_whitespace || is_last_punctuation);
    let can_close =
        !is_last_whitespace && (!is_last_punctuation || is_next_whitespace || is_next_punctuation);

    if can_open && can_close {
        return (is_last_punctuation, is_next_punctuation);
    }
    (can_open, can_close)
}
