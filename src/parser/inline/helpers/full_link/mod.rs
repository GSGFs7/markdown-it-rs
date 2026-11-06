//! Structure similar to `[link](<to> "stuff")` with configurable prefix.
//!
//! There are two structures in CommonMark that match this syntax:
//!  - links - `[text](<href> "title")`
//!  - images - `![alt](<src> "title")`
//!
//! You can add custom rules like `~[foo](<bar> "baz")`. Let us know if
//! you come up with fun use case to add as an example!
//!
//! Add a custom structure by using [add_prefix] function, which takes following arguments:
//!  - `PREFIX` - marker character before label (`!` in case of images)
//!  - `ENABLE_NESTED` - allow nested links inside
//!  - `md` - parser instance
//!  - `f` - function that should return your custom [NodeId] given href and title
//!

// TODO: rewrite this

use crate::MarkdownIt;
use crate::document::{Document, NodeId};
use crate::parser::inline::{InlineRule, InlineState};

mod cache;
mod candidate;
mod config;
mod label;
mod reference;
mod target;

pub(crate) use self::cache::LinkLabelBounds;
use self::candidate::{link_check, link_run};
use self::config::LinkCfg;
#[cfg(feature = "linkify")]
pub(crate) use self::config::MarkdownLinkLabel;
pub(crate) use self::config::{
    IndependentLabelSource,
    InlineTargetFallback,
    LabelContext,
    LinkOptions,
};
pub use self::target::{ParseLinkFragmentResult, parse_link_destination, parse_link_title};

/// Register a link rule with an arena node factory.
pub fn add<const ENABLE_NESTED: bool>(
    md: &mut MarkdownIt,
    factory: fn(&mut Document, Option<String>, Option<String>) -> NodeId,
) {
    md.ext.insert(LinkCfg::<'\0'> {
        factory,
        options: LinkOptions::default(),
    });
    md.inline.add_rule::<LinkScanner<ENABLE_NESTED>>();
    if !md.inline.has_rule::<LinkScannerEnd>() {
        md.inline.add_rule::<LinkScannerEnd>();
    }
}

/// Register a prefixed link rule with default parsing behavior.
pub fn add_prefix<const PREFIX: char, const ENABLE_NESTED: bool>(
    md: &mut MarkdownIt,
    factory: fn(&mut Document, Option<String>, Option<String>) -> NodeId,
) {
    add_prefix_with_options::<PREFIX, ENABLE_NESTED>(md, LinkOptions::default(), factory);
}

/// Register a prefixed rule with explicit parsing behavior.
/// The returned builder can assign syntax-specific names and rule ordering.
pub(crate) fn add_prefix_with_options<const PREFIX: char, const ENABLE_NESTED: bool>(
    md: &mut MarkdownIt,
    options: LinkOptions,
    factory: fn(&mut Document, Option<String>, Option<String>) -> NodeId,
) -> crate::parser::inline::RuleBuilder<'_, crate::parser::inline::RuleEntry> {
    md.ext.insert(LinkCfg::<PREFIX> { factory, options });
    if !md.inline.has_rule::<LinkScannerEnd>() {
        md.inline.add_rule::<LinkScannerEnd>();
    }
    md.inline
        .add_rule::<LinkPrefixScanner<PREFIX, ENABLE_NESTED>>()
}

#[doc(hidden)]
pub struct LinkScanner<const ENABLE_NESTED: bool>;

impl<const ENABLE_NESTED: bool> InlineRule for LinkScanner<ENABLE_NESTED> {
    const MARKER: char = '[';
    const NAMES: &'static [&'static str] = &["link"];

    fn check(context: &mut InlineState<'_>) -> Option<usize> {
        let options = context.markdown_it().ext.get::<LinkCfg<'\0'>>()?.options;
        link_check(context, ENABLE_NESTED, 0, options.inline_target_fallback)
    }

    fn run(state: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        let config = *state
            .markdown_it()
            .ext
            .get::<LinkCfg<'\0'>>()
            .expect("direct link rule requires an arena node factory");
        link_run(state, ENABLE_NESTED, 0, config.options, config.factory)
    }
}

#[doc(hidden)]
pub struct LinkPrefixScanner<const PREFIX: char, const ENABLE_NESTED: bool>;

impl<const PREFIX: char, const ENABLE_NESTED: bool> InlineRule
    for LinkPrefixScanner<PREFIX, ENABLE_NESTED>
{
    const MARKER: char = PREFIX;
    const NAMES: &'static [&'static str] = &["link_prefix"];

    fn check(context: &mut InlineState<'_>) -> Option<usize> {
        if !context.remaining().starts_with(PREFIX) {
            return None;
        }

        let options = context.markdown_it().ext.get::<LinkCfg<PREFIX>>()?.options;
        link_check(
            context,
            ENABLE_NESTED,
            PREFIX.len_utf8(),
            options.inline_target_fallback,
        )
    }

    fn run(state: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        if !state.remaining().starts_with(PREFIX) {
            return None;
        }

        let config = *state
            .markdown_it()
            .ext
            .get::<LinkCfg<PREFIX>>()
            .expect("direct prefix rule requires an arena node factory");
        link_run(
            state,
            ENABLE_NESTED,
            PREFIX.len_utf8(),
            config.options,
            config.factory,
        )
    }
}

#[doc(hidden)]
/// this rule makes sure that parser is stopped on "]" character,
/// but it actually doesn't do anything
pub struct LinkScannerEnd;

impl InlineRule for LinkScannerEnd {
    const MARKER: char = ']';
    const NAMES: &'static [&'static str] = &["link_end"];

    fn run(_state: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        None
    }
}

#[cfg(test)]
mod tests;
