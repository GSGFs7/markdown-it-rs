//! Structure similar to `` `code span` `` with configurable markers of variable length.
//!
//! It allows you to define a custom structure with variable number of markers
//! (e.g. with `%` defined as a marker, user can write `%foo%` or `%%%foo%%%`
//! resulting in the same node).
//!
//! You add a custom structure by using [add_with] function, which takes following arguments:
//!  - `MARKER` - marker character
//!  - `md` - parser instance
//!  - `f` - function that should return your custom [NodeDraft]
//!
//! Here is an example of a rule turning `%foo%` into `🦀foo🦀`:
//!
//! ```rust
//! use markdown_it::generics::inline::code_pair;
//! use markdown_it::{MarkdownIt, Node, NodeDraft, NodeValue, Renderer};
//!
//! #[derive(Debug)]
//! struct Ferris;
//! impl NodeValue for Ferris {
//!     fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
//!         fmt.text("🦀");
//!         fmt.contents(&node.children);
//!         fmt.text("🦀");
//!     }
//! }
//!
//! let md = &mut MarkdownIt::empty();
//! code_pair::add_with::<'%'>(md, |_| NodeDraft::new(Ferris));
//! let html = md.parse_document_direct("hello %world%").unwrap().into_legacy().render();
//! assert_eq!(html.trim(), "hello 🦀world🦀");
//! ```
//!
//! This generic structure follows exact rules of code span in CommonMark:
//!
//! 1. Literal marker character sequence can be used inside of structure if its length
//!    doesn't match length of the opening/closing sequence (e.g. with `%` defined
//!    as a marker, `%%foo%bar%%` gets parsed as `Node("foo%bar")`).
//!
//! 2. Single space inside is trimmed to allow you to write `% %%foo %` to be parsed as
//!    `Node("%%foo")`.
//!
//! If you define two structures with the same marker, only the first one will work.
//!
use crate::parser::document::NodeDraft;
use crate::parser::document_parser::DocumentInlineState;
use crate::parser::inline::probe::{InlineProbeContext, InlineProbeKind, InlineProbeResult};
use crate::parser::inline::{InlineRule, InlineState, LegacyInlineRule, Text};
use crate::parser::main::MarkdownIt;
use crate::parser::node::Node;

#[derive(Debug, Default, Clone)]
struct CodePairCache<const MARKER: char> {
    scanned: bool,
    max: Vec<usize>,
}
#[derive(Debug)]
struct CodePairConfig<const MARKER: char>(fn(usize) -> NodeDraft);

pub fn add_with<const MARKER: char>(md: &mut MarkdownIt, f: fn(length: usize) -> NodeDraft) {
    md.ext.insert(CodePairConfig::<MARKER>(f));

    let builder = md.inline.add_migrated_rule::<CodePairScanner<MARKER>>();
    if MARKER == '`' {
        builder.alias_named("backticks");
    }
}

#[doc(hidden)]
pub struct CodePairScanner<const MARKER: char>;
impl<const MARKER: char> InlineRule for CodePairScanner<MARKER> {
    const MARKER: char = MARKER;
    const NAMES: &'static [&'static str] = &["code_pair"];

    fn probe(context: &mut InlineProbeContext<'_>) -> InlineProbeResult {
        let follows_marker = context.trailing_text().ends_with(MARKER);
        let matched = context.with_scratch(|src, start, end, scratch| {
            scan_code_pair_bounds::<MARKER>(src, start, end, follows_marker, scratch)
        });
        match matched {
            Some(matched) => InlineProbeResult::Match {
                len: matched.consumed,
                kind: InlineProbeKind::Token,
            },
            None => InlineProbeResult::NoMatch,
        }
    }

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
        let matched = scan_code_pair::<MARKER>(
            &state.src,
            state.pos,
            state.pos_max,
            state.trailing_text().ends_with(MARKER),
            &mut state.inline_ext,
        )?;
        let f = state
            .markdown_it()
            .ext
            .get::<CodePairConfig<MARKER>>()
            .unwrap()
            .0;
        let mut node = f(matched.marker_len);
        let mut text = NodeDraft::new(Text {
            content: matched.content,
        });
        text.set_srcmap(state.get_map(matched.content_start, matched.content_end));
        node.push_child(text);
        Some((Some(node), matched.consumed))
    }
}

impl<const MARKER: char> LegacyInlineRule for CodePairScanner<MARKER> {
    const MARKER: char = MARKER;
    const NAMES: &'static [&'static str] = &["code_pair"];

    fn check(state: &mut InlineState) -> Option<usize> {
        // avoid polluting cache
        let old_cache = state.inline_ext.get::<CodePairCache<MARKER>>().cloned();
        let result = <Self as LegacyInlineRule>::run(state).map(|(_, len)| len);

        if let Some(cache) = old_cache {
            state.inline_ext.insert(cache);
        } else {
            state.inline_ext.remove::<CodePairCache<MARKER>>();
        }

        result
    }

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let follows_marker = state.trailing_text_get().ends_with(MARKER);
        let matched = state.with_inline_ext(|src, pos, pos_max, inline_ext| {
            scan_code_pair::<MARKER>(src, pos, pos_max, follows_marker, inline_ext)
        })?;
        let f = state.md.ext.get::<CodePairConfig<MARKER>>().unwrap().0;
        let mut node = f(matched.marker_len).into_legacy();
        let mut text = Node::new(Text {
            content: matched.content,
        });
        text.srcmap = state.get_map(matched.content_start, matched.content_end);
        node.children.push(text);
        Some((node, matched.consumed))
    }
}

struct CodePairBounds {
    marker_len: usize,
    consumed: usize,
    content_start: usize,
    content_end: usize,
}

struct CodePairMatch {
    marker_len: usize,
    consumed: usize,
    content_start: usize,
    content_end: usize,
    content: String,
}

fn scan_code_pair<const MARKER: char>(
    src: &str,
    start: usize,
    end: usize,
    follows_marker: bool,
    inline_ext: &mut crate::parser::extset::InlineRootExtSet,
) -> Option<CodePairMatch> {
    let bounds = scan_code_pair_bounds::<MARKER>(src, start, end, follows_marker, inline_ext)?;

    let mut mapped_start = bounds.content_start;
    let mut mapped_end = bounds.content_end;
    let mut content = src[bounds.content_start..bounds.content_end].replace('\n', " ");
    if content.starts_with(' ') && content.ends_with(' ') && content.chars().any(|ch| ch != ' ') {
        content = content[1..content.len() - 1].to_owned();
        mapped_start += 1;
        mapped_end -= 1;
    }

    Some(CodePairMatch {
        marker_len: bounds.marker_len,
        consumed: bounds.consumed,
        content_start: mapped_start,
        content_end: mapped_end,
        content,
    })
}

fn scan_code_pair_bounds<const MARKER: char>(
    src: &str,
    start: usize,
    end: usize,
    follows_marker: bool,
    inline_ext: &mut crate::parser::extset::InlineRootExtSet,
) -> Option<CodePairBounds> {
    let marker_width = MARKER.len_utf8();
    if !src[start..end].starts_with(MARKER) || follows_marker {
        return None;
    }

    let mut content_start = start;
    let mut marker_len = 0;
    while src[content_start..end].starts_with(MARKER) {
        marker_len += 1;
        content_start += marker_width;
    }

    let cache = inline_ext.get_or_insert_default::<CodePairCache<MARKER>>();
    if cache.scanned && cache.max.get(marker_len).copied().unwrap_or(0) <= start {
        return None;
    }

    let mut search = content_start;
    while let Some(offset) = src[search..end].find(MARKER) {
        let closer_start = search + offset;
        let mut closer_end = closer_start;
        let mut closer_len = 0;
        while src[closer_end..end].starts_with(MARKER) {
            closer_len += 1;
            closer_end += marker_width;
        }

        if closer_len == marker_len {
            return Some(CodePairBounds {
                marker_len,
                consumed: closer_end - start,
                content_start,
                content_end: closer_start,
            });
        }

        let cache = inline_ext.get_mut::<CodePairCache<MARKER>>().unwrap();
        while cache.max.len() <= closer_len {
            cache.max.push(0);
        }
        cache.max[closer_len] = closer_start;
        search = closer_end;
    }

    inline_ext
        .get_mut::<CodePairCache<MARKER>>()
        .unwrap()
        .scanned = true;
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::extset::InlineRootExtSet;

    #[test]
    fn bounds_scan_unicode_markers_without_building_content() {
        let source = "雪 content 雪";
        let mut scratch = InlineRootExtSet::new();
        let matched =
            scan_code_pair_bounds::<'雪'>(source, 0, source.len(), false, &mut scratch).unwrap();

        assert_eq!(matched.marker_len, 1);
        assert_eq!(matched.consumed, source.len());
        assert_eq!(matched.content_start, '雪'.len_utf8());
        assert_eq!(matched.content_end, source.len() - '雪'.len_utf8());
    }

    #[test]
    fn bounds_reject_continuation_and_unclosed_runs() {
        let mut scratch = InlineRootExtSet::new();
        assert!(
            scan_code_pair_bounds::<'雪'>("x雪", 0, "x雪".len(), false, &mut scratch).is_none()
        );

        let source = "`x``";
        let mut scratch = InlineRootExtSet::new();
        assert!(
            scan_code_pair_bounds::<'`'>(source, 0, source.len(), false, &mut scratch).is_none()
        );
        // The unclosed two-marker closer was cached; the marker run at its
        // start must not be reused as a shorter opener.
        assert!(
            scan_code_pair_bounds::<'`'>(source, 2, source.len(), false, &mut scratch).is_none()
        );
        // A following marker run is rejected outright.
        assert!(
            scan_code_pair_bounds::<'`'>(source, 3, source.len(), true, &mut scratch).is_none()
        );
    }
}

#[cfg(test)]
mod adapter_tests {
    use super::*;
    use crate::parser::extset::InlineRootExtSet;

    #[test]
    fn adapter_keeps_trimmed_source_map_boundaries() {
        let source = "` hi `";
        let mut scratch = InlineRootExtSet::new();
        let matched = scan_code_pair::<'`'>(source, 0, source.len(), false, &mut scratch).unwrap();

        assert_eq!(matched.content, "hi");
        assert_eq!(matched.content_start, 2);
        assert_eq!(matched.content_end, 4);
        assert_eq!(matched.consumed, source.len());
    }

    #[test]
    fn adapter_replaces_newlines_in_content() {
        let source = "`a\nb`";
        let mut scratch = InlineRootExtSet::new();
        let matched = scan_code_pair::<'`'>(source, 0, source.len(), false, &mut scratch).unwrap();

        assert_eq!(matched.content, "a b");
        assert_eq!(matched.content_start, 1);
        assert_eq!(matched.content_end, 4);
    }
}
