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
use crate::parser::inline::{InlineRule, InlineState, LegacyInlineRule, Text};
use crate::{MarkdownIt, Node};

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
            let mut mapped_start = content_start;
            let mut mapped_end = closer_start;
            let mut content = src[content_start..closer_start].replace('\n', " ");
            if content.starts_with(' ')
                && content.ends_with(' ')
                && content.chars().any(|ch| ch != ' ')
            {
                content = content[1..content.len() - 1].to_owned();
                mapped_start += 1;
                mapped_end -= 1;
            }
            return Some(CodePairMatch {
                marker_len,
                consumed: closer_end - start,
                content_start: mapped_start,
                content_end: mapped_end,
                content,
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
