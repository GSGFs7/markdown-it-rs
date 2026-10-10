//! Find urls and emails, and turn them into links

use std::cmp::Ordering;
use std::collections::HashMap;

use linkify::{LinkKind, Linkify};

use crate::MarkdownIt;
use crate::common::extset::RootExtSet;
use crate::document::{NodeId, NodeRef, NodeValue, StructuralEvent, Text, TextSpecial};
use crate::links::LinkFormatter;
use crate::parser::core::{CoreRule, DocumentCoreRule};
use crate::parser::inline::builtin::InlineParserRule;
use crate::parser::inline::{DocumentInlineState, InlineRule};
use crate::render::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    TransparentDocumentRenderer,
    write_html_close,
    write_html_open,
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
        let mut attrs = node.attrs().clone();
        attrs.push(("href".into(), link.url.clone()));
        write_html_open(output, "a", &attrs);
        context.render_children(node.id(), output);
        write_html_close(output, "a");
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
        .add_rule_with_finalize::<LinkifyScanner>(retry_html_link_text);
    md.inline.add_rule::<LinkifyFuzzyScanner>();
    md.inline.add_rule::<LinkifyEmailScanner>();
    md.add_document_renderer::<Linkified, _>("html", LinkifiedDocumentRenderer);
    md.add_document_renderer::<Linkified, _>("text", TransparentDocumentRenderer);
}

// The JS core linkifier walks parsed tokens backwards. An unmatched HTML
// opening anchor therefore does not protect text in that second pass, even
// though it disabled the inline scanner. Retry that text after tokenization.
fn retry_html_link_text(state: &mut DocumentInlineState<'_>) {
    use crate::plugins::cmark::inline::autolink::Autolink;
    use crate::plugins::cmark::inline::image::Image;
    use crate::plugins::cmark::inline::link::Link;
    use crate::plugins::html::html_inline::HtmlInline;

    if state.link_level <= 0 {
        return;
    }

    let mut leaves = Vec::new();
    let mut protected_depth = 0;
    let mut html_delta = 0;
    for &root in state.nodes() {
        for event in state.document.events(root) {
            let node = event.node();
            let protected = node.is::<Link>()
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
                        leaves.push((node.id(), delta));
                    } else {
                        leaves.push((node.id(), 0));
                    }
                }
                StructuralEvent::Enter(_) | StructuralEvent::Exit(_) if protected_depth == 0 => {
                    leaves.push((node.id(), 0));
                }
                _ => {}
            }
        }
    }

    // Child sessions inside Markdown links inherit a positive link level.
    if html_delta <= 0 || state.link_level - html_delta > 0 {
        return;
    }

    let mut reverse_level = 0;
    let mut replacements: HashMap<Option<NodeId>, HashMap<NodeId, Vec<NodeId>>> = HashMap::new();
    let fuzzy = state
        .md
        .ext
        .get::<LinkifyOptions>()
        .copied()
        .unwrap_or_default()
        .fuzzy_links;
    for index in (0..leaves.len()).rev() {
        let (id, delta) = leaves[index];
        if delta < 0 {
            reverse_level += 1;
        } else if delta > 0 && reverse_level > 0 {
            reverse_level -= 1;
        }

        if reverse_level > 0 {
            continue;
        }

        let Some(text) = state.document.node(id).cast::<Text>() else {
            continue;
        };

        // With fuzzy links disabled, every URL/email needs one of these
        // markers. Avoid matching and copying ordinary text in the retry pass.
        if !fuzzy && !text.content.contains([':', '@', '/']) {
            continue;
        }

        let links = Linkify::new().links_with_fuzzy(&text.content, fuzzy);
        if links.is_empty() {
            continue;
        }

        let content = text.content.clone();
        let source_map = state.document.node(id).srcmap();
        let mut nodes = Vec::new();
        let mut end = 0;
        for found in links {
            if found.start() == 0
                && index > 0
                && state.document.node(leaves[index - 1].0).is::<TextSpecial>()
            {
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
            let Some(link) = prepare_link(state.md.link_formatter.as_ref(), mode, url) else {
                continue;
            };

            if found.start() > end {
                let node = state.document.create_node(Text {
                    content: content[end..found.start()].to_owned(),
                });
                state.document.node_mut(node).set_srcmap(text_range_map(
                    source_map,
                    content.len(),
                    end,
                    found.start(),
                ));
                nodes.push(node);
            }

            let inner = state.document.create_node(TextSpecial {
                content: link.content.clone(),
                markup: link.content,
                info: "autolink",
            });
            let node = state.document.create_node(Linkified { url: link.href });
            let map = text_range_map(source_map, content.len(), found.start(), found.end());
            state.document.node_mut(inner).set_srcmap(map);
            state.document.node_mut(node).set_srcmap(map);
            state.document.push_child(node, inner);
            nodes.push(node);
            end = found.end();
        }

        if nodes.is_empty() {
            continue;
        }

        if end < content.len() {
            let node = state.document.create_node(Text {
                content: content[end..].to_owned(),
            });
            state.document.node_mut(node).set_srcmap(text_range_map(
                source_map,
                content.len(),
                end,
                content.len(),
            ));
            nodes.push(node);
        }
        replacements
            .entry(state.document.parent(id))
            .or_default()
            .insert(id, nodes);
    }

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
    document: &mut crate::document::Document,
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

fn anchor_delta(content: &str) -> i32 {
    let bytes = content.as_bytes();
    if bytes.len() >= 3
        && bytes[0] == b'<'
        && bytes[1].eq_ignore_ascii_case(&b'a')
        && (bytes[2] == b'>' || bytes[2].is_ascii_whitespace())
    {
        1
    } else if bytes.len() >= 4
        && bytes[..2] == *b"</"
        && bytes[2].eq_ignore_ascii_case(&b'a')
        && (bytes[3] == b'>' || bytes[3].is_ascii_whitespace())
    {
        -1
    } else {
        0
    }
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

#[doc(hidden)]
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

#[doc(hidden)]
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
    let scheme_len = if matches!(mode, LinkifyMode::Scheme) {
        Some(find_scheme_len(input.src, input.pos, trailing.len())?)
    } else {
        None
    };

    let start = input.source_start;
    let positions = input.positions;
    // https://example.com
    // ^    ^            ^
    // |    |            |
    // start colon      end
    // find which interval the colon is in
    let found_idx = positions
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

    let found = positions[found_idx];
    if !mode.accepts(found) {
        return None;
    }

    let rewind = start - found.start;
    if rewind > trailing.len() {
        return None;
    }
    if scheme_len.is_some_and(|scheme_len| scheme_len != rewind) {
        return None;
    }
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
        end: input.pos - rewind + found.end - found.start,
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

    let link = prepare_link(
        state.markdown_it().link_formatter.as_ref(),
        mode,
        &state.src[candidate.start..candidate.end],
    )?;

    let inner = state.document.create_node(TextSpecial {
        content: link.content.clone(),
        markup: link.content,
        info: "autolink",
    });
    let srcmap = state.get_map(candidate.start, candidate.end);
    state.document.node_mut(inner).set_srcmap(srcmap);

    let node = state.document.create_node(Linkified { url: link.href });
    state.document.push_child(node, inner);
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
mod tests {
    use crate as markdown_it;

    #[test]
    fn retry_batches_siblings_and_nested_emphasis() {
        let input = format!(
            "<a>{}",
            "https://example.org *https://example.com* &amp; plain ".repeat(128)
        );
        let output = format!(
            "<p><a>{}</p>",
            "<a href=\"https://example.org\">https://example.org</a> <em><a href=\"https://example.com\">https://example.com</a></em> &amp; plain ".repeat(128),
        );
        // The parser trims trailing spaces at the end of the inline range.
        run(input.trim_end(), &output.replace("plain </p>", "plain</p>"));
    }

    #[test]
    fn unmatched_html_anchor_retries_linkification() {
        let mut md = crate::MarkdownIt::new();
        crate::plugins::html::add(&mut md);
        crate::plugins::extra::linkify::add(&mut md);
        crate::plugins::extra::typographer::add(&mut md);
        crate::plugins::extra::smartquotes::add(&mut md);
        let run = |input: &str, output: &str| {
            assert_eq!(md.render(input), format!("{output}\n"));
        };
        run(
            r#"<a href>a!~|\中="/">https://example.org</a"#,
            r#"<p><a href>a!~|\中=“/”&gt;<a href="https://example.org">https://example.org</a>&lt;/a</p>"#,
        );
        run(
            "<a>https://example.org",
            "<p><a><a href=\"https://example.org\">https://example.org</a></p>",
        );
        run(
            "<a>a@b.co //example.org",
            "<p><a><a href=\"mailto:a@b.co\">a@b.co</a> <a href=\"//example.org\">//example.org</a></p>",
        );
        run(
            "<a>*https://example.org*</a",
            "<p><a><em><a href=\"https://example.org\">https://example.org</a></em>&lt;/a</p>",
        );
        run(
            "<a>https://example.org</a> <a>https://example.org",
            "<p><a>https://example.org</a> <a><a href=\"https://example.org\">https://example.org</a></p>",
        );
        run(
            "[<a>https://example.org](https://example.org)",
            "<p><a href=\"https://example.org\"><a>https://example.org</a></p>",
        );
    }

    #[test]
    fn direct_prescan_persists_original_source_positions_without_scanners() {
        use super::{
            LinkifyEmailScanner,
            LinkifyFuzzyScanner,
            LinkifyPrescan,
            LinkifyScanner,
            LinkifyState,
        };
        use crate::document::Root;
        use crate::parser::block::builtin::BlockParserRule;
        use crate::parser::inline::builtin::InlineParserRule;
        for before_blocks in [false, true] {
            let mut md = crate::MarkdownIt::empty();
            super::add(&mut md);
            md.inline.remove_rule::<LinkifyScanner>();
            md.inline.remove_rule::<LinkifyFuzzyScanner>();
            md.inline.remove_rule::<LinkifyEmailScanner>();
            md.remove_rule::<LinkifyPrescan>();
            if before_blocks {
                md.add_rule::<LinkifyPrescan>().before::<BlockParserRule>();
            } else {
                md.add_rule::<LinkifyPrescan>()
                    .after::<BlockParserRule>()
                    .before::<InlineParserRule>();
            }
            for limit in [0, 100] {
                md.max_nesting = limit;
                for source in ["", "雪 https://example.com\r\na@b.co"] {
                    let document = md.parse_document(source);
                    let positions: Vec<_> = document
                        .node(document.root())
                        .cast::<Root>()
                        .unwrap()
                        .ext
                        .get::<LinkifyState>()
                        .unwrap()
                        .iter()
                        .map(|position| (position.start, position.end, position.email))
                        .collect();
                    let expected = if source.is_empty() {
                        vec![]
                    } else {
                        vec![(4, 23, false), (25, 31, true)]
                    };
                    assert_eq!(
                        positions, expected,
                        "{source:?}, before_blocks={before_blocks}, limit={limit}"
                    );
                }
            }
        }
    }

    #[test]
    fn prescan_does_not_run_inline_postprocessors_too_early() {
        use crate::plugins::cmark;
        use crate::plugins::extra::*;

        let md = &mut MarkdownIt::empty();
        cmark::add(md);
        typographer::add(md);
        smartquotes::add(md);
        linkify::add(md);

        assert_eq!(md.render(r#"a~~"foo"~~"#), "<p>a~~“foo”~~</p>\n");
    }

    fn run(input: &str, output: &str) {
        let output = if output.is_empty() {
            "".to_owned()
        } else {
            output.to_owned() + "\n"
        };
        let md = &mut markdown_it::MarkdownIt::empty();
        markdown_it::plugins::cmark::add(md);
        markdown_it::plugins::html::add(md);
        markdown_it::plugins::extra::linkify::add(md);
        let node = md.parse_document(&(input.to_owned() + "\n"));

        // make sure we have sourcemaps for everything
        for event in node.events(node.root()) {
            assert!(event.node().srcmap().is_some());
        }

        let result = md.render_document(&node);
        assert_eq!(result, output);
        // make sure it doesn't crash without trailing \n
        let _ = md.parse_document(input.trim_end());
    }

    #[test]
    fn linkify() {
        let input = r#"url http://www.youtube.com/watch?v=5Jt5GEr4AYg."#;
        let output = r#"<p>url <a href="http://www.youtube.com/watch?v=5Jt5GEr4AYg">http://www.youtube.com/watch?v=5Jt5GEr4AYg</a>.</p>"#;
        run(input, output);
    }

    #[test]
    fn don_t_touch_text_in_links() {
        let input = r#"[https://example.com](https://example.com)"#;
        let output = r#"<p><a href="https://example.com">https://example.com</a></p>"#;
        run(input, output);
    }

    #[test]
    fn don_t_touch_text_in_autolinks() {
        let input = r#"<https://example.com>"#;
        let output = r#"<p><a href="https://example.com">https://example.com</a></p>"#;
        run(input, output);
    }

    #[test]
    fn don_t_touch_text_in_html_a_tags() {
        let input = r#"<a href="https://example.com">https://example.com</a>"#;
        let output = r#"<p><a href="https://example.com">https://example.com</a></p>"#;
        run(input, output);
    }

    #[test]
    fn entities_inside_raw_links() {
        let input = r#"https://example.com/foo&amp;bar"#;
        let output = r#"<p><a href="https://example.com/foo&amp;amp;bar">https://example.com/foo&amp;amp;bar</a></p>"#;
        run(input, output);
    }

    #[test]
    fn emphasis_inside_raw_links_asterisk_can_happen_in_links_with_params() {
        let input = r#"https://example.com/foo*bar*baz"#;
        let output = r#"<p><a href="https://example.com/foo*bar*baz">https://example.com/foo*bar*baz</a></p>"#;
        run(input, output);
    }

    #[test]
    fn emphasis_inside_raw_links_underscore() {
        let input = r#"http://example.org/foo._bar_-_baz"#;
        let output = r#"<p><a href="http://example.org/foo._bar_-_baz">http://example.org/foo._bar_-_baz</a></p>"#;
        run(input, output);
    }

    #[test]
    fn backticks_inside_raw_links() {
        let input = r#"https://example.com/foo`bar`baz"#;
        let output = r#"<p><a href="https://example.com/foo%60bar%60baz">https://example.com/foo`bar`baz</a></p>"#;
        run(input, output);
    }

    #[test]
    fn unicode_tlds_are_linkified_and_normalized() {
        run(
            "https://例子.测试/a_(b)?x=1&y=2",
            "<p><a href=\"https://xn--fsqu00a.xn--0zwm56d/a_(b)?x=1&amp;y=2\">https://例子.测试/a_(b)?x=1&amp;y=2</a></p>",
        );
    }

    #[test]
    fn pipes_in_url_paths_are_linkified_and_encoded() {
        run(
            "https://例子.测试/a_(b)?x=1|é~&y=2é[aé(",
            "<p><a href=\"https://xn--fsqu00a.xn--0zwm56d/a_(b)?x=1%7C%C3%A9~&amp;y=2%C3%A9\">https://例子.测试/a_(b)?x=1|é~&amp;y=2é</a>[aé(</p>",
        );
        run(
            "//example.com/a|b",
            "<p><a href=\"//example.com/a%7Cb\">//example.com/a|b</a></p>",
        );
    }

    #[test]
    fn links_inside_raw_links() {
        let input = r#"https://example.com/foo[123](456)bar"#;
        let output = r#"<p><a href="https://example.com/foo%5B123%5D(456)bar">https://example.com/foo[123](456)bar</a></p>"#;
        run(input, output);
    }

    #[test]
    fn escapes_not_allowed_at_the_start() {
        let input = r#"\https://example.com"#;
        let output = r#"<p>\https://example.com</p>"#;
        run(input, output);
    }

    #[test]
    fn escapes_not_allowed_at_comma() {
        let input = r#"https\://example.com"#;
        let output = r#"<p>https://example.com</p>"#;
        run(input, output);
    }

    #[test]
    fn escapes_not_allowed_at_slashes() {
        let input = r#"https:\//aa.org https://bb.org"#;
        let output = r#"<p>https://aa.org <a href="https://bb.org">https://bb.org</a></p>"#;
        run(input, output);
    }

    #[test]
    fn fuzzy_link_shouldn_t_match_cc_org() {
        let input = r#"https:/\/cc.org"#;
        let output = r#"<p>https://cc.org</p>"#;
        run(input, output);
    }

    #[test]
    fn bold_links_exclude_markup_of_pairs_from_link_tail() {
        let input = r#"**http://example.com/foobar**"#;
        let output = r#"<p><strong><a href="http://example.com/foobar">http://example.com/foobar</a></strong></p>"#;
        run(input, output);
    }

    #[test]
    fn match_links_without_protocol() {
        let md = &mut markdown_it::MarkdownIt::empty();
        markdown_it::plugins::cmark::add(md);
        markdown_it::plugins::extra::linkify::add_with_options(
            md,
            markdown_it::plugins::extra::linkify::LinkifyOptions { fuzzy_links: true },
        );

        assert_eq!(
            md.render("www.example.org"),
            "<p><a href=\"http://www.example.org\">www.example.org</a></p>\n"
        );
    }

    #[test]
    fn links_without_protocol_are_disabled_by_default() {
        let md = &mut markdown_it::MarkdownIt::empty();
        markdown_it::plugins::cmark::add(md);
        markdown_it::plugins::extra::linkify::add(md);

        assert_eq!(md.render("www.example.org"), "<p>www.example.org</p>\n");
    }

    #[test]
    fn protocol_relative_boundaries_match_linkify_it() {
        for input in [
            "x//example.com",
            "http:////example.com",
            "////example.com",
            "组//example.com",
        ] {
            run(input, &format!("<p>{input}</p>"));
        }

        run(
            "。//example.com",
            "<p>。<a href=\"//example.com\">//example.com</a></p>",
        );
        run(
            "//example.com//other.org",
            "<p><a href=\"//example.com//other.org\">//example.com//other.org</a></p>",
        );
    }

    #[test]
    fn short_email_with_beautifier_does_not_panic() {
        let md = &mut markdown_it::MarkdownIt::empty();
        markdown_it::plugins::cmark::add(md);
        markdown_it::plugins::extra::beautify_links::add(md);
        markdown_it::plugins::extra::linkify::add(md);

        assert_eq!(
            md.render("ping a@b.co ok"),
            "<p>ping <a href=\"mailto:a@b.co\">a@b.co</a> ok</p>\n"
        );
    }

    #[test]
    fn emails() {
        let input = r#"test@example.com

mailto:test@example.com"#;
        let output = r#"<p><a href="mailto:test@example.com">test@example.com</a></p>
<p><a href="mailto:test@example.com">mailto:test@example.com</a></p>"#;
        run(input, output);
    }

    #[test]
    fn email_tlds_can_contain_digits() {
        run(
            "<foo+special@Bar.b9\n[bz-barar0.com>\n",
            "<p>&lt;<a href=\"mailto:foo+special@Bar.b9\">foo+special@Bar.b9</a>\n[bz-barar0.com&gt;</p>",
        );
    }

    #[test]
    fn typorgapher_should_not_break_href() {
        let input = r#"http://example.com/(c)"#;
        let output = r#"<p><a href="http://example.com/(c)">http://example.com/(c)</a></p>"#;
        run(input, output);
    }

    #[test]
    fn coverage_prefix_not_valid() {
        let input = r#"http:/example.com/"#;
        let output = r#"<p>http:/example.com/</p>"#;
        run(input, output);
    }

    #[test]
    fn unregistered_schemes_are_not_linkified() {
        let input = r#"a://a://"#;
        let output = r#"<p>a://a://</p>"#;
        run(input, output);
    }

    #[test]
    fn coverage_negative_link_level() {
        let input = r#"</a>[https://example.com](https://example.com)"#;
        let output = r#"<p></a><a href="https://example.com"><a href="https://example.com">https://example.com</a></a></p>"#;
        run(input, output);
    }

    #[test]
    fn emphasis_with_real_link() {
        let input = r#"http://cdecl.ridiculousfish.com/?q=int+%28*f%29+%28float+*%29%3B"#;
        let output = r#"<p><a href="http://cdecl.ridiculousfish.com/?q=int+%28*f%29+%28float+*%29%3B">http://cdecl.ridiculousfish.com/?q=int+(*f)+(float+*)%3B</a></p>"#;
        run(input, output);
    }

    #[test]
    fn emphasis_with_real_link_1() {
        let input = r#"https://www.sell.fi/sites/default/files/elainlaakarilehti/tieteelliset_artikkelit/kahkonen_t._et_al.canine_pancreatitis-_review.pdf"#;
        let output = r#"<p><a href="https://www.sell.fi/sites/default/files/elainlaakarilehti/tieteelliset_artikkelit/kahkonen_t._et_al.canine_pancreatitis-_review.pdf">https://www.sell.fi/sites/default/files/elainlaakarilehti/tieteelliset_artikkelit/kahkonen_t._et_al.canine_pancreatitis-_review.pdf</a></p>"#;
        run(input, output);
    }
}
