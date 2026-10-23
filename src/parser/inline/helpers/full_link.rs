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
use std::any::TypeId;
use std::collections::{HashMap, HashSet};

use crate::MarkdownIt;
use crate::common::utils::unescape_all;
use crate::document::{Document, NodeId};
use crate::parser::inline::{InlineRule, InlineState};
use crate::plugins::cmark::block::reference::ReferenceMap;

#[derive(Debug)]
struct InlineLinkTarget {
    href: Option<String>,
    title: Option<String>,
    end: usize,
}

// TODO: public some API?
// TODO: refactor this

/// How to proceed when an attempted inline target cannot be parsed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum InlineTargetFallback {
    /// Try resolving the label as a reference.
    #[default]
    Reference,
    /// Reject this rule's candidate without trying a reference.
    Reject,
}

/// Nesting context used to parse the label's inline children.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum LabelContext {
    /// Inherit the current depth and increment the link nesting level.
    #[default]
    NestedLink,
    /// Parse with a fresh depth and link nesting budget.
    Independent,
}

/// Parsing behavior independent of the marker and the constructed node type.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct LinkOptions {
    pub(crate) inline_target_fallback: InlineTargetFallback,
    pub(crate) label_context: LabelContext,
}

#[derive(Debug, Clone, Copy)]
struct LinkCfg<const PREFIX: char> {
    factory: fn(&mut Document, Option<String>, Option<String>) -> NodeId,
    options: LinkOptions,
}

/// Marks a Markdown link label, which core linkify must skip regardless of
/// inline anchor counter changes from HTML tags.
#[derive(Debug)]
pub(crate) struct MarkdownLinkLabel;

/// Original content of a label parsed in its own inline context (image alt).
#[derive(Debug)]
pub(crate) struct IndependentLabelSource(pub String);

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

// Scan a link label with independent check sessions.
//
// This function assumes that the first character ("[") already matches;
// it returns the byte length of the label including the closing "]".
fn scan_link_label(mut context: InlineState<'_>, enable_nested: bool) -> Option<usize> {
    // Empty labels close immediately, even at the nesting limit; no cache needed.
    if context.remaining().starts_with(']') {
        return Some(0);
    }
    let start = context.pos;
    let memoize = context
        .scan_limit_cache
        .borrow()
        .link_bounds
        .as_ref()
        .is_some_and(|bounds| bounds.can_memoize_labels(start, context.pos_max));
    let initial_key = LabelScanKey {
        start,
        end: context.pos_max,
        depth: context.depth,
        link_level: context.link_level,
        enable_nested,
    };
    if memoize {
        let shared = context.scan_limit_cache.borrow();
        if shared.link_bounds.as_ref()?.failed_labels.contains(&(
            initial_key.start,
            initial_key.end,
            initial_key.link_level,
            initial_key.enable_nested,
        )) {
            return None;
        }
        if let Some(&(epoch, close)) = shared.link_bounds.as_ref()?.labels.get(&initial_key) {
            // Additional nesting cutoffs can invalidate a successful boundary.
            if epoch == shared.epoch() {
                return Some(close - start);
            }
        }
    }

    let mut open = vec![initial_key];
    while let Some(ch) = context.remaining().chars().next() {
        let before = context.remaining().len();
        if ch == ']' {
            let matched = open.pop().unwrap();
            if memoize {
                // Every raw opener traversed by this scan has the same fresh
                // label boundary at this depth. Record its result as well, so
                // later candidates do not walk the overlapping label again.
                cache_label_scan(&context, matched, Some(context.pos));
            }
            if open.is_empty() {
                return Some(context.pos - start);
            }
        }

        if context.skip_token().is_none() {
            break;
        }

        let consumed = before - context.remaining().len();
        if ch == '[' {
            if consumed == 1 {
                if memoize
                    && context
                        .scan_limit_cache
                        .borrow()
                        .link_bounds
                        .as_ref()
                        .is_some_and(|bounds| {
                            bounds.failed_labels.contains(&(
                                context.pos,
                                context.pos_max,
                                context.link_level,
                                enable_nested,
                            ))
                        })
                {
                    // An uncloseable nested opener also blocks this label; its
                    // probe already recorded any nesting cutoff.
                    break;
                }
                open.push(LabelScanKey {
                    start: context.pos,
                    end: context.pos_max,
                    depth: context.depth,
                    link_level: context.link_level,
                    enable_nested,
                });
            } else if !enable_nested {
                break;
            }
        }
    }

    if memoize {
        for key in open {
            cache_label_scan(&context, key, None);
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct LabelScanKey {
    start: usize,
    end: usize,
    depth: u32,
    link_level: i32,
    enable_nested: bool,
}

fn cache_label_scan(context: &InlineState<'_>, key: LabelScanKey, close: Option<usize>) {
    let mut shared = context.scan_limit_cache.borrow_mut();
    let epoch = shared.epoch();
    // Only reached while `memoize` is set, which requires `link_bounds`.
    let bounds = shared
        .link_bounds
        .as_mut()
        .expect("label memoization requires link bounds");
    if close.is_none() {
        bounds
            .failed_labels
            .insert((key.start, key.end, key.link_level, key.enable_nested));
        return;
    }
    bounds.labels.insert(key, (epoch, close.unwrap()));
}

pub struct ParseLinkFragmentResult {
    /// end position
    pub pos: usize,
    /// number of linebreaks inside
    pub lines: usize,
    /// parsed result
    pub str: String,
}

/// Helper function used to parse `<href>` part of the links with optional brackets.
pub fn parse_link_destination(
    str: &str,
    start: usize,
    max: usize,
) -> Option<ParseLinkFragmentResult> {
    let mut chars = str[start..max].chars().peekable();
    let mut pos = start;

    if let Some('<') = chars.peek() {
        chars.next(); // skip '<'
        pos += 1;
        loop {
            match chars.next() {
                Some('\n' | '<') | None => return None,
                Some('>') => {
                    return Some(ParseLinkFragmentResult {
                        pos: pos + 1,
                        lines: 0,
                        str: unescape_all(&str[start + 1..pos]).into_owned(),
                    });
                }
                Some('\\') => {
                    let x = chars.next()?;
                    pos += 1 + x.len_utf8();
                }
                Some(x) => {
                    pos += x.len_utf8();
                }
            }
        }
    } else {
        let mut level: u32 = 0;
        loop {
            match chars.next() {
                // space + ascii control characters
                Some('\0'..=' ' | '\x7f') | None => break,
                Some('\\') => match chars.next() {
                    Some(' ') | None => {
                        // [a](/url\ "title")
                        //          ^--- this space can't be escape in CommonMark
                        // it should be:
                        //  <a href="/url%5C" title="title">a</a>
                        //               ^^^--- backslash here
                        pos += 1;
                        break;
                    }
                    Some(x) => pos += 1 + x.len_utf8(),
                },
                Some('(') => {
                    level += 1;
                    if level > 32 {
                        return None;
                    }
                    pos += 1;
                }
                Some(')') => {
                    if level == 0 {
                        break;
                    }
                    level -= 1;
                    pos += 1;
                }
                Some(x) => {
                    pos += x.len_utf8();
                }
            }
        }

        if level != 0 {
            return None;
        }

        Some(ParseLinkFragmentResult {
            pos,
            lines: 0,
            str: unescape_all(&str[start..pos]).into_owned(),
        })
    }
}

/// Helper function used to parse `"title"` part of the links (with `'title'` or `(title)` alternative syntax).
pub fn parse_link_title(str: &str, start: usize, max: usize) -> Option<ParseLinkFragmentResult> {
    let mut chars = str[start..max].chars();
    let mut pos = start + 1;
    let mut lines = 0;

    let marker = match chars.next() {
        Some('"') => '"',
        Some('\'') => '\'',
        Some('(') => ')',
        None | Some(_) => return None,
    };

    loop {
        match chars.next() {
            Some(ch) if ch == marker => {
                return Some(ParseLinkFragmentResult {
                    pos: pos + 1,
                    lines,
                    str: unescape_all(&str[start + 1..pos]).into_owned(),
                });
            }
            Some('(') if marker == ')' => {
                return None;
            }
            Some('\n') => {
                pos += 1;
                lines += 1;
            }
            Some('\\') => {
                let x = chars.next()?;
                pos += 1 + x.len_utf8();
                if x == '\n' {
                    // [foo]: /url "
                    // hello
                    // \
                    // world
                    // "
                    //
                    // physical line break, must be recorded
                    // otherwise, some problems may occur during parsing
                    lines += 1;
                }
            }
            Some(x) => {
                pos += x.len_utf8();
            }
            None => {
                return None;
            }
        }
    }
}

struct ParseLinkResult {
    pub label_start: usize,
    pub label_end: usize,
    pub href: Option<String>,
    pub title: Option<String>,
    pub end: usize,
}

// [link](  <href>  "title"  )
//        ^^ skipping these spaces
fn skip_link_spaces(source: &str, mut pos: usize, max: usize) -> usize {
    while pos < max && matches!(source.as_bytes()[pos], b' ' | b'\t' | b'\n') {
        pos += 1;
    }
    pos
}

fn scan_inline_link_target(
    md: &MarkdownIt,
    source: &str,
    start: usize,
    max: usize,
) -> (Option<InlineLinkTarget>, usize) {
    let mut pos = start;
    if pos >= max {
        return (None, max);
    }

    let mut href = None;
    let mut title = None;

    // [link](  <href>  "title"  )
    //          ^^^^^^ parsing link destination
    if let Some(result) = parse_link_destination(source, pos, max) {
        let candidate = md.link_formatter.normalize_link(&result.str);
        if md.link_formatter.validate_link(&candidate).is_some() {
            pos = result.pos;
            href = Some(candidate);
        }

        // [link](  <href>  "title"  )
        //                ^^ skipping these spaces
        let title_separator_start = pos;
        pos = skip_link_spaces(source, pos, max);
        if pos > title_separator_start
            && let Some(result) = parse_link_title(source, pos, max)
        {
            title = Some(result.str);
            // [link](  <href>  "title"  )
            //                         ^^ skipping these spaces
            pos = skip_link_spaces(source, result.pos, max);
        }
    }

    let target = if source[pos..max].starts_with(')') {
        Some(InlineLinkTarget {
            href,
            title,
            end: pos + 1,
        })
    } else {
        None
    };
    // markdown-it advances past the failed delimiter before probing for a
    // reference at that cursor.
    let next = source[pos..max].chars().next().map_or(pos + 1, |ch| {
        pos + if ch.len_utf16() == 1 {
            ch.len_utf8()
        } else {
            1
        }
    });
    (target, next)
}

fn resolve_reference_link(
    source: &str,
    label_start: usize,
    label_end: usize,
    reference_end: Option<(usize, usize)>,
    references: &ReferenceMap,
) -> Option<ParseLinkResult> {
    let (label, end) = match reference_end {
        Some((start, close)) => {
            let explicit_label = &source[start..close];
            let label = if explicit_label.is_empty() {
                &source[label_start..label_end]
            } else {
                explicit_label
            };
            (label, close + 1)
        }
        None => (&source[label_start..label_end], label_end + 1),
    };

    let (destination, title) = references.get(label)?;
    Some(ParseLinkResult {
        label_start,
        label_end,
        href: Some(destination.to_owned()),
        title: title.map(str::to_owned),
        end,
    })
}

fn scan_link_candidate(
    context: &mut InlineState<'_>,
    pos: usize,
    enable_nested: bool,
    references: Option<&ReferenceMap>,
    fallback: InlineTargetFallback,
) -> Option<ParseLinkResult> {
    if !has_possible_link_label_close(context, pos, references) {
        return None;
    }

    let source = context.remaining();
    let child_depth = context.depth.checked_add(1)?;
    let label_start = pos + 1;
    let label_end = label_start
        + scan_link_label(
            context.child_state(label_start..source.len(), child_depth, context.link_level)?,
            enable_nested,
        )?;

    let source = context.remaining();
    let suffix_start = label_end + 1;
    let mut reference_pos = suffix_start;
    if source[suffix_start..].starts_with('(') {
        let target_start = skip_link_spaces(source, suffix_start + 1, source.len());
        // An opening parenthesis followed only by spaces cannot resolve a reference.
        if target_start == source.len() {
            return None;
        }

        let (target, next_pos) =
            scan_inline_link_target(context.markdown_it(), source, target_start, source.len());
        reference_pos = next_pos;
        if let Some(target) = target {
            return Some(ParseLinkResult {
                label_start,
                label_end,
                href: target.href,
                title: target.title,
                end: target.end,
            });
        }

        if fallback == InlineTargetFallback::Reject {
            return None;
        }
    }

    let reference_end = if source.as_bytes().get(reference_pos) == Some(&b'[') {
        let reference_start = reference_pos + 1;
        context
            .child_state(
                reference_start..source.len(),
                child_depth,
                context.link_level,
            )
            .and_then(|child| scan_link_label(child, true))
            .map(|close| (reference_start, reference_start + close))
    } else {
        None
    };

    let source = context.remaining();
    resolve_reference_link(source, label_start, label_end, reference_end, references?)
}

fn link_check(
    context: &mut InlineState<'_>,
    enable_nested: bool,
    offset: usize,
    fallback: InlineTargetFallback,
) -> Option<usize> {
    let references = context.root_ext.and_then(|ext| ext.get::<ReferenceMap>());
    scan_link_candidate(context, offset, enable_nested, references, fallback)
        .map(|candidate| candidate.end)
}

fn link_run(
    state: &mut InlineState<'_>,
    enable_nested: bool,
    offset: usize,
    options: LinkOptions,
    factory: fn(&mut Document, Option<String>, Option<String>) -> NodeId,
) -> Option<(Option<NodeId>, usize)> {
    // Parses [link](<to> "stuff") and other link forms (inline target or
    // reference), then parses the label as inline children.
    let references = state.root_ext.and_then(|ext| ext.get::<ReferenceMap>());
    let candidate = scan_link_candidate(
        state,
        offset,
        enable_nested,
        references,
        options.inline_target_fallback,
    )?;
    // We found the end of the link and know for a fact it's a valid link;
    // all that's left to do is to parse the label contents as inline children.
    let label_range = candidate.label_start..candidate.label_end;
    // Parse the label directly (extensions survive) before allocating the node.
    let children = match options.label_context {
        LabelContext::Independent => state.parse_child(label_range.clone(), 0, 0, |label| {
            label
                .inline_ext
                .insert(super::delimiters::EmphasisSourceStart(label.pos));
        }),
        LabelContext::NestedLink => {
            let child_link_level = state
                .link_level
                .checked_add(1)
                .expect("inline link nesting level overflow");
            state.parse_child(
                label_range.clone(),
                state.depth.saturating_add(1),
                child_link_level,
                |label| {
                    label.inline_ext.insert(MarkdownLinkLabel);
                },
            )
        }
    }?;
    let label_source = (options.label_context == LabelContext::Independent)
        .then(|| IndependentLabelSource(state.remaining()[label_range].to_owned()));
    let node = factory(state.document, candidate.href, candidate.title);
    if let Some(source) = label_source {
        state.document.node_mut(node).ext_mut().insert(source);
    }
    state.document.attach_children(node, children);

    Some((Some(node), candidate.end))
}

#[derive(Debug)]
pub(crate) struct LinkLabelBounds {
    close_positions: Vec<usize>,
    plain_closes: Option<HashMap<usize, usize>>,
    target_positions: Vec<usize>,
    non_memoizable_positions: Option<Vec<usize>>,
    labels: HashMap<LabelScanKey, (usize, usize)>,
    failed_labels: HashSet<(usize, usize, i32, bool)>,
}

impl LinkLabelBounds {
    pub(crate) fn can_memoize_labels(&self, start: usize, end: usize) -> bool {
        let Some(positions) = &self.non_memoizable_positions else {
            return false;
        };
        let next = positions.partition_point(|&pos| pos < start);
        positions.get(next).is_none_or(|&pos| pos >= end)
    }
}

/// Index markers that prevent memoization within a label scan's window.
fn non_memoizable_positions(source: &str, state: &InlineState<'_>) -> Option<Vec<usize>> {
    use crate::plugins::cmark::block::reference::DefaultReferenceMap;

    if state
        .root_ext
        .and_then(|ext| ext.get::<ReferenceMap>())
        .is_some_and(|map| map.downcast_ref::<DefaultReferenceMap>().is_none())
    {
        return None;
    }

    let uncached_rules: Vec<_> = state
        .ruleset
        .checks
        .iter()
        .filter(|rule| !is_memoizable_check(rule.type_id))
        .collect();
    Some(
        source
            .char_indices()
            .filter(|&(_, ch)| uncached_rules.iter().any(|rule| rule.matches_marker(ch)))
            .map(|(pos, _)| pos)
            .collect(),
    )
}

/// Check rules whose `check` is a pure boundary probe.
///
/// A memoized label scan skips re-running checks, so this list must cover every
/// built-in rule that can run inside a label. The
/// `memoizable_checks_cover_shipped_presets` test derives that set from the
/// shipped parsers, so a missing rule fails instead of silently disabling the
/// cache (which would restore the quadratic blowup). Mutable built-ins such as
/// `FootnoteInlineScanner` are deliberately absent.
fn is_memoizable_check(type_id: TypeId) -> bool {
    use crate::parser::inline::TextScanner;
    use crate::parser::inline::helpers::code_pair::CodePairScanner;
    use crate::parser::inline::helpers::emph_pair::EmphPairScanner;
    use crate::plugins::cmark::inline::autolink::AutolinkScanner;
    use crate::plugins::cmark::inline::entity::EntityScanner;
    use crate::plugins::cmark::inline::escape::EscapeScanner;
    use crate::plugins::cmark::inline::newline::NewlineScanner;
    use crate::plugins::directives::TextDirective;
    use crate::plugins::extra::footnote::FootnoteReferenceScanner;
    use crate::plugins::html::html_inline::HtmlInlineScanner;

    [
        TypeId::of::<LinkScanner<false>>(),
        TypeId::of::<LinkPrefixScanner<'!', true>>(),
        TypeId::of::<LinkScannerEnd>(),
        TypeId::of::<TextScanner>(),
        TypeId::of::<FootnoteReferenceScanner>(),
        TypeId::of::<CodePairScanner<'`'>>(),
        TypeId::of::<EmphPairScanner<'*', true>>(),
        TypeId::of::<EmphPairScanner<'_', false>>(),
        TypeId::of::<EmphPairScanner<'~', true>>(),
        TypeId::of::<EmphPairScanner<'=', true>>(),
        TypeId::of::<EscapeScanner>(),
        TypeId::of::<EntityScanner>(),
        TypeId::of::<NewlineScanner>(),
        TypeId::of::<AutolinkScanner>(),
        TypeId::of::<HtmlInlineScanner>(),
        TypeId::of::<TextDirective>(),
        #[cfg(feature = "linkify")]
        TypeId::of::<crate::plugins::extra::linkify::LinkifyScanner>(),
        #[cfg(feature = "linkify")]
        TypeId::of::<crate::plugins::extra::linkify::LinkifyFuzzyScanner>(),
        #[cfg(feature = "linkify")]
        TypeId::of::<crate::plugins::extra::linkify::LinkifyEmailScanner>(),
        #[cfg(feature = "katex")]
        TypeId::of::<crate::plugins::extra::math::MathInlineScanner>(),
    ]
    .contains(&type_id)
}

/// Index label ends directly, bypassing check sessions for plain input.
///
/// Returns `None` when the source contains syntax the index pass cannot
/// handle; label ends must then be scanned normally.
fn plain_label_closes(
    source: &str,
    end: usize,
    rules: &crate::parser::inline::RuleSet,
) -> Option<HashMap<usize, usize>> {
    let source = &source[..end];
    if !is_plain_label_source(source) || !has_only_supported_checks(source, rules) {
        return None;
    }
    Some(index_bracket_closes(source))
}

/// Whether the source is simple enough to index directly.
///
/// Parenthesized text must contain no brackets or image markers: a checked
/// target can hide those bytes and change the raw bracket pairing.
fn is_plain_label_source(source: &str) -> bool {
    let bytes = source.as_bytes();
    let is_plain = |byte: u8| {
        byte.is_ascii_alphanumeric() || byte.is_ascii_whitespace() || b"[]!()".contains(&byte)
    };
    if !bytes.iter().all(|&byte| is_plain(byte)) {
        return false;
    }

    let mut depth = 0usize;
    for &byte in bytes {
        match byte {
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            b'[' | b']' | b'!' if depth != 0 => return false,
            _ => {}
        }
    }
    true
}

/// Whether every check rule triggered by this source is handled by the index pass.
fn has_only_supported_checks(source: &str, rules: &crate::parser::inline::RuleSet) -> bool {
    rules.checks.iter().all(|rule| {
        !source.chars().any(|ch| rule.matches_marker(ch)) || is_index_supported(rule.type_id)
    })
}

fn is_index_supported(type_id: std::any::TypeId) -> bool {
    use std::any::TypeId;
    [
        TypeId::of::<LinkScanner<false>>(),
        TypeId::of::<LinkPrefixScanner<'!', true>>(),
        TypeId::of::<LinkScannerEnd>(),
        TypeId::of::<crate::parser::inline::TextScanner>(),
        TypeId::of::<crate::plugins::extra::footnote::FootnoteReferenceScanner>(),
    ]
    .contains(&type_id)
}

/// Map each `[` byte index to its matching `]`.
fn index_bracket_closes(source: &str) -> HashMap<usize, usize> {
    let mut open = Vec::new();
    let mut closes = HashMap::new();
    for (index, byte) in source.bytes().enumerate() {
        match byte {
            b'[' => open.push(index),
            b']' => {
                if let Some(open) = open.pop() {
                    closes.insert(open, index);
                }
            }
            _ => {}
        }
    }
    closes
}

fn has_possible_link_label_close(
    state: &mut InlineState<'_>,
    offset: usize,
    references: Option<&ReferenceMap>,
) -> bool {
    if !state
        .remaining()
        .get(offset..)
        .is_some_and(|tail| tail.starts_with('['))
    {
        return false;
    }

    let source = state.src.as_ref();
    let end = state.pos_max;
    let mut shared = state.scan_limit_cache.borrow_mut();
    let cache = shared.link_bounds.get_or_insert_with(|| LinkLabelBounds {
        close_positions: source.match_indices(']').map(|(pos, _)| pos).collect(),
        plain_closes: plain_label_closes(source, source.len(), state.ruleset),
        target_positions: source.match_indices('(').map(|(pos, _)| pos).collect(),
        non_memoizable_positions: non_memoizable_positions(source, state),
        labels: HashMap::new(),
        failed_labels: HashSet::new(),
    });

    let open = state.pos + offset;
    let last_before_end = |positions: &[usize]| {
        positions[..positions.partition_point(|&pos| pos < end)]
            .last()
            .copied()
    };
    if !last_before_end(&cache.target_positions).is_some_and(|last| last > open)
        && references.is_none()
    {
        return false;
    }

    if let Some(closes) = &cache.plain_closes {
        let Some(&close) = closes.get(&open).filter(|&&close| close < end) else {
            return last_before_end(&cache.close_positions).is_some_and(|last| last > open);
        };

        if state.depth >= state.markdown_it().max_nesting || source[open + 1..close].contains('[') {
            return true;
        }

        if source[close + 1..end].starts_with(['(', '[']) {
            return true;
        }

        return references.is_some_and(|map| {
            map.downcast_ref::<crate::plugins::cmark::block::reference::DefaultReferenceMap>()
                .is_none_or(|map| {
                    use crate::plugins::cmark::block::reference::CustomReferenceMap;
                    map.get(&source[open + 1..close]).is_some()
                })
        });
    }
    last_before_end(&cache.close_positions).is_some_and(|last| last > open)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod check_label_tests;

#[cfg(test)]
mod cache_tests;
