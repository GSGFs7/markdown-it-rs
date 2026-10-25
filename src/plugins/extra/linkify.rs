//! Find urls and emails, and turn them into links

// TODO: refactor this

use std::cmp::Ordering;
use std::collections::HashMap;

use linkify::{LinkKind, Linkify};

use crate::MarkdownIt;
use crate::document::{Document, NodeId, NodeRef, StructuralEvent, Text, TextSpecial};
use crate::links::LinkFormatter;
use crate::parser::core::{CoreRule, CoreState};
use crate::parser::inline::{InlineParserRule, InlinePostprocessRanges, InlineRule, InlineState};
use crate::plugins::html::anchor_delta;
use crate::render::{NodeRenderer, RenderContext, TransparentRenderer, render_html_link};

#[derive(Debug)]
pub struct Linkified {
    pub url: String,
}

struct LinkifiedRenderer;

impl NodeRenderer<Linkified> for LinkifiedRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        link: &Linkified,
        context: &mut RenderContext<'_>,
        output: &mut crate::Writer,
    ) {
        render_html_link(node, &link.url, None, context, output);
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LinkifyOptions {
    /// Recognize URLs without an explicit scheme, such as `example.org`.
    ///
    /// Disabled by default to match `linkify-it` 6.
    pub fuzzy_links: bool,
}

pub fn add(md: &mut MarkdownIt) {
    add_with_options(md, LinkifyOptions::default());
}

pub fn add_with_options(md: &mut MarkdownIt, options: LinkifyOptions) {
    md.ext.insert(options);
    md.add_rule::<LinkifyPrescan>()
        .before::<InlineParserRule>()
        .before_all();
    md.add_rule::<LinkifyRule>()
        .after::<InlineParserRule>()
        .before_all();
    md.inline.add_rule::<LinkifyScanner>();
    md.inline.add_rule::<LinkifyFuzzyScanner>();
    md.inline.add_rule::<LinkifyEmailScanner>();
    md.add_renderer::<Linkified, _>("html", LinkifiedRenderer);
    md.add_renderer::<Linkified, _>("text", TransparentRenderer);
}

/// Document-level linkification after inline parsing and emphasis cleanup.
#[derive(Default)]
pub struct LinkifyRule;

impl CoreRule for LinkifyRule {
    const NAMES: &'static [&'static str] = &["linkify"];

    fn run(&self, state: &mut CoreState<'_>) {
        let document = &mut *state.document;
        let ranges = document
            .node_mut(document.root())
            .ext_mut()
            .remove::<InlinePostprocessRanges>()
            .unwrap_or_default();
        let fuzzy = state
            .md
            .ext
            .get::<LinkifyOptions>()
            .copied()
            .unwrap_or_default()
            .fuzzy_links;
        for roots in ranges.0 {
            let roots: Vec<_> = roots
                .into_iter()
                .filter(|&id| {
                    document
                        .get_node(id)
                        .is_some_and(|node| node.parent().is_some())
                })
                .collect();
            let events = collect_linkify_events(document, &roots);
            let replacements = collect_linkify_replacements(
                document,
                &events,
                state.md.link_formatter.as_ref(),
                fuzzy,
            );
            apply_linkify_replacements(document, replacements);
        }
    }
}

#[derive(Debug)]
struct LinkifyPositions(Vec<LinkifyPosition>);

fn collect_linkify_events(document: &Document, roots: &[NodeId]) -> Vec<(NodeId, i32)> {
    use crate::plugins::cmark::inline::autolink::Autolink;
    use crate::plugins::cmark::inline::backticks::CodeInline;
    use crate::plugins::cmark::inline::image::Image;
    use crate::plugins::cmark::inline::link::Link;
    use crate::plugins::html::html_inline::HtmlInline;

    let mut events = Vec::new();
    let mut protected_depth = 0;
    for &root in roots {
        for event in document.events(root) {
            let node = event.node();
            let protected = node.is::<CodeInline>()
                || node.is::<Link>()
                || node.is::<Autolink>()
                || node.is::<Linkified>()
                || node.is::<Image>();
            match event {
                StructuralEvent::Enter(_) if protected => protected_depth += 1,
                StructuralEvent::Exit(_) if protected => protected_depth -= 1,
                StructuralEvent::Leaf(_) if protected_depth == 0 && !protected => {
                    if let Some(html) = node.cast::<HtmlInline>() {
                        events.push((node.id(), anchor_delta(&html.content)));
                    } else {
                        events.push((node.id(), 0));
                    }
                }
                StructuralEvent::Enter(_) | StructuralEvent::Exit(_) if protected_depth == 0 => {
                    events.push((node.id(), 0));
                }
                _ => {}
            }
        }
    }

    events
}

type LinkifyReplacements = HashMap<Option<NodeId>, HashMap<NodeId, Vec<NodeId>>>;

fn collect_linkify_replacements(
    document: &mut Document,
    events: &[(NodeId, i32)],
    formatter: &dyn LinkFormatter,
    fuzzy: bool,
) -> LinkifyReplacements {
    let mut reverse_level = 0;
    let mut replacements: LinkifyReplacements = HashMap::new();
    for index in (0..events.len()).rev() {
        let (id, delta) = events[index];
        if delta < 0 {
            reverse_level += 1;
        } else if delta > 0 && reverse_level > 0 {
            reverse_level -= 1;
        }

        if reverse_level > 0 {
            continue;
        }

        let previous = index.checked_sub(1).map(|index| events[index].0);
        if let Some(nodes) = linkify_text_node(document, id, previous, formatter, fuzzy) {
            replacements
                .entry(document.parent(id))
                .or_default()
                .insert(id, nodes);
        }
    }
    replacements
}

fn linkify_text_node(
    document: &mut Document,
    id: NodeId,
    previous: Option<NodeId>,
    formatter: &dyn LinkFormatter,
    fuzzy: bool,
) -> Option<Vec<NodeId>> {
    let text = document.node(id).cast::<Text>()?;

    if !fuzzy && !text.content.contains([':', '@', '/']) {
        return None;
    }

    let links = Linkify::new().links_with_fuzzy(&text.content, fuzzy);
    if links.is_empty() {
        return None;
    }

    let content = text.content.clone();
    let preceded_by_special = previous.is_some_and(|id| document.node(id).is::<TextSpecial>());
    #[cfg(feature = "sourcemap")]
    let source_map = document.node(id).srcmap();
    let mut nodes = Vec::new();
    let mut end = 0;
    for found in links {
        if found.start() == 0 && preceded_by_special {
            continue;
        }

        let url = &content[found.start()..found.end()];
        let mode = if found.kind() == LinkKind::Email {
            LinkifyMode::Email
        } else if url.contains("://") {
            LinkifyMode::Scheme
        } else {
            LinkifyMode::Fuzzy
        };
        let Some(link) = prepare_link(formatter, mode, url) else {
            continue;
        };

        if found.start() > end {
            let node = document.create_node(Text {
                content: content[end..found.start()].to_owned(),
            });
            #[cfg(feature = "sourcemap")]
            document.node_mut(node).set_srcmap(text_range_map(
                source_map,
                content.len(),
                end,
                found.start(),
            ));
            nodes.push(node);
        }

        #[cfg(feature = "sourcemap")]
        let map = text_range_map(source_map, content.len(), found.start(), found.end());
        let node = create_linkified(
            document,
            link,
            #[cfg(feature = "sourcemap")]
            map,
        );
        #[cfg(feature = "sourcemap")]
        document.node_mut(node).set_srcmap(map);
        nodes.push(node);
        end = found.end();
    }

    if nodes.is_empty() {
        return None;
    }

    if end < content.len() {
        let node = document.create_node(Text {
            content: content[end..].to_owned(),
        });
        #[cfg(feature = "sourcemap")]
        document.node_mut(node).set_srcmap(text_range_map(
            source_map,
            content.len(),
            end,
            content.len(),
        ));
        nodes.push(node);
    }
    Some(nodes)
}

fn apply_linkify_replacements(document: &mut Document, replacements: LinkifyReplacements) {
    for (parent, replacements) in replacements {
        if let Some(parent) = parent {
            document.rewrite_children(parent, |document, children| {
                rebuild_linkified_children(document, children, replacements);
            });
        }
    }
}

// As in JS core linkify, size the result once and copy each sibling once.
// Arena nodes are grouped by parent because emphasis creates nested arrays.
fn rebuild_linkified_children(
    document: &mut Document,
    children: &mut Vec<NodeId>,
    mut replacements: HashMap<NodeId, Vec<NodeId>>,
) {
    let extra: usize = replacements.values().map(|nodes| nodes.len() - 1).sum();
    let old_children = std::mem::replace(children, Vec::with_capacity(children.len() + extra));
    for child in old_children {
        if let Some(nodes) = replacements.remove(&child) {
            children.extend(nodes);
            document.discard_node(child);
        } else {
            children.push(child);
        }
    }
    debug_assert!(replacements.is_empty());
}

#[cfg(feature = "sourcemap")]
fn text_range_map(
    map: Option<crate::common::sourcemap::SourcePos>,
    text_len: usize,
    start: usize,
    end: usize,
) -> Option<crate::common::sourcemap::SourcePos> {
    let (source_start, source_end) = map?.get_byte_offsets();
    // Multiline container prefixes can make the original span noncontiguous.
    // Only derive subranges when text and source offsets correspond directly.
    (source_end - source_start == text_len)
        .then(|| crate::common::sourcemap::SourcePos::new(source_start + start, source_start + end))
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LinkifyEnabled {
    postprocess: bool,
}

#[derive(Debug, Clone, Copy)]
struct LinkifyPosition {
    start: usize,
    end: usize,
    email: bool,
}

#[doc(hidden)]
#[derive(Default)]
pub struct LinkifyPrescan;
impl CoreRule for LinkifyPrescan {
    const NAMES: &'static [&'static str] = &["linkify_prescan"];

    fn run(&self, state: &mut CoreState<'_>) {
        // Gate inline scanners; positions are cached per inline source.
        let postprocess = state.md.has_rule::<LinkifyRule>();
        state.root_ext_mut().insert(LinkifyEnabled { postprocess });
    }
}

fn scan_positions(source: &str, md: &MarkdownIt) -> Vec<LinkifyPosition> {
    let fuzzy_links = md
        .ext
        .get::<LinkifyOptions>()
        .copied()
        .unwrap_or_default()
        .fuzzy_links;
    // Every supported link form needs one of these bytes. Check each inline
    // source because core rules may synthesize new content.
    if !fuzzy_links && memchr::memchr3(b':', b'@', b'/', source.as_bytes()).is_none() {
        return Vec::new();
    }
    Linkify::new()
        .links_with_fuzzy(source, fuzzy_links)
        .into_iter()
        .map(|link| LinkifyPosition {
            start: link.start(),
            end: link.end(),
            email: link.kind() == LinkKind::Email,
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
enum LinkifyMode {
    /// URL with an explicit scheme (`http://example.com/path`).
    Scheme,
    /// URL without a scheme (`example.com`) or with `//`.
    Fuzzy,
    /// Email address (`user@example.com`).
    Email,
}

impl LinkifyMode {
    fn accepts(self, position: LinkifyPosition) -> bool {
        match self {
            Self::Email => position.email,
            Self::Scheme | Self::Fuzzy => !position.email,
        }
    }

    fn injected_prefix(self, url: &str) -> Option<&'static str> {
        match self {
            Self::Email if !starts_with_ascii_case_insensitive(url, "mailto:") => Some("mailto:"),
            Self::Fuzzy if !url.starts_with("//") => Some("http://"),
            _ => None,
        }
    }
}

#[doc(hidden)]
pub struct LinkifyScanner;

#[doc(hidden)]
pub struct LinkifyFuzzyScanner;

#[doc(hidden)]
pub struct LinkifyEmailScanner;

// Boundary scans must not consume or rewind linkify spans.
impl InlineRule for LinkifyScanner {
    const MARKER: char = ':';
    const NAMES: &'static [&'static str] = &["linkify"];

    fn check(_: &mut InlineState<'_>) -> Option<usize> {
        None
    }

    fn run(state: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        run_candidate(state, LinkifyMode::Scheme)
    }
}

impl InlineRule for LinkifyFuzzyScanner {
    const MARKER: char = '.';
    const NAMES: &'static [&'static str] = &["linkify_fuzzy"];

    fn check(_: &mut InlineState<'_>) -> Option<usize> {
        None
    }

    fn run(state: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        run_candidate(state, LinkifyMode::Fuzzy)
    }
}

impl InlineRule for LinkifyEmailScanner {
    const MARKER: char = '@';
    const NAMES: &'static [&'static str] = &["linkify_email"];

    fn check(_: &mut InlineState<'_>) -> Option<usize> {
        None
    }

    fn run(state: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        run_candidate(state, LinkifyMode::Email)
    }
}

// --- runner ---

#[derive(Debug, Clone, Copy)]
struct CandidateRange {
    start: usize,
    end: usize,
    rewind: usize,
}

impl CandidateRange {
    fn len(self) -> usize {
        self.end - self.start
    }
}

#[derive(Debug, Clone)]
struct PreparedLink {
    href: String,
    content: String,
}

struct CandidateInput<'a> {
    src: &'a str,
    pos: usize,
    pos_max: usize,
    trailing: &'a str,
    source_start: usize,
    positions: &'a [LinkifyPosition],
}

fn scan_candidate(input: CandidateInput<'_>, mode: LinkifyMode) -> Option<CandidateRange> {
    let trailing = input.trailing;
    let (rewind, length) = if matches!(mode, LinkifyMode::Scheme) {
        if !input.src[input.pos..input.pos_max].starts_with("://") {
            return None;
        }

        let rewind = find_scheme_len(input.src, input.pos, trailing.len())?;
        // Scheme match skips the prescan's boundary rule, so `//https://` still links.
        let found = Linkify::new().match_at_start(&input.src[input.pos - rewind..input.pos_max])?;
        (rewind, found.end())
    } else {
        let start = input.source_start;
        // https://example.com
        // ^    ^            ^
        // |    |            |
        // start colon      end
        // find which interval the colon is in
        let found_idx = input
            .positions
            .binary_search_by(|x| {
                if x.start >= start {
                    Ordering::Greater
                } else if x.end <= start {
                    Ordering::Less
                } else {
                    Ordering::Equal
                }
            })
            .ok()?;
        let found = input.positions[found_idx];
        if !mode.accepts(found) {
            return None;
        }

        let rewind = start - found.start;
        if rewind > trailing.len() {
            return None;
        }

        (rewind, found.end - found.start)
    };

    // \https://example.com
    // this should keep text.
    if trailing[..trailing.len() - rewind].ends_with('\\') {
        return None;
    }

    debug_assert_eq!(
        &trailing[trailing.len() - rewind..],
        &input.src[input.pos - rewind..input.pos]
    );

    let mut candidate = CandidateRange {
        start: input.pos - rewind,
        end: input.pos - rewind + length,
        rewind,
    };
    // markdown-it's inline linkifier leaves trailing asterisks for emphasis.
    // The detector itself must retain them: they are valid URL path content.
    if matches!(mode, LinkifyMode::Scheme) {
        while candidate.end > candidate.start && input.src.as_bytes()[candidate.end - 1] == b'*' {
            candidate.end -= 1;
        }
    }
    if candidate.end > input.pos_max {
        return None;
    }

    let url = &input.src[candidate.start..candidate.end];
    if matches!(mode, LinkifyMode::Fuzzy) && url.contains("://") {
        return None;
    }

    Some(candidate)
}

fn prepare_link(
    formatter: &dyn LinkFormatter,
    mode: LinkifyMode,
    url: &str,
) -> Option<PreparedLink> {
    let injected_prefix = mode.injected_prefix(url);
    let href_source = match injected_prefix {
        Some(prefix) => format!("{prefix}{url}"),
        None => url.to_owned(),
    };

    let href = formatter.normalize_link(&href_source);
    formatter.validate_link(&href)?;

    let mut content = formatter.normalize_link_text(&href_source);
    if let Some(prefix) = injected_prefix
        && starts_with_ascii_case_insensitive(&content, prefix)
    {
        content.drain(..prefix.len());
    }

    Some(PreparedLink { href, content })
}

fn create_linkified(
    document: &mut Document,
    link: PreparedLink,
    #[cfg(feature = "sourcemap")] map: Option<crate::common::sourcemap::SourcePos>,
) -> NodeId {
    let inner = document.create_node(TextSpecial {
        content: link.content.clone(),
        markup: link.content,
        info: "autolink",
    });

    #[cfg(feature = "sourcemap")]
    document.node_mut(inner).set_srcmap(map);

    let node = document.create_node(Linkified { url: link.href });
    document.push_child(node, inner);
    node
}

fn run_candidate(
    state: &mut InlineState<'_>,
    mode: LinkifyMode,
) -> Option<(Option<NodeId>, usize)> {
    // Require the prescan gate; positions are local to this source.
    let postprocess = state.root_ext?.get::<LinkifyEnabled>()?.postprocess;
    if state.link_level > 0
        && (!postprocess
            || state
                .inline_ext
                .contains::<crate::parser::inline::helpers::full_link::MarkdownLinkLabel>())
    {
        return None;
    }

    let scheme = matches!(mode, LinkifyMode::Scheme);
    let (has_candidates, candidate) = {
        let mut cache = state.shared_ext();
        // Scheme matching ignores prescan boundaries and needs no source-wide
        // positions when there is no core pass.
        let positions = if scheme && !postprocess {
            &[][..]
        } else {
            cache
                .get_or_insert_with(|| LinkifyPositions(scan_positions(&state.src, state.md)))
                .0
                .as_slice()
        };
        if !scheme && positions.is_empty() {
            return None;
        }
        let has_candidates = !positions.is_empty();
        let candidate = if state.link_level > 0 {
            None
        } else {
            scan_candidate(
                CandidateInput {
                    src: &state.src,
                    pos: state.pos,
                    pos_max: state.pos_max,
                    trailing: state.trailing_text(),
                    source_start: state.pos,
                    positions,
                },
                mode,
            )
        };
        (has_candidates, candidate)
    };

    // Decide from the prescan even if this cursor did not match; core may
    // retry text hidden by raw HTML anchors.
    if postprocess
        && has_candidates
        && !state
            .inline_ext
            .contains::<crate::parser::inline::helpers::full_link::MarkdownLinkLabel>()
    {
        state.defer_to_core();
    }
    let candidate = candidate?;

    // Emails are only linked by the core pass; a later `</a>` may still
    // protect this text.
    if matches!(mode, LinkifyMode::Email) {
        return None;
    }

    let raw = &state.src[candidate.start..candidate.end];
    if matches!(mode, LinkifyMode::Fuzzy) && raw.starts_with("//") {
        // Defer until after emphasis cleanup.
        return None;
    }

    let link = prepare_link(state.markdown_it().link_formatter.as_ref(), mode, raw)?;

    #[cfg(feature = "sourcemap")]
    let srcmap = state.get_map(candidate.start, candidate.end);
    let node = create_linkified(
        state.document,
        link,
        #[cfg(feature = "sourcemap")]
        srcmap,
    );
    state.pop_trailing_text(candidate.rewind);
    state.pos -= candidate.rewind;

    Some((Some(node), candidate.len()))
}

// --- helper ---

fn starts_with_ascii_case_insensitive(input: &str, prefix: &str) -> bool {
    input
        .get(..prefix.len())
        .is_some_and(|actual| actual.eq_ignore_ascii_case(prefix))
}

fn find_scheme_len(src: &str, pos: usize, trailing_len: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    // look back at most 10 chars. avoid ReDoS
    let min = pos.saturating_sub(10.min(trailing_len));
    let mut start = pos;

    while start > min {
        let byte = bytes[start - 1];
        if byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.') {
            start -= 1;
        } else {
            break;
        }
    }

    if start == pos || !bytes[start].is_ascii_alphabetic() {
        return None;
    }

    Some(pos - start)
}

#[cfg(all(test, feature = "linkify"))]
mod tests;
