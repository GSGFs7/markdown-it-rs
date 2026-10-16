//! Find urls and emails, and turn them into links

// TODO: refactor this

use std::cmp::Ordering;
use std::collections::HashMap;

use linkify::{LinkKind, Linkify};

use crate::MarkdownIt;
use crate::common::extset::RootExtSet;
use crate::common::sourcemap::SourcePos;
use crate::document::{Document, NodeId, NodeRef, NodeValue, StructuralEvent, Text, TextSpecial};
use crate::links::LinkFormatter;
use crate::parser::core::{CoreRule, DocumentCoreRule};
use crate::parser::inline::builtin::InlineParserRule;
use crate::parser::inline::{DocumentInlineState, InlineRule};
use crate::plugins::html::anchor_delta;
use crate::render::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    TransparentDocumentRenderer,
    render_html_link,
};

#[derive(Debug)]
pub struct Linkified {
    pub url: String,
}

impl NodeValue for Linkified {}

struct LinkifiedDocumentRenderer;

impl DocumentNodeRenderer<Linkified> for LinkifiedDocumentRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        link: &Linkified,
        context: &mut DocumentRenderContext<'_>,
        output: &mut crate::DocumentWriter,
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
    md.inline
        .add_rule_with_finalize::<LinkifyScanner>(linkify_text);
    md.inline.add_rule::<LinkifyFuzzyScanner>();
    md.inline.add_rule::<LinkifyEmailScanner>();
    md.add_document_renderer::<Linkified, _>("html", LinkifiedDocumentRenderer);
    md.add_document_renderer::<Linkified, _>("text", TransparentDocumentRenderer);
}

// Second linkify pass, mirroring JS core linkify: match protocol-relative
// URLs and emails after emphasis, and retry text hidden by an unmatched `<a>`
// (which disables the inline scanners but does not protect text in JS).
fn linkify_text(state: &mut DocumentInlineState<'_>) {
    if state
        .inline_ext
        .contains::<crate::parser::inline::helpers::full_link::MarkdownLinkLabel>()
    {
        return;
    }

    if state.link_level <= 0 && !state.src.contains("//") && !state.src.contains('@') {
        return;
    }

    let (events, html_delta) = collect_linkify_events(state.document, state.nodes());

    // Child sessions inside Markdown links inherit a positive link level.
    // An HTML closing tag can lower the final level to zero even inside a
    // Markdown link. Recover the inherited level before deciding to retry.
    if state.link_level - html_delta > 0 {
        return;
    }
    let retry_html = state.link_level > 0 && html_delta > 0;
    let fuzzy = state
        .md
        .ext
        .get::<LinkifyOptions>()
        .copied()
        .unwrap_or_default()
        .fuzzy_links;

    // JS gates on the original inline content first; e.g. `__//example.org__`
    // fails the `_//` boundary even after emphasis strips the underscores.
    if Linkify::new()
        .links_with_fuzzy(&state.src, fuzzy)
        .is_empty()
    {
        return;
    }

    let replacements = collect_linkify_replacements(
        state.document,
        &events,
        state.md.link_formatter.as_ref(),
        retry_html,
        fuzzy,
    );
    apply_linkify_replacements(state, replacements);
}

fn collect_linkify_events(document: &Document, roots: &[NodeId]) -> (Vec<(NodeId, i32)>, i32) {
    use crate::plugins::cmark::inline::autolink::Autolink;
    use crate::plugins::cmark::inline::backticks::CodeInline;
    use crate::plugins::cmark::inline::image::Image;
    use crate::plugins::cmark::inline::link::Link;
    use crate::plugins::html::html_inline::HtmlInline;

    let mut events = Vec::new();
    let mut protected_depth = 0;
    let mut html_delta = 0;
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
                        let delta = anchor_delta(&html.content);
                        html_delta += delta;
                        events.push((node.id(), delta));
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

    (events, html_delta)
}

type LinkifyReplacements = HashMap<Option<NodeId>, HashMap<NodeId, Vec<NodeId>>>;

fn collect_linkify_replacements(
    document: &mut Document,
    events: &[(NodeId, i32)],
    formatter: &dyn LinkFormatter,
    retry_html: bool,
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
        if let Some(nodes) = linkify_text_node(document, id, previous, formatter, retry_html, fuzzy)
        {
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
    retry_html: bool,
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
    let source_map = document.node(id).srcmap();
    let mut nodes = Vec::new();
    let mut end = 0;
    for found in links {
        if found.start() == 0 && preceded_by_special {
            continue;
        }

        let url = &content[found.start()..found.end()];
        if !retry_html && found.kind() != LinkKind::Email && !url.starts_with("//") {
            continue;
        }

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
            document.node_mut(node).set_srcmap(text_range_map(
                source_map,
                content.len(),
                end,
                found.start(),
            ));
            nodes.push(node);
        }

        let map = text_range_map(source_map, content.len(), found.start(), found.end());
        let node = create_linkified(document, link, map);
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

fn apply_linkify_replacements(
    state: &mut DocumentInlineState<'_>,
    replacements: LinkifyReplacements,
) {
    for (parent, replacements) in replacements {
        if let Some(parent) = parent {
            state
                .document
                .rewrite_children(parent, |document, children| {
                    rebuild_linkified_children(document, children, replacements);
                });
        } else {
            let mut children = std::mem::take(state.nodes_mut());
            rebuild_linkified_children(state.document, &mut children, replacements);
            *state.nodes_mut() = children;
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

fn text_range_map(
    map: Option<SourcePos>,
    text_len: usize,
    start: usize,
    end: usize,
) -> Option<SourcePos> {
    let (source_start, source_end) = map?.get_byte_offsets();
    // Multiline container prefixes can make the original span noncontiguous.
    // Only derive subranges when text and source offsets correspond directly.
    (source_end - source_start == text_len)
        .then(|| SourcePos::new(source_start + start, source_start + end))
}

type LinkifyState = Vec<LinkifyPosition>;

#[derive(Debug, Clone, Copy)]
struct LinkifyPosition {
    start: usize,
    end: usize,
    email: bool,
}

#[doc(hidden)]
pub struct LinkifyPrescan;
impl CoreRule for LinkifyPrescan {
    const NAMES: &'static [&'static str] = &["linkify_prescan"];

    fn document_rule() -> DocumentCoreRule {
        DocumentCoreRule::PrepareState(Self::prepare)
    }
}

impl LinkifyPrescan {
    fn prepare(source: &str, md: &MarkdownIt, root_ext: &mut RootExtSet) {
        root_ext.insert(scan_positions(source, md));
    }
}

fn scan_positions(source: &str, md: &MarkdownIt) -> LinkifyState {
    let fuzzy_links = md
        .ext
        .get::<LinkifyOptions>()
        .copied()
        .unwrap_or_default()
        .fuzzy_links;
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

#[derive(Clone, Copy)]
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

    fn check(_: &mut DocumentInlineState<'_>) -> Option<usize> {
        None
    }

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        run_document_candidate(state, LinkifyMode::Scheme)
    }
}

impl InlineRule for LinkifyFuzzyScanner {
    const MARKER: char = '.';
    const NAMES: &'static [&'static str] = &["linkify_fuzzy"];

    fn check(_: &mut DocumentInlineState<'_>) -> Option<usize> {
        None
    }

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        run_document_candidate(state, LinkifyMode::Fuzzy)
    }
}

impl InlineRule for LinkifyEmailScanner {
    const MARKER: char = '@';
    const NAMES: &'static [&'static str] = &["linkify_email"];

    fn check(_: &mut DocumentInlineState<'_>) -> Option<usize> {
        None
    }

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        run_document_candidate(state, LinkifyMode::Email)
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

struct PreparedLink {
    href: String,
    content: String,
}

struct CandidateInput<'a> {
    src: &'a str,
    pos: usize,
    pos_max: usize,
    trailing: &'a str,
    link_level: i32,
    source_start: usize,
    positions: &'a [LinkifyPosition],
}

fn scan_candidate(input: CandidateInput<'_>, mode: LinkifyMode) -> Option<CandidateRange> {
    if input.link_level > 0 {
        return None;
    }

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

fn create_linkified(document: &mut Document, link: PreparedLink, map: Option<SourcePos>) -> NodeId {
    let inner = document.create_node(TextSpecial {
        content: link.content.clone(),
        markup: link.content,
        info: "autolink",
    });
    document.node_mut(inner).set_srcmap(map);
    let node = document.create_node(Linkified { url: link.href });
    document.push_child(node, inner);
    node
}

fn run_document_candidate(
    state: &mut DocumentInlineState<'_>,
    mode: LinkifyMode,
) -> Option<(Option<NodeId>, usize)> {
    let (source_start, _) = state.get_map(state.pos, state.pos_max)?.get_byte_offsets();
    let candidate = scan_candidate(
        CandidateInput {
            src: &state.src,
            pos: state.pos,
            pos_max: state.pos_max,
            trailing: state.trailing_text(),
            link_level: state.link_level,
            source_start,
            positions: state.root_ext?.get::<LinkifyState>()?,
        },
        mode,
    )?;

    if matches!(mode, LinkifyMode::Email) {
        // JS only links emails in its core postpass. At `@` we cannot yet
        // know whether a later `</a>` protects this text, and consuming the
        // prescan span here could also swallow emphasis or code delimiters.
        state.request_finalization();
        return None;
    }

    if matches!(mode, LinkifyMode::Fuzzy)
        && state.src[candidate.start..candidate.end].starts_with("//")
    {
        // Defer to emphasis and the finalizer below. The flag keeps the
        // finalizer running even when the block stays plain text.
        state.request_finalization();
        return None;
    }

    let link = prepare_link(
        state.markdown_it().link_formatter.as_ref(),
        mode,
        &state.src[candidate.start..candidate.end],
    )?;

    let srcmap = state.get_map(candidate.start, candidate.end);
    let node = create_linkified(state.document, link, srcmap);
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
