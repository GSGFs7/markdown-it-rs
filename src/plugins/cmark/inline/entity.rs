//! Entity and numeric character references
//!
//! `&#123;`, `&#xAF;`, `&quot;`
//!
//! <https://spec.commonmark.org/0.30/#entity-and-numeric-character-references>
use std::sync::LazyLock;

use regex::Regex;

use crate::common::utils::{get_entity_from_str, is_valid_entity_code};
use crate::parser::document::NodeDraft;
use crate::parser::document_parser::DocumentInlineState;
use crate::parser::inline::probe::{InlineProbeContext, InlineProbeKind, InlineProbeResult};
use crate::parser::inline::{InlineRule, InlineState, LegacyInlineRule, TextSpecial};
use crate::parser::main::MarkdownIt;
use crate::parser::node::Node;

pub fn add(md: &mut MarkdownIt) {
    md.inline.add_migrated_rule::<EntityScanner>();
}

static DIGITAL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("(?i)^&#((?:x[a-f0-9]{1,6}|[0-9]{1,7}));").unwrap());

static NAMED_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("(?i)^&([a-z][a-z0-9]{1,31});").unwrap());

enum EntityValue {
    Character(char),
    Named(&'static str),
}

struct EntityMatch {
    len: usize,
    value: EntityValue,
}

fn scan_entity(src: &str) -> Option<EntityMatch> {
    if !src.starts_with('&') {
        return None;
    }

    if src.starts_with("&#") {
        let capture = DIGITAL_RE.captures(src)?;
        let entity = &capture[1];
        let code = if entity.starts_with('x') || entity.starts_with('X') {
            u32::from_str_radix(&entity[1..], 16).ok()?
        } else {
            entity.parse::<u32>().ok()?
        };
        let value = if is_valid_entity_code(code) {
            char::from_u32(code)?
        } else {
            '\u{FFFD}'
        };
        Some(EntityMatch {
            len: capture[0].len(),
            value: EntityValue::Character(value),
        })
    } else {
        let capture = NAMED_RE.captures(src)?;
        Some(EntityMatch {
            len: capture[0].len(),
            value: EntityValue::Named(get_entity_from_str(&capture[0])?),
        })
    }
}

#[doc(hidden)]
pub struct EntityScanner;

impl EntityScanner {
    fn parse(src: &str) -> Option<(TextSpecial, usize)> {
        let matched = scan_entity(src)?;
        let content = match matched.value {
            EntityValue::Character(ch) => ch.to_string(),
            EntityValue::Named(value) => value.to_owned(),
        };
        Some((
            TextSpecial {
                content,
                markup: src[..matched.len].to_owned(),
                info: "entity",
            },
            matched.len,
        ))
    }
}

impl InlineRule for EntityScanner {
    const MARKER: char = '&';
    const NAMES: &'static [&'static str] = &["entity"];

    fn probe(context: &mut InlineProbeContext<'_>) -> InlineProbeResult {
        match scan_entity(context.remaining()) {
            Some(matched) => InlineProbeResult::Match {
                len: matched.len,
                kind: InlineProbeKind::Token,
            },
            None => InlineProbeResult::NoMatch,
        }
    }

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
        let (entity, len) = Self::parse(&state.src[state.pos..state.pos_max])?;
        Some((Some(NodeDraft::new(entity)), len))
    }
}

impl LegacyInlineRule for EntityScanner {
    const MARKER: char = '&';
    const NAMES: &'static [&'static str] = &["entity"];

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let (entity, len) = Self::parse(&state.src[state.pos..state.pos_max])?;
        Some((Node::new(entity), len))
    }
}
