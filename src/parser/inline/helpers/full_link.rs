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
use crate::parser::inline::{DocumentInlineState, InlineRule};
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
struct DocumentLinkCfg<const PREFIX: char> {
    factory: fn(&mut Document, Option<String>, Option<String>) -> NodeId,
    options: LinkOptions,
}

/// Marks a Markdown link label, which core linkify must skip regardless of
/// inline anchor counter changes from HTML tags.
#[derive(Debug)]
pub(crate) struct MarkdownLinkLabel;

/// Register a link rule with an arena node factory.
pub fn add<const ENABLE_NESTED: bool>(
    md: &mut MarkdownIt,
    factory: fn(&mut Document, Option<String>, Option<String>) -> NodeId,
) {
    md.ext.insert(DocumentLinkCfg::<'\0'> {
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
    md.ext
        .insert(DocumentLinkCfg::<PREFIX> { factory, options });
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

    fn check(context: &mut crate::DocumentInlineState<'_>) -> Option<usize> {
        let options = context
            .markdown_it()
            .ext
            .get::<DocumentLinkCfg<'\0'>>()?
            .options;
        document_link_check(context, ENABLE_NESTED, 0, options.inline_target_fallback)
    }

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        let config = *state
            .markdown_it()
            .ext
            .get::<DocumentLinkCfg<'\0'>>()
            .expect("direct link rule requires an arena node factory");
        document_link_run(state, ENABLE_NESTED, 0, config.options, config.factory)
    }
}

#[doc(hidden)]
pub struct LinkPrefixScanner<const PREFIX: char, const ENABLE_NESTED: bool>;

impl<const PREFIX: char, const ENABLE_NESTED: bool> InlineRule
    for LinkPrefixScanner<PREFIX, ENABLE_NESTED>
{
    const MARKER: char = PREFIX;
    const NAMES: &'static [&'static str] = &["link_prefix"];

    fn check(context: &mut crate::DocumentInlineState<'_>) -> Option<usize> {
        if !context.remaining().starts_with(PREFIX) {
            return None;
        }

        let options = context
            .markdown_it()
            .ext
            .get::<DocumentLinkCfg<PREFIX>>()?
            .options;
        document_link_check(
            context,
            ENABLE_NESTED,
            PREFIX.len_utf8(),
            options.inline_target_fallback,
        )
    }

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        if !state.remaining().starts_with(PREFIX) {
            return None;
        }

        let config = *state
            .markdown_it()
            .ext
            .get::<DocumentLinkCfg<PREFIX>>()
            .expect("direct prefix rule requires an arena node factory");
        document_link_run(
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

    fn run(_state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        None
    }
}

// Scan a link label with independent check sessions.
//
// This function assumes that the first character ("[") already matches;
// it returns the byte length of the label including the closing "]".
fn scan_link_label(
    mut context: crate::parser::inline::DocumentInlineState<'_>,
    enable_nested: bool,
) -> Option<usize> {
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

fn cache_label_scan(context: &DocumentInlineState<'_>, key: LabelScanKey, close: Option<usize>) {
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

#[cfg(test)]
fn parse_inline_link_target(
    md: &MarkdownIt,
    source: &str,
    start: usize,
    max: usize,
) -> Option<InlineLinkTarget> {
    scan_inline_link_target(md, source, start, max).0
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
    context: &mut crate::parser::inline::DocumentInlineState<'_>,
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

fn document_link_check(
    context: &mut crate::parser::inline::DocumentInlineState<'_>,
    enable_nested: bool,
    offset: usize,
    fallback: InlineTargetFallback,
) -> Option<usize> {
    let references = context.root_ext.and_then(|ext| ext.get::<ReferenceMap>());
    match scan_link_candidate(context, offset, enable_nested, references, fallback) {
        Some(candidate) => Some(candidate.end),
        None => None,
    }
}

fn document_link_run(
    state: &mut DocumentInlineState<'_>,
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
    let node = factory(state.document, candidate.href, candidate.title);
    let label_range = candidate.label_start..candidate.label_end;
    let children = if options.label_context == LabelContext::Independent {
        let mut label = state.child_state(label_range, 0, 0)?;
        label
            .inline_ext
            .insert(super::delimiters::EmphasisSourceStart(label.pos));
        // Parse this state directly: another child session would discard the
        // label-local extension. Nested labels retain their own syntax context.
        stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
            label.tokenize();
            label.finish_nodes();
        });
        std::mem::take(label.nodes_mut())
    } else {
        let child_link_level = state
            .link_level
            .checked_add(1)
            .expect("inline link nesting level overflow");
        let mut label =
            state.child_state(label_range, state.depth.saturating_add(1), child_link_level)?;
        label.inline_ext.insert(MarkdownLinkLabel);
        stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
            label.tokenize();
            label.finish_nodes();
        });
        std::mem::take(label.nodes_mut())
    };
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
fn non_memoizable_positions(source: &str, state: &DocumentInlineState<'_>) -> Option<Vec<usize>> {
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
    use crate::parser::inline::builtin::TextScanner;
    use crate::parser::inline::helpers::code_pair::CodePairScanner;
    use crate::parser::inline::helpers::emph_pair::EmphPairScanner;
    use crate::plugins::cmark::inline::autolink::AutolinkScanner;
    use crate::plugins::cmark::inline::entity::EntityScanner;
    use crate::plugins::cmark::inline::escape::EscapeScanner;
    use crate::plugins::cmark::inline::newline::NewlineScanner;
    use crate::plugins::directives::TextDirective;
    use crate::plugins::extra::footnote::FootnoteReferenceScanner;
    use crate::plugins::html::html_inline::HtmlInlineScanner;

    if [
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
    ]
    .contains(&type_id)
    {
        return true;
    }

    #[cfg(feature = "linkify")]
    if [
        TypeId::of::<crate::plugins::extra::linkify::LinkifyScanner>(),
        TypeId::of::<crate::plugins::extra::linkify::LinkifyFuzzyScanner>(),
        TypeId::of::<crate::plugins::extra::linkify::LinkifyEmailScanner>(),
    ]
    .contains(&type_id)
    {
        return true;
    }

    #[cfg(feature = "katex")]
    if type_id == TypeId::of::<crate::plugins::extra::math::MathInlineScanner>() {
        return true;
    }

    false
}

/// Built-in check that is deliberately left uncached: its `check` mutates the
/// shared inline-footnote scan budget, so the same probe can return different
/// results under budget pressure.
#[cfg(test)]
fn is_known_uncached_check(type_id: TypeId) -> bool {
    type_id == TypeId::of::<crate::plugins::extra::footnote::FootnoteInlineScanner>()
}

/// Index label ends directly, bypassing check sessions for plain input.
///
/// Returns `None` when the source contains syntax the index pass cannot
/// handle; label ends must then be scanned normally.
fn plain_label_closes(
    source: &str,
    end: usize,
    rules: &crate::parser::inline::DocumentRuleSet,
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
fn has_only_supported_checks(source: &str, rules: &crate::parser::inline::DocumentRuleSet) -> bool {
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
        TypeId::of::<crate::parser::inline::builtin::TextScanner>(),
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
    state: &mut DocumentInlineState<'_>,
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
mod tests {
    use super::*;

    fn parse_target(source: &str) -> Option<InlineLinkTarget> {
        let md = MarkdownIt::empty();
        if !source.starts_with('(') {
            return None;
        }
        let pos = skip_link_spaces(source, 1, source.len());
        parse_inline_link_target(&md, source, pos, source.len())
    }

    #[test]
    fn parses_empty_destination() {
        let source = "()";
        let target = parse_target(source).unwrap();
        assert_eq!(target.href.as_deref(), Some(""));
        assert_eq!(target.title, None);
        assert_eq!(target.end, source.len());
    }

    #[test]
    fn missing_target_content_respects_the_parse_window() {
        let md = MarkdownIt::empty();
        let source = "雪( \t\n/url)";
        let start = "雪".len();
        for max in start + 1..=source.find('/').unwrap() {
            let target_start = skip_link_spaces(source, start + 1, max);
            assert!(parse_inline_link_target(&md, source, target_start, max).is_none());
        }
        let target_start = skip_link_spaces(source, start + 1, source.len());
        let target = parse_inline_link_target(&md, source, target_start, source.len()).unwrap();
        assert_eq!(target.href.as_deref(), Some("/url"));
    }

    #[test]
    fn parses_angle_destination_and_single_quoted_title() {
        let source = "(<a> 't')";
        let target = parse_target(source).unwrap();
        assert_eq!(target.href.as_deref(), Some("a"));
        assert_eq!(target.title.as_deref(), Some("t"));
        assert_eq!(target.end, source.len());
    }

    #[test]
    fn inline_title_requires_whitespace_after_destination() {
        for title in ["\"title\"", "'title'", "(title)"] {
            assert!(parse_target(&format!("(<url>{title})")).is_none());
            for whitespace in [" ", "\t", "\n"] {
                let source = format!("(<url>{whitespace}{title})");
                let target = parse_target(&source).unwrap();
                assert_eq!(target.href.as_deref(), Some("url"));
                assert_eq!(target.title.as_deref(), Some("title"));
                assert_eq!(target.end, source.len());
            }
        }
    }

    #[test]
    fn parses_escaped_parentheses_in_destination() {
        let source = r"(foo\)bar)";
        let target = parse_target(source).unwrap();
        assert_eq!(target.href.as_deref(), Some("foo)bar"));
        assert_eq!(target.end, source.len());

        let source = r"(foo\(bar)";
        let target = parse_target(source).unwrap();
        assert_eq!(target.href.as_deref(), Some("foo(bar"));
        assert_eq!(target.end, source.len());
    }

    #[test]
    fn rejects_unclosed_destination() {
        assert!(parse_target("(/url").is_none());
        assert!(parse_target("(<a").is_none());
    }

    #[test]
    fn rejects_unclosed_title() {
        assert!(parse_target(r#"(/url "title)"#).is_none());
        assert!(parse_target("(/url 'title)").is_none());
    }

    #[test]
    fn rejects_javascript_protocol() {
        assert!(parse_target("(javascript:alert(1))").is_none());
        assert!(parse_target("(JAVASCRIPT:alert(1))").is_none());
    }

    #[test]
    fn inline_target_respects_byte_range() {
        let md = MarkdownIt::empty();
        let source = "雪(/url \"title\")雨";
        let start = "雪".len();
        let max = source.len() - "雨".len();
        let target_start = skip_link_spaces(source, start + 1, max);
        let target = parse_inline_link_target(&md, source, target_start, max).unwrap();
        assert_eq!(target.href.as_deref(), Some("/url"));
        assert_eq!(target.title.as_deref(), Some("title"));
        assert_eq!(target.end, max);
        assert!(parse_inline_link_target(&md, source, target_start, max - 1).is_none());
    }

    #[test]
    fn reference_candidate_selects_label_and_consumed_end() {
        let mut references = ReferenceMap::default();
        references.insert("a".into(), "/shortcut".into(), None);
        references.insert("b".into(), "/full".into(), Some("title".into()));

        for (source, reference_end, href, title, end) in [
            ("[a]", None, "/shortcut", None, 3),
            ("[a][]", Some(4), "/shortcut", None, 5),
            ("[a][b]", Some(5), "/full", Some("title"), 6),
            ("[a][", None, "/shortcut", None, 3),
            ("[a](/unfinished", None, "/shortcut", None, 3),
        ] {
            let result = resolve_reference_link(
                source,
                1,
                2,
                reference_end.map(|end| (4, end)),
                &references,
            )
            .expect(source);
            assert_eq!((result.label_start, result.label_end), (1, 2));
            assert_eq!(result.href.as_deref(), Some(href), "{source}");
            assert_eq!(result.title.as_deref(), title, "{source}");
            assert_eq!(result.end, end, "{source}");
        }

        // A missing explicit reference must not fall back to the first label.
        assert!(resolve_reference_link("[a][missing]", 1, 2, Some((4, 11)), &references).is_none());
    }
}

#[cfg(test)]
mod check_label_tests {
    use super::*;
    use crate::document::Text;
    use crate::parser::inline::InlineRule;
    use crate::plugins::cmark::inline::link::Link;
    use crate::{DocumentInlineState, MarkdownIt, NodeId};

    // Register both brackets so the built-in text classifier stops at them.
    struct Bracket<const C: char>;
    impl<const C: char> InlineRule for Bracket<C> {
        const MARKER: char = C;
        fn check(_: &mut DocumentInlineState<'_>) -> Option<usize> {
            None
        }
        fn run(_: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
            panic!("the consumer must consume the complete test input")
        }
    }

    struct Consumer<const NESTED: bool>;
    impl<const NESTED: bool> InlineRule for Consumer<NESTED> {
        const MARKER: char = '@';
        fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
            if !state.remaining().starts_with("@[") {
                return None;
            }
            let len = state.remaining().len();
            let expected_source = state.remaining()[1..].to_owned();
            let mut parent = state
                .child_state(1..len, state.depth.saturating_add(1), state.link_level)
                .unwrap();
            let result = parent
                .child_state(
                    1..parent.remaining().len(),
                    parent.depth.saturating_add(1),
                    parent.link_level,
                )
                .and_then(|child| scan_link_label(child, NESTED));
            assert_eq!(parent.remaining(), expected_source);
            assert_eq!(parent.trailing_text(), "");
            assert_eq!(parent.link_level, 0);
            let content = match result {
                Some(end) => format!("end={end}"),
                None => "none".to_owned(),
            };
            Some((Some(state.document.create_node(Text { content })), len))
        }
    }

    fn parser<const NESTED: bool>() -> MarkdownIt {
        let mut md = MarkdownIt::empty();
        crate::plugins::cmark::block::paragraph::add(&mut md);
        md.inline
            .add_rule::<crate::parser::inline::builtin::TextScanner>()
            .before_all();
        md.inline.add_rule::<Bracket<'['>>();
        md.inline.add_rule::<Bracket<']'>>();
        md.inline.add_rule::<Consumer<NESTED>>();
        crate::plugins::cmark::inline::escape::add(&mut md);
        crate::plugins::cmark::inline::backticks::add(&mut md);
        crate::plugins::cmark::inline::entity::add(&mut md);
        crate::plugins::html::html_inline::add(&mut md);
        md
    }

    fn render(md: &MarkdownIt, source: &str) -> String {
        md.render(source)
    }

    /// Label bodies shared by the raw-label and combined-candidate tests.
    ///
    /// A `]` inside a code span, escape, entity, or raw HTML must not close
    /// the label; raw nested `[` bytes still increase the bracket level.
    const LABELS: &[&str] = &[
        "",
        "abc",
        "雪[a]雨",
        "`]`",
        r"\]",
        "&#93;",
        "<i x=']'>",
        "<a>x",
    ];

    #[test]
    fn labels_respect_raw_brackets_and_opaque_spans() {
        let md = parser::<false>();
        for label in LABELS {
            let source = format!("@[{label}]");
            assert_eq!(
                render(&md, &source),
                format!("<p>end={}</p>\n", label.len()),
                "{source}"
            );
        }
        assert_eq!(render(&md, "@[a[b]"), "<p>none</p>\n");
        assert_eq!(render(&md, "@[abc"), "<p>none</p>\n");
    }

    struct DefaultCheck;
    impl InlineRule for DefaultCheck {
        const MARKER: char = '?';
        fn run(_: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
            None
        }
    }

    struct Closing;
    impl InlineRule for Closing {
        const MARKER: char = ']';
        fn check(_: &mut DocumentInlineState<'_>) -> Option<usize> {
            panic!("terminal bracket must be checked before dispatch")
        }
        fn run(_: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
            panic!("consumer owns input")
        }
    }

    #[test]
    fn terminal_close_precedes_dispatch_and_does_not_scan_suffix() {
        let mut md = parser::<false>();
        md.inline.add_rule::<Closing>();
        md.inline.add_rule::<DefaultCheck>();
        assert_eq!(render(&md, "@[x]?"), "<p>end=1</p>\n");
        assert_eq!(render(&md, "@[?]"), "<p>end=1</p>\n");
    }

    struct BracketToken;
    impl InlineRule for BracketToken {
        const MARKER: char = '[';
        fn check(context: &mut DocumentInlineState<'_>) -> Option<usize> {
            if context.remaining().starts_with("[x]") {
                // Legacy uses consumed length, even if a custom rule calls it Text.
                Some(3)
            } else {
                None
            }
        }
        fn run(_: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
            panic!("consumer owns input")
        }
    }

    #[test]
    fn nested_flag_preserves_consumption_rule() {
        let mut no = parser::<false>();
        no.inline.add_rule::<BracketToken>();
        assert_eq!(render(&no, "@[[x]]"), "<p>none</p>\n");
        let mut yes = parser::<true>();
        yes.inline.add_rule::<BracketToken>();
        assert_eq!(render(&yes, "@[[x]]"), "<p>end=3</p>\n");
    }

    #[test]
    fn recursive_entry_at_depth_limit_yields_no_label() {
        let mut md = parser::<false>();
        md.max_nesting = 1;
        // An immediately closing bracket does not call `skipToken`, so it is
        // still accepted at the nesting limit.
        assert_eq!(render(&md, "@[]"), "<p>end=0</p>\n");
        // A non-empty label must be scanned with `skipToken`, which records
        // the failure once the nesting limit is reached.
        assert_eq!(render(&md, "@[x]"), "<p>none</p>\n");
    }

    #[test]
    fn registered_link_and_image_check_respect_remaining_depth() {
        let mut document = crate::Document::new("", crate::Root::new(""));
        let mut md = MarkdownIt::empty();
        crate::plugins::cmark::inline::link::add(&mut md);
        crate::plugins::cmark::inline::image::add(&mut md);
        md.max_nesting = 3;
        let rules = md.inline.document_rules();

        for source in ["[雪](/url)", "[](/url)", "![雪](/img)", "![](/img)"] {
            for depth in [1, 2, 3] {
                let mut context = DocumentInlineState::new(
                    &mut document,
                    source,
                    0,
                    source.len(),
                    &md,
                    &rules,
                    depth,
                    0,
                );
                let result = if source.starts_with('!') {
                    <LinkPrefixScanner<'!', true> as InlineRule>::check(&mut context)
                } else {
                    <LinkScanner<false> as InlineRule>::check(&mut context)
                };
                // `depth` is the rule level of the check; a non-empty label
                // is rejected once that level reaches `maxNesting`. An empty
                // label closes before `skipToken` and is still accepted.
                let empty_label = source.starts_with("[]") || source.starts_with("![]");
                let expected = if depth < 3 || empty_label {
                    Some(source.len())
                } else {
                    None
                };
                assert_eq!(result, expected, "depth={depth}, source={source:?}");
                assert_eq!(context.remaining(), source);
                assert_eq!(context.depth, depth);
                assert_eq!(context.link_level, 0);
            }
        }
    }

    fn utf8_parser<const PREFIX: char>() -> MarkdownIt {
        let mut md = MarkdownIt::empty();
        crate::plugins::cmark::add(&mut md);
        super::add_prefix::<PREFIX, true>(&mut md, |document, href, title| {
            document.create_node(Link {
                url: href.unwrap_or_default(),
                title,
            })
        });
        md
    }

    #[test]
    fn custom_prefix_policies_are_independent_of_marker_and_label_context() {
        fn verify<const PREFIX: char>() {
            for fallback in [
                InlineTargetFallback::Reference,
                InlineTargetFallback::Reject,
            ] {
                for label_context in [LabelContext::NestedLink, LabelContext::Independent] {
                    let mut md = utf8_parser::<PREFIX>();
                    add_prefix_with_options::<PREFIX, true>(
                        &mut md,
                        LinkOptions {
                            inline_target_fallback: fallback,
                            label_context,
                        },
                        |document, href, title| {
                            document.create_node(Link {
                                url: href.unwrap_or_default(),
                                title,
                            })
                        },
                    );
                    let source = format!("{PREFIX}[foo](/unfinished");
                    let mut references = ReferenceMap::default();
                    references.insert("foo".into(), "/url".into(), None);
                    let mut root_ext = crate::common::extset::RootExtSet::default();
                    root_ext.insert(references);
                    let mut document = crate::Document::new("", crate::Root::new(""));
                    let rules = md.inline.document_rules();
                    let mut context = DocumentInlineState::new(
                        &mut document,
                        &source,
                        0,
                        source.len(),
                        &md,
                        &rules,
                        0,
                        0,
                    );
                    context.root_ext = Some(&root_ext);
                    let expected_end = match fallback {
                        InlineTargetFallback::Reference => Some(PREFIX.len_utf8() + "[foo]".len()),
                        InlineTargetFallback::Reject => None,
                    };
                    assert_eq!(
                        LinkPrefixScanner::<PREFIX, true>::check(&mut context),
                        expected_end
                    );
                    let retained_prefix = if fallback == InlineTargetFallback::Reject {
                        PREFIX.to_string()
                    } else {
                        String::new()
                    };
                    assert_eq!(
                        md.render(&format!("{source}\n\n[foo]: /url\n")),
                        format!("<p>{retained_prefix}<a href=\"/url\">foo</a>(/unfinished</p>\n"),
                    );
                    assert_eq!(
                        md.render(&format!("{PREFIX}[foo](/inline)")),
                        "<p><a href=\"/inline\">foo</a></p>\n",
                    );
                    assert_eq!(
                        md.render(&format!("{PREFIX}[foo][ref]\n\n[ref]: /url\n")),
                        "<p><a href=\"/url\">foo</a></p>\n",
                    );
                }
            }
        }
        verify::<'~'>();
        verify::<'雪'>();
        verify::<'!'>();
    }

    #[test]
    fn custom_prefix_uses_utf8_byte_length() {
        for (source, html) in [
            ("雪[文字](/url)", "<p><a href=\"/url\">文字</a></p>\n"),
            (
                "雪[文字][ref]\n\n[ref]: /url",
                "<p><a href=\"/url\">文字</a></p>\n",
            ),
            ("雪[文字]", "<p>雪[文字]</p>\n"),
        ] {
            assert_eq!(utf8_parser::<'雪'>().render(source), html, "{source}");
        }

        assert_eq!(
            utf8_parser::<'😀'>().render("😀[x](/url)"),
            "<p><a href=\"/url\">x</a></p>\n",
        );
        assert_eq!(
            utf8_parser::<'~'>().render("~[x](/url)"),
            "<p><a href=\"/url\">x</a></p>\n",
        );
    }

    #[test]
    fn label_scan_checks_unicode_prefixed_rule() {
        // Image label scanning calls the custom rule's check before its run.
        // The nested custom link contributes its text to the image alt value.
        assert_eq!(
            utf8_parser::<'雪'>().render("![雪[x](/inner)](/image)"),
            "<p><img src=\"/image\" alt=\"x\"></p>\n",
        );
    }

    #[test]
    fn candidate_combines_label_target_and_reference() {
        let mut document = crate::Document::new("", crate::Root::new(""));
        let md = parser::<false>();
        let ruleset = md.inline.document_rules();
        let mut references = ReferenceMap::default();
        references.insert("x".into(), "/shortcut".into(), None);
        references.insert("ref".into(), "/ref".into(), Some("t".into()));

        for (source, href, title, end) in [
            ("[x](/inline)", "/inline", None, 12),
            ("[x][ref]", "/ref", Some("t"), 8),
            ("[x][]", "/shortcut", None, 5),
            ("[x]", "/shortcut", None, 3),
            ("[x][", "/shortcut", None, 3),
            ("[x](/unfinished", "/shortcut", None, 3),
        ] {
            let mut context = DocumentInlineState::new(
                &mut document,
                source,
                0,
                source.len(),
                &md,
                &ruleset,
                0,
                0,
            );
            let result = scan_link_candidate(
                &mut context,
                0,
                false,
                Some(&references),
                InlineTargetFallback::Reference,
            )
            .expect(source);
            assert_eq!((result.label_start, result.label_end), (1, 2));
            assert_eq!(result.href.as_deref(), Some(href), "{source}");
            assert_eq!(result.title.as_deref(), title, "{source}");
            assert_eq!(result.end, end, "{source}");
            assert_eq!(context.remaining(), source);
            assert_eq!(context.trailing_text(), "");
            assert_eq!(context.link_level, 0);
        }

        for source in [
            "[x][missing]",
            "[missing]",
            "[unclosed",
            "[x](",
            "[x]( \t\n",
        ] {
            let mut context = DocumentInlineState::new(
                &mut document,
                source,
                0,
                source.len(),
                &md,
                &ruleset,
                0,
                0,
            );
            assert!(
                scan_link_candidate(
                    &mut context,
                    0,
                    false,
                    Some(&references),
                    InlineTargetFallback::Reference
                )
                .is_none()
            );
        }
    }

    #[test]
    fn candidate_offsets_are_relative_to_current_window() {
        let mut document = crate::Document::new("", crate::Root::new(""));
        let md = parser::<false>();
        let ruleset = md.inline.document_rules();
        let source = "前雪[x](/url)";
        let mut context = DocumentInlineState::new(
            &mut document,
            source,
            "前".len(),
            source.len(),
            &md,
            &ruleset,
            0,
            0,
        );
        let result = scan_link_candidate(
            &mut context,
            "雪".len(),
            false,
            None,
            InlineTargetFallback::Reference,
        )
        .unwrap();
        assert_eq!(
            (result.label_start, result.label_end, result.end),
            (4, 5, 12)
        );
        assert_eq!(result.href.as_deref(), Some("/url"));
        assert_eq!(context.remaining(), "雪[x](/url)");
        assert!(
            scan_link_candidate(
                &mut context,
                1,
                false,
                None,
                InlineTargetFallback::Reference
            )
            .is_none()
        );
    }

    #[test]
    fn candidate_combines_opaque_labels_with_target() {
        let mut document = crate::Document::new("", crate::Root::new(""));
        let md = parser::<false>();
        let ruleset = md.inline.document_rules();
        for label in LABELS {
            let source = format!("[{label}](/url)");
            let mut context = DocumentInlineState::new(
                &mut document,
                &source,
                0,
                source.len(),
                &md,
                &ruleset,
                0,
                0,
            );
            let result = scan_link_candidate(
                &mut context,
                0,
                false,
                None,
                InlineTargetFallback::Reference,
            )
            .unwrap_or_else(|| panic!("{source}"));
            // `]` bytes inside opaque spans must not shorten the label.
            assert_eq!(
                (result.label_start, result.label_end),
                (1, label.len() + 1),
                "{source}"
            );
            assert_eq!(result.href.as_deref(), Some("/url"), "{source}");
            assert_eq!(result.title, None, "{source}");
            assert_eq!(result.end, source.len(), "{source}");
        }
    }

    #[test]
    fn candidate_requires_reference_map_and_label_window() {
        let mut document = crate::Document::new("", crate::Root::new(""));
        let md = parser::<false>();
        let ruleset = md.inline.document_rules();

        // Inline targets parse without references, shortcut references do not.
        let source = "[x](/url)";
        let mut context =
            DocumentInlineState::new(&mut document, source, 0, source.len(), &md, &ruleset, 0, 0);
        let result = scan_link_candidate(
            &mut context,
            0,
            false,
            None,
            InlineTargetFallback::Reference,
        )
        .expect(source);
        assert_eq!(result.href.as_deref(), Some("/url"));
        assert_eq!(result.end, source.len());

        for source in ["[x]", "[x][ref]"] {
            let mut context = DocumentInlineState::new(
                &mut document,
                source,
                0,
                source.len(),
                &md,
                &ruleset,
                0,
                0,
            );
            assert!(
                scan_link_candidate(
                    &mut context,
                    0,
                    false,
                    None,
                    InlineTargetFallback::Reference
                )
                .is_none(),
                "{source}"
            );
        }

        // Label scanning consumes one nesting level; when the budget is spent
        // the whole candidate is rejected even though the syntax is valid.
        let mut md = parser::<false>();
        md.max_nesting = 2;
        let ruleset = md.inline.document_rules();
        let source = "[x](/url)";
        let mut root =
            DocumentInlineState::new(&mut document, source, 0, source.len(), &md, &ruleset, 0, 0);
        assert!(
            scan_link_candidate(&mut root, 0, false, None, InlineTargetFallback::Reference)
                .is_some()
        );

        let mut nested = root
            .child_state(
                0..source.len(),
                root.depth.saturating_add(2),
                root.link_level,
            )
            .unwrap();
        assert_eq!(nested.depth, 2);
        assert!(
            scan_link_candidate(&mut nested, 0, false, None, InlineTargetFallback::Reference)
                .is_none()
        );
    }

    #[test]
    fn candidate_respects_window_edges_and_parent_cursor() {
        let mut document = crate::Document::new("", crate::Root::new(""));
        let md = parser::<false>();
        let ruleset = md.inline.document_rules();

        // The full source parses, but the window ends before the closing
        // parenthesis; bytes after `remaining()` must stay invisible.
        let source = "[x](/url)tail";
        let close = source.find(')').unwrap();
        let mut truncated =
            DocumentInlineState::new(&mut document, source, 0, close, &md, &ruleset, 0, 0);
        assert_eq!(truncated.remaining(), "[x](/url");
        assert!(
            scan_link_candidate(
                &mut truncated,
                0,
                false,
                None,
                InlineTargetFallback::Reference
            )
            .is_none()
        );

        // Including the closing parenthesis makes the same window parse.
        let mut complete =
            DocumentInlineState::new(&mut document, source, 0, close + 1, &md, &ruleset, 0, 0);
        let result = scan_link_candidate(
            &mut complete,
            0,
            false,
            None,
            InlineTargetFallback::Reference,
        )
        .unwrap();
        assert_eq!(result.end, close + 1);

        // After the parent cursor advances, results stay relative to
        // `remaining()`, not to the start of the session.
        let source = "前雪[x](/url)";
        let mut context = DocumentInlineState::new(
            &mut document,
            source,
            "前".len(),
            source.len(),
            &md,
            &ruleset,
            0,
            0,
        );
        context.skip_token().unwrap();
        assert_eq!(context.remaining(), "[x](/url)");
        let result = scan_link_candidate(
            &mut context,
            0,
            false,
            None,
            InlineTargetFallback::Reference,
        )
        .unwrap();
        assert_eq!(
            (result.label_start, result.label_end, result.end),
            (1, 2, 9)
        );
        assert_eq!(result.href.as_deref(), Some("/url"));
        assert_eq!(context.remaining(), "[x](/url)");
    }

    #[test]
    fn registered_link_check_does_not_require_running_the_factory() {
        let mut document = crate::Document::new("", crate::Root::new(""));
        let mut md = MarkdownIt::empty();
        add::<false>(&mut md, |_, _, _| {
            panic!("check must not call arena node factory")
        });
        let ruleset = md.inline.document_rules();
        let source = "[x](/url)";
        let mut context =
            DocumentInlineState::new(&mut document, source, 0, source.len(), &md, &ruleset, 0, 0);
        let token = context.skip_token().unwrap();
        assert_eq!(token, source.len());
        assert_eq!(context.link_level, 0);
        assert!(context.skip_token().is_none());
    }

    #[test]
    fn registered_prefix_check_does_not_require_running_the_factory() {
        let mut document = crate::Document::new("", crate::Root::new(""));
        let mut md = MarkdownIt::empty();
        add_prefix::<'雪', true>(&mut md, |_, _, _| {
            panic!("check must not call arena node factory")
        });
        let ruleset = md.inline.document_rules();
        let source = "雪[x](/url)";
        let mut context =
            DocumentInlineState::new(&mut document, source, 0, source.len(), &md, &ruleset, 0, 0);
        let token = context.skip_token().unwrap();
        assert_eq!(token, source.len());
        assert_eq!(context.link_level, 0);
        assert!(context.skip_token().is_none());
    }

    #[test]
    fn direct_prefix_uses_utf8_byte_offsets() {
        fn check<const PREFIX: char>() {
            let mut md = MarkdownIt::empty();
            crate::plugins::cmark::block::paragraph::add(&mut md);
            crate::plugins::cmark::inline::link::add(&mut md);
            add_prefix::<PREFIX, true>(&mut md, |document, href, title| {
                document.create_node(Link {
                    url: href.unwrap_or_default(),
                    title,
                })
            });
            // Expected outputs captured from the pre-migration legacy parser.
            let cases = [
                (
                    format!("{PREFIX}[雪](/url)"),
                    "<p><a href=\"/url\">雪</a></p>\n".to_owned(),
                    1,
                ),
                (
                    format!("[a {PREFIX}[雪](/in)](/out)"),
                    "<p><a href=\"/out\">a <a href=\"/in\">雪</a></a></p>\n".to_owned(),
                    2,
                ),
                (
                    format!("{PREFIX}[雪](/unfinished"),
                    format!("<p>{PREFIX}[雪](/unfinished</p>\n"),
                    0,
                ),
            ];
            for (source, expected, link_count) in cases {
                let document = md.parse_document(&source);
                assert_eq!(md.render_document(&document), expected, "{source}");
                let links = document
                    .events(document.root())
                    .filter(|event| {
                        matches!(event, crate::StructuralEvent::Enter(_))
                            && event.node().is::<Link>()
                    })
                    .count();
                assert_eq!(links, link_count, "{source}");
                if link_count > 0 {
                    let start = source.find("[雪]").unwrap() + 1;
                    let text_ranges: Vec<_> = document
                        .events(document.root())
                        .filter_map(|event| {
                            let node = event.node();
                            node.cast::<crate::document::Text>()
                                .filter(|text| text.content == "雪")
                                .map(|_| node.srcmap().unwrap().get_byte_offsets())
                        })
                        .collect();
                    assert_eq!(text_ranges, [(start, start + "雪".len())], "{source}");
                }
            }
        }
        check::<'~'>();
        check::<'雪'>();
        check::<'🦀'>();
    }
}

#[cfg(test)]
#[test]
fn plain_brackets_have_a_linear_close_index() {
    let mut md = MarkdownIt::new();
    crate::plugins::html::add(&mut md);
    crate::plugins::extra::add(&mut md);
    let rules = md.inline.document_rules();
    let closes = plain_label_closes("![[]()", 6, &rules)
        .expect("plain cmark brackets support a close index");
    assert!(!closes.contains_key(&1));
    assert_eq!(closes.get(&2), Some(&3));
    assert!(plain_label_closes("[[a]](x)", 8, &rules).is_some());
    for source in ["[a](x[y])", "[a](x!y)", "[a](x\\)y)"] {
        assert!(plain_label_closes(source, source.len(), &rules).is_none());
    }
}

#[cfg(test)]
#[test]
fn indexed_labels_preserve_nested_link_and_image_rendering() {
    let md = MarkdownIt::new();
    for (source, expected) in [
        ("[[[a]]](x)", "<p><a href=\"x\">[[a]]</a></p>\n"),
        ("[[[a](x)]]", "<p>[[<a href=\"x\">a</a>]]</p>\n"),
        (
            "[![a](x)](y)",
            "<p><a href=\"y\"><img src=\"x\" alt=\"a\"></a></p>\n",
        ),
        (
            "[r]: x\n\n[[[a]]] [r]",
            "<p>[[[a]]] <a href=\"x\">r</a></p>\n",
        ),
        ("[r]: x\n\n[[a]][r]", "<p><a href=\"x\">[a]</a></p>\n"),
    ] {
        assert_eq!(md.render(source), expected, "{source}");
    }
}

#[cfg(test)]
#[test]
fn memoizable_checks_cover_shipped_presets() {
    fn assert_all_memoizable(md: &MarkdownIt, preset: &str) {
        for rule in &md.inline.document_rules().checks {
            assert!(
                is_memoizable_check(rule.type_id) || is_known_uncached_check(rule.type_id),
                "{preset}: check rule for marker {:?} is neither memoizable nor a known \
                 uncached rule; update `is_memoizable_check`",
                rule.marker,
            );
        }
    }

    assert_all_memoizable(&MarkdownIt::new(), "MarkdownItDefault");

    let mut extras = MarkdownIt::empty();
    crate::plugins::cmark::add(&mut extras);
    crate::plugins::html::add(&mut extras);
    crate::plugins::extra::add(&mut extras);
    assert_all_memoizable(&extras, "cmark+html+extra");
}

#[cfg(test)]
#[test]
fn memoized_labels_match_uncached_checks_across_nesting_limits() {
    struct Uncached;
    impl InlineRule for Uncached {
        const MARKER: char = '\0';
        fn run(_: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
            None
        }
    }

    let mut cached = MarkdownIt::new();
    crate::plugins::html::add(&mut cached);
    crate::plugins::extra::add(&mut cached);
    let mut uncached = MarkdownIt::new();
    crate::plugins::html::add(&mut uncached);
    crate::plugins::extra::add(&mut uncached);
    // A custom check disables both optimizations, without changing syntax.
    uncached.inline.add_rule::<Uncached>().before_all();

    for depth in [0, 1, 2, 3, 8, 20] {
        cached.max_nesting = depth;
        uncached.max_nesting = depth;
        for body in [
            "a",
            "雪",
            "*a*",
            "`[`",
            "`]`",
            "\\]",
            "<b>]</b>",
            "<a href=x>a</a>",
            "![a](x)",
            "[a](x)",
            "[r]",
            "&amp;",
            "^[note]",
            "^[^[nested]]",
        ] {
            for suffix in ["(x)", "[r]", "(/unfinished", ""] {
                for prefix in ["雪", "^"] {
                    let source = format!(
                        "[r]: /u\n\n{prefix}[[[{body}]]]{suffix} *[[[{body}]]]{suffix} ^[tail]"
                    );
                    assert_eq!(
                        cached.render(&source),
                        uncached.render(&source),
                        "depth={depth}, source={source:?}"
                    );
                }
            }
        }
    }
}
