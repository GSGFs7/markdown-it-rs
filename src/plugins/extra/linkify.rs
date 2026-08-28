//! Find urls and emails, and turn them into links

use std::cmp::Ordering;
use std::sync::LazyLock;

use linkify::{LinkFinder, LinkKind};
use regex::Regex;

use crate::parser::core::{CoreRule, Root};
use crate::parser::extset::RootExt;
use crate::parser::inline::builtin::InlineParserRule;
use crate::parser::inline::{InlineRule, InlineState, TextSpecial};
use crate::{MarkdownIt, Node, NodeValue, Renderer};

static SCHEME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:^|[^a-z0-9.+-])([a-z][a-z0-9.+-]*)$").unwrap());

#[derive(Debug)]
pub struct Linkified {
    pub url: String,
}

impl NodeValue for Linkified {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        let mut attrs = node.attrs.clone();
        attrs.push(("href", self.url.clone()));

        fmt.open("a", &attrs);
        fmt.contents(&node.children);
        fmt.close("a");
    }
}

pub fn add(md: &mut MarkdownIt) {
    md.add_rule::<LinkifyPrescan>()
        .before::<InlineParserRule>()
        .before_all();

    md.inline.add_rule::<LinkifyScanner>();
    md.inline.add_rule::<LinkifyFuzzyScanner>();
    md.inline.add_rule::<LinkifyEmailScanner>();
}

type LinkifyState = Vec<LinkifyPosition>;
impl RootExt for LinkifyState {}

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

    fn run(root: &mut Node, _: &MarkdownIt) {
        let root_data = root.cast_mut::<Root>().unwrap();
        let source = root_data.content.as_str();
        let mut finder = LinkFinder::new();
        finder.url_must_have_scheme(false);

        let mut positions = finder
            .links(source)
            .map(|link| {
                let email = *link.kind() == LinkKind::Email;
                let mut start = link.start();
                if email
                    && start >= "mailto:".len()
                    && source[start - "mailto:".len()..start].eq_ignore_ascii_case("mailto:")
                {
                    start -= "mailto:".len();
                }
                LinkifyPosition {
                    start,
                    end: link.end(),
                    email,
                }
            })
            .collect::<Vec<_>>();

        // rust `linkify` deliberately doesn't recognize protocol-relative URLs.
        // but markdwonit.js's `linkify-it` will identify it.
        for (start, _) in source.match_indices("//") {
            // https://example.com
            //      ^--- processed
            // \//example.com
            // ^--- disable auto linkify
            if source[..start].ends_with([':', '\\']) {
                continue;
            }

            // //example.com/ ciallo
            //   ^^^^^^^^^^^^^^^^--- check if this is a link
            // (it should identify "example.com/")
            let rest = &source[start + 2..];
            let Some(link) = finder.links(rest).next() else {
                continue;
            };
            if link.start() != 0 || *link.kind() != LinkKind::Url {
                continue;
            }

            positions.push(LinkifyPosition {
                start,
                end: start + 2 + link.end(),
                email: false,
            });
        }

        positions.sort_by_key(|position| (position.start, std::cmp::Reverse(position.end)));
        positions.dedup_by(|a, b| a.start == b.start && a.end == b.end);
        root_data.ext.insert(positions);
    }
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
impl InlineRule for LinkifyScanner {
    const MARKER: char = ':';
    const NAMES: &'static [&'static str] = &["linkify"];

    // `run_candidate` mutates trailing text and the current position. The
    // default `check` calls `run`, which would pollute speculative scans.
    fn check(_: &mut InlineState) -> Option<usize> {
        None
    }

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let mut chars = state.src[state.pos..state.pos_max].chars();
        if chars.next().unwrap() != ':' {
            return None;
        }
        run_candidate(state, LinkifyMode::Scheme)
    }
}

#[doc(hidden)]
pub struct LinkifyFuzzyScanner;
impl InlineRule for LinkifyFuzzyScanner {
    const MARKER: char = '.';
    const NAMES: &'static [&'static str] = &["linkify_fuzzy"];

    fn check(_: &mut InlineState) -> Option<usize> {
        None
    }

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        // entrance guard
        if !state.src[state.pos..state.pos_max].starts_with('.') {
            return None;
        }
        run_candidate(state, LinkifyMode::Fuzzy)
    }
}

#[doc(hidden)]
pub struct LinkifyEmailScanner;
impl InlineRule for LinkifyEmailScanner {
    const MARKER: char = '@';
    const NAMES: &'static [&'static str] = &["linkify_email"];

    fn check(_: &mut InlineState) -> Option<usize> {
        None
    }

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        if !state.src[state.pos..state.pos_max].starts_with('@') {
            return None;
        }
        run_candidate(state, LinkifyMode::Email)
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

// process pipeline
fn run_candidate(state: &mut InlineState, mode: LinkifyMode) -> Option<(Node, usize)> {
    let candidate = find_candidate(state, mode)?;
    let url = &state.src[candidate.start..candidate.end];
    let link = prepare_link(state, mode, url)?;
    let node = build_link_node(state, candidate, link);

    state.trailing_text_pop(candidate.rewind);
    state.pos -= candidate.rewind;
    Some((node, candidate.len()))
}

fn find_candidate(state: &InlineState, mode: LinkifyMode) -> Option<CandidateRange> {
    if state.link_level > 0 {
        // e.g. [https://example.com](other)
        return None;
    }

    // cia https://example.com llo
    // ^^^^^^^^^-- this
    let trailing = state.trailing_text_get();
    if matches!(mode, LinkifyMode::Scheme) && !SCHEME_RE.is_match(trailing) {
        return None;
    }

    let map = state.get_map(state.pos, state.pos_max)?;
    let (start, _) = map.get_byte_offsets();

    let positions = state.root_ext.get::<LinkifyState>()?;

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
    // \https://example.com
    // this should keep text.
    if trailing[..trailing.len() - rewind].ends_with('\\') {
        return None;
    }

    debug_assert_eq!(
        &trailing[trailing.len() - rewind..],
        &state.src[state.pos - rewind..state.pos]
    );

    let candidate = CandidateRange {
        start: state.pos - rewind,
        end: state.pos - rewind + found.end - found.start,
        rewind,
    };
    if candidate.end > state.pos_max {
        return None;
    }

    let url = &state.src[candidate.start..candidate.end];
    if matches!(mode, LinkifyMode::Fuzzy) && url.contains("://") {
        return None;
    }

    Some(candidate)
}

fn prepare_link(state: &InlineState, mode: LinkifyMode, url: &str) -> Option<PreparedLink> {
    let injected_prefix = mode.injected_prefix(url);
    let href_source = match injected_prefix {
        Some(prefix) => format!("{prefix}{url}"),
        None => url.to_owned(),
    };
    let href = state.md.link_formatter.normalize_link(&href_source);

    state.md.link_formatter.validate_link(&href)?;

    let mut content = state.md.link_formatter.normalize_link_text(&href_source);
    if let Some(prefix) = injected_prefix {
        content.drain(..prefix.len());
    }

    Some(PreparedLink { href, content })
}

fn build_link_node(state: &InlineState, candidate: CandidateRange, link: PreparedLink) -> Node {
    let mut inner_node = Node::new(TextSpecial {
        content: link.content.clone(),
        markup: link.content,
        info: "autolink",
    });
    inner_node.srcmap = state.get_map(candidate.start, candidate.end);

    let mut node = Node::new(Linkified { url: link.href });
    node.children.push(inner_node);
    node
}

// --- helper ---

fn starts_with_ascii_case_insensitive(input: &str, prefix: &str) -> bool {
    input
        .get(..prefix.len())
        .is_some_and(|actual| actual.eq_ignore_ascii_case(prefix))
}

#[cfg(test)]
mod test {
    #[test]
    fn prescan_does_not_run_inline_postprocessors_too_early() {
        use crate::plugins::cmark;
        use crate::plugins::extra::*;

        let md = &mut MarkdownIt::new();
        cmark::add(md);
        typographer::add(md);
        smartquotes::add(md);
        linkify::add(md);

        assert_eq!(md.parse(r#"a~~"foo"~~"#).render(), "<p>a~~“foo”~~</p>\n");
    }
}
