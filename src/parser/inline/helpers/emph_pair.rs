//! Structure similar to `*emphasis*` with configurable markers of fixed length.
//!
//! There are many structures in various markdown flavors that
//! can be implemented with this, namely:
//!
//!  - `*emphasis*` or `_emphasis_` -> `<em>emphasis</em>`
//!  - `**strong**` or `__strong__` -> `<strong>strong</strong>`
//!  - `~~strikethrough~~` -> `<s>strikethrough</s>`
//!  - `==marked==` -> `<mark>marked</mark>`
//!  - `++inserted++` -> `<ins>inserted</ins>`
//!  - `~subscript~` -> `<sub>subscript</sub>`
//!  - `^superscript^` -> `<sup>superscript</sup>`
//!
//! You add a custom structure by using [add_with] function, which takes following arguments:
//!  - `MARKER` - marker character
//!  - `LENGTH` - length of the opening/closing marker (can be 1, 2 or 3)
//!  - `CAN_SPLIT_WORD` - whether this structure can be found in the middle of the word
//!    (for example, note the difference between `foo*bar*baz` and `foo_bar_baz`
//!    in CommonMark - first one is an emphasis, second one isn't)
//!  - `md` - parser instance
//!  - `f` - function that should return your custom [NodeId]
//!
//! Here is an example of implementing superscript in your custom code:
//!
//! ```rust
//! use markdown_it::parser::inline::helpers::emph_pair;
//! use markdown_it::{MarkdownIt, NodeId, NodeValue, NodeRef, DocumentNodeRenderer, DocumentRenderContext, DocumentWriter};
//! #[derive(Debug)]
//! struct Superscript;
//! impl NodeValue for Superscript {}
//! struct CustomRenderer;
//! impl DocumentNodeRenderer<Superscript> for CustomRenderer {
//!     fn render(&self, node: NodeRef<'_>, _: &Superscript, ctx: &mut DocumentRenderContext<'_>, out: &mut DocumentWriter) {
//!         out.write_str("<sup>"); ctx.render_children(node.id(), out); out.write_str("</sup>");
//!     }
//! }
//! let md = &mut MarkdownIt::empty();
//! emph_pair::add_with::<'^', 1, true>(md, |document| document.create_node(Superscript));
//! md.add_document_renderer::<Superscript, _>("html", CustomRenderer);
//! assert_eq!(md.render("e^iπ^+1=0").trim(), "e<sup>iπ</sup>+1=0");
//! ```
//!
//! Note that these structures have lower priority than the rest of the rules,
//! e.g. `` *foo`bar*baz` `` is parsed as `*foo<code>bar*baz</code>`.
//!
use std::cmp::min;

use crate::MarkdownIt;
use crate::common::sourcemap::SourcePos;
use crate::document::{Document, NodeId, NodeValue, Text};
use crate::parser::inline::{DocumentInlineState, InlineRule};

type NodeIdFn = fn(&mut Document) -> NodeId;

#[derive(Debug, Default)]
struct PairConfig<const MARKER: char> {
    inserted: bool,
    fns: [Option<NodeIdFn>; 3],
}

#[derive(Debug, Default)]
struct OpenersBottom<const MARKER: char>([usize; 6]);

#[derive(Debug, Clone)]
#[doc(hidden)]
pub struct EmphMarker {
    // Starting marker
    pub marker: char,

    // Total length of these series of delimiters.
    pub length: usize,

    // Remaining length that's not already matched to other delimiters.
    pub remaining: usize,

    // Boolean flags that determine if this delimiter could open or close
    // an emphasis.
    pub open: bool,

    // Boolean flags that determine if this delimiter could open or close
    // an emphasis.
    pub close: bool,
}

// this node is supposed to be replaced by actual emph or text node
impl NodeValue for EmphMarker {}

pub fn add_with<const MARKER: char, const LENGTH: u8, const CAN_SPLIT_WORD: bool>(
    md: &mut MarkdownIt,
    f: fn(&mut Document) -> NodeId,
) {
    let pair_config = md.ext.get_or_insert_default::<PairConfig<MARKER>>();
    pair_config.fns[LENGTH as usize - 1] = Some(f);

    if !pair_config.inserted {
        pair_config.inserted = true;
        let builder = md
            .inline
            .add_rule_with_finalize::<EmphPairScanner<MARKER, CAN_SPLIT_WORD>>(
                finalize_emphasis_document,
            );
        if MARKER == '*' || MARKER == '_' {
            builder.alias_named("emphasis");
        } else if MARKER == '~' {
            builder.alias_named("strikethrough");
        }
    }
}

#[doc(hidden)]
pub struct EmphPairScanner<const MARKER: char, const CAN_SPLIT_WORD: bool>;

impl<const MARKER: char, const CAN_SPLIT_WORD: bool> InlineRule
    for EmphPairScanner<MARKER, CAN_SPLIT_WORD>
{
    const MARKER: char = MARKER;
    const NAMES: &'static [&'static str] = &["emph_pair"];

    fn check(_context: &mut DocumentInlineState<'_>) -> Option<usize> {
        None
    }

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        if state.remaining().chars().next()? != MARKER {
            return None;
        }

        let scanned = super::delimiters::scan_delimiter_run(
            state.markdown_it(),
            &state.src,
            state.pos,
            state.pos_max,
            CAN_SPLIT_WORD,
        );
        let scanned_bytes = scanned.byte_length();

        state.flush_text();

        let mut closer = state.document.create_node(EmphMarker {
            marker: MARKER,
            length: scanned.length,
            remaining: scanned.length,
            open: scanned.can_open,
            close: scanned.can_close,
        });
        let srcmap = state.get_map(state.pos, state.pos + scanned_bytes);
        state.document.node_mut(closer).set_srcmap(srcmap);

        closer = scan_and_match_document::<MARKER>(state, closer);

        let map = state
            .document
            .node(closer)
            .srcmap()
            .unwrap()
            .get_byte_offsets();
        // backtrack to keep correct source maps
        state.pos += scanned_bytes;
        let token_len = map.1 - map.0;
        state.pos -= token_len;

        Some((Some(closer), token_len))
    }
}

/// Assuming the last node is a closing delimiter we just inserted,
/// try to find opener(s). If any are found, move stuff to nested emph node.
fn scan_and_match_document<const MARKER: char>(
    state: &mut DocumentInlineState,
    closer_token: NodeId,
) -> NodeId {
    if state.nodes().is_empty() {
        return closer_token;
    } // must have at least opener and closer

    let mut closer = state
        .document
        .node(closer_token)
        .cast::<EmphMarker>()
        .unwrap()
        .clone();
    if !closer.close {
        return closer_token;
    }

    // Previously calculated lower bounds (previous fails)
    // for each marker, each delimiter length modulo 3,
    // and for whether this closer can be an opener;
    // https://github.com/commonmark/cmark/commit/34250e12ccebdc6372b8b49c44fab57c72443460
    let openers_parameter = (closer.open as usize) * 3 + closer.length % 3;
    let min_opener_idx = state
        .inline_ext
        .get_or_insert_default::<OpenersBottom<MARKER>>()
        .0[openers_parameter];

    let mut idx = state.nodes().len() - 1;
    let mut new_min_opener_idx = idx;
    while idx > min_opener_idx {
        idx -= 1;

        let Some(opener) = state.document.node(state.nodes()[idx]).cast::<EmphMarker>() else {
            continue;
        };

        let mut opener = opener.clone();
        if opener.open && opener.marker == closer.marker && !is_odd_match(&opener, &closer) {
            while closer.remaining > 0 && opener.remaining > 0 {
                let max_marker_len = min(3, min(opener.remaining, closer.remaining));
                let mut matched_rule = None;
                // PairConfig lives on the parser (not in the per-root ext set):
                // `inline_ext` only holds state scoped to this inline run.
                let fns = state
                    .markdown_it()
                    .ext
                    .get::<PairConfig<MARKER>>()
                    .unwrap()
                    .fns;
                for marker_len in (1..=max_marker_len).rev() {
                    if let Some(f) = fns[marker_len - 1] {
                        matched_rule = Some((marker_len, f));
                        break;
                    }
                }

                // If matched_fn isn't found, it can only mean that function is defined for larger marker
                // than we have (e.g. function defined for **, we have *).
                // Treat this as "marker not found".
                if matched_rule.is_none() {
                    break;
                }

                let (marker_len, marker_fn) = matched_rule.unwrap();
                let mark_bytes = marker_len * MARKER.len_utf8(); // UTF-8 marker support

                closer.remaining -= marker_len;
                opener.remaining -= marker_len;

                let new_token = marker_fn(state.document);
                let children = state.nodes_mut().split_off(idx + 1);
                state.document.attach_children(new_token, children);

                // cut marker_len chars from start, i.e. "12345" -> "345"
                let mut end_map_pos = 0;
                if let Some(map) = state.document.node(closer_token).srcmap() {
                    let (start, end) = map.get_byte_offsets();
                    state
                        .document
                        .node_mut(closer_token)
                        .set_srcmap(Some(SourcePos::new(start + mark_bytes, end)));
                    end_map_pos = start + mark_bytes;
                }

                // cut marker_len chars from end, i.e. "12345" -> "123"
                let mut start_map_pos = 0;
                let opener_token = *state.nodes().last().unwrap();
                if let Some(map) = state.document.node(opener_token).srcmap() {
                    let (start, end) = map.get_byte_offsets();
                    state
                        .document
                        .node_mut(opener_token)
                        .set_srcmap(Some(SourcePos::new(start, end - mark_bytes)));
                    start_map_pos = end - mark_bytes;
                }

                state
                    .document
                    .node_mut(new_token)
                    .set_srcmap(Some(SourcePos::new(start_map_pos, end_map_pos)));

                // remove empty node as a small optimization so we can do less work later
                if opener.remaining == 0 {
                    let removed = state.nodes_mut().pop().unwrap();
                    state.document.discard_node(removed);
                }

                new_min_opener_idx = 0;
                state.nodes_mut().push(new_token);
            }
        }

        if opener.remaining > 0 {
            let id = state.nodes()[idx];
            *state
                .document
                .node_mut(id)
                .cast_mut::<EmphMarker>()
                .unwrap() = opener; // reuse the Box, avoid unnecessary memory allocation
        } // otherwise node was already deleted

        if closer.remaining == 0 {
            break;
        }
    }

    if new_min_opener_idx != 0 {
        // If match for this delimiter run failed, we want to set lower bound for
        // future lookups. This is required to make sure algorithm has linear
        // complexity.
        //
        // See details here:
        // https://github.com/commonmark/cmark/issues/178#issuecomment-270417442
        //
        let openers_for_marker = state
            .inline_ext
            .get_or_insert_default::<OpenersBottom<MARKER>>();
        openers_for_marker.0[openers_parameter] = new_min_opener_idx;
    }

    // remove empty node as a small optimization so we can do less work later
    if closer.remaining > 0 {
        *state
            .document
            .node_mut(closer_token)
            .cast_mut::<EmphMarker>()
            .unwrap() = closer;
        closer_token
    } else {
        state.document.discard_node(closer_token);
        state.nodes_mut().pop().unwrap()
    }
}

fn is_odd_match(opener: &EmphMarker, closer: &EmphMarker) -> bool {
    // from spec:
    //
    // If one of the delimiters can both open and close emphasis, then the
    // sum of the lengths of the delimiter runs containing the opening and
    // closing delimiters must not be a multiple of 3 unless both lengths
    // are multiples of 3.
    //
    #[allow(clippy::collapsible_if)]
    if opener.close || closer.open {
        if (opener.length + closer.length).is_multiple_of(3) {
            if !opener.length.is_multiple_of(3) || !closer.length.is_multiple_of(3) {
                return true;
            }
        }
    }

    false
}

/// Clean up nodes after emphasis and strikethrough postprocessing:
/// merge adjacent text nodes into one and re-calculate all source maps.
///
/// This is necessary because initially emphasis delimiter markers (*, _, ~)
/// are treated as their own separate text nodes. Then the emphasis rule either
/// leaves them as text (needed to merge with adjacent text) or turns them
/// into opening/closing elements (which messes up the source maps inside).
///
fn fragments_join_children(document: &mut Document, nodes: &mut Vec<NodeId>) {
    let mut write = 0; // result
    for read in 0..nodes.len() {
        let id = nodes[read];
        let marker = document
            .node(id)
            .cast::<EmphMarker>()
            .map(|data| (data.marker, data.remaining));

        // empty test || no remaining chars marker
        if document
            .node(id)
            .cast::<Text>()
            .is_some_and(|text| text.content.is_empty())
            || marker.is_some_and(|(_, remaining)| remaining == 0)
        {
            document.discard_node(id);
            continue;
        }

        // whether it can be merged into the preceding result node
        if write > 0
            && document.node(nodes[write - 1]).is::<Text>()
            && (marker.is_some() || document.node(id).is::<Text>())
        {
            let previous = nodes[write - 1];

            if let Some((marker, remaining)) = marker {
                // Append unmatched delimiters without materializing a temporary
                // String or replacing a payload that will immediately be discarded.
                let previous = document.node_mut(previous);
                let content = &mut previous.cast_mut::<Text>().unwrap().content;
                content.reserve(remaining * marker.len_utf8());
                for _ in 0..remaining {
                    content.push(marker);
                }
            } else {
                let content =
                    std::mem::take(&mut document.node_mut(id).cast_mut::<Text>().unwrap().content);
                document
                    .node_mut(previous)
                    .cast_mut::<Text>()
                    .unwrap()
                    .content += &content;
            }

            // update source map
            if let (Some(first), Some(last)) =
                (document.node(previous).srcmap(), document.node(id).srcmap())
            {
                document.node_mut(previous).set_srcmap(Some(SourcePos::new(
                    first.get_byte_offsets().0,
                    last.get_byte_offsets().1,
                )));
            }

            document.discard_node(id);
        } else {
            if let Some((marker, remaining)) = marker {
                let mut encoded = [0; 4];
                let content = marker.encode_utf8(&mut encoded).repeat(remaining);
                document.node_mut(id).replace_value(Text { content });
            }
            nodes[write] = id;
            write += 1;
        }
    }

    nodes.truncate(write);
}

fn finalize_emphasis_document(state: &mut DocumentInlineState<'_>) {
    let mut nodes = std::mem::take(state.nodes_mut());
    fragments_join_children(state.document, &mut nodes);
    let mut pending = nodes.clone();
    while let Some(parent) = pending.pop() {
        if state.document.children(parent).is_empty() {
            continue;
        }
        state
            .document
            .rewrite_children(parent, |document, children| {
                fragments_join_children(document, children);
                pending.extend_from_slice(children);
            });
    }
    *state.nodes_mut() = nodes;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Preset;

    #[test]
    fn joining_markers_preserves_first_node_metadata_and_source_maps() {
        let mut document = Document::new("", crate::document::Root::new(""));
        let first = document.create_node(EmphMarker {
            marker: '🦀',
            length: 2,
            remaining: 2,
            open: false,
            close: false,
        });
        document
            .node_mut(first)
            .set_srcmap(Some(SourcePos::new(0, 8)));
        document
            .node_mut(first)
            .attrs_mut()
            .push(("class".into(), "kept".into()));
        document.node_mut(first).ext_mut().insert(42_u8);
        let empty = document.create_node(Text {
            content: String::new(),
        });
        let text = document.create_node(Text {
            content: "雪".into(),
        });
        document
            .node_mut(text)
            .set_srcmap(Some(SourcePos::new(8, 11)));
        let last = document.create_node(EmphMarker {
            marker: '_',
            length: 3,
            remaining: 1,
            open: false,
            close: true,
        });
        document
            .node_mut(last)
            .set_srcmap(Some(SourcePos::new(11, 12)));
        let barrier = document.create_node(CustomEmphasis);
        let trailing = document.create_node(EmphMarker {
            marker: '🦀',
            length: 1,
            remaining: 1,
            open: false,
            close: false,
        });
        let mut nodes = vec![first, empty, text, last, barrier, trailing];

        fragments_join_children(&mut document, &mut nodes);

        assert_eq!(nodes, vec![first, barrier, trailing]);
        assert_eq!(
            document.node(first).cast::<Text>().unwrap().content,
            "🦀🦀雪_"
        );
        assert_eq!(
            document.node(first).srcmap().unwrap().get_byte_offsets(),
            (0, 12)
        );
        assert_eq!(document.node(first).attrs()[0].1, "kept");
        assert_eq!(document.node(first).ext().get::<u8>(), Some(&42));
        assert_eq!(
            document.node(trailing).cast::<Text>().unwrap().content,
            "🦀"
        );
        assert!(document.node(trailing).srcmap().is_none());
        for removed in [empty, text, last] {
            assert!(document.get_node(removed).is_none());
        }
    }

    #[test]
    fn joining_markers_keeps_missing_source_map_and_discards_empty_markers() {
        let mut document = Document::new("", crate::document::Root::new(""));
        let first = document.create_node(Text {
            content: "a".into(),
        });
        let marker = document.create_node(EmphMarker {
            marker: '_',
            length: 1,
            remaining: 1,
            open: false,
            close: true,
        });
        document
            .node_mut(marker)
            .set_srcmap(Some(SourcePos::new(1, 2)));
        let empty = document.create_node(EmphMarker {
            marker: '*',
            length: 1,
            remaining: 0,
            open: false,
            close: false,
        });
        let mut nodes = vec![first, marker, empty];
        fragments_join_children(&mut document, &mut nodes);
        assert_eq!(nodes, vec![first]);
        assert_eq!(document.node(first).cast::<Text>().unwrap().content, "a_");
        assert!(document.node(first).srcmap().is_none());
        assert!(document.get_node(empty).is_none());
    }

    fn run(input: &str, output: &str) {
        let md = &mut MarkdownIt::with_preset(Preset::CommonMark);
        let node = md.parse_document(input);

        for event in node.events(node.root()) {
            assert!(event.node().srcmap().is_some());
        }
        assert_eq!(md.render_document(&node), output);
    }

    /// Parse with only the CommonMark paragraph + emphasis rules registered
    /// (nothing else), and check the expected rendered output.
    fn run_minimal(input: &str, output: &str) {
        let md = &mut MarkdownIt::empty();
        crate::plugins::cmark::block::paragraph::add(md);
        crate::plugins::cmark::inline::emphasis::add(md);

        let node = md.parse_document(input);
        for event in node.events(node.root()) {
            assert!(event.node().srcmap().is_some());
        }
        assert_eq!(
            md.render_document(&node),
            output,
            "document parser for {input:?}"
        );
    }

    #[test]
    fn cmark_delimiter_cases_with_minimal_registration() {
        let cases: [(&str, &str); 11] = [
            ("*foo*", "<p><em>foo</em></p>\n"),
            ("**foo**", "<p><strong>foo</strong></p>\n"),
            ("***foo***", "<p><em><strong>foo</strong></em></p>\n"),
            ("_foo_bar_baz_", "<p><em>foo_bar_baz</em></p>\n"),
            ("foo *_*", "<p>foo <em>_</em></p>\n"),
            ("*foo**bar*", "<p><em>foo**bar</em></p>\n"),
            ("*foo _bar* baz_", "<p><em>foo _bar</em> baz_</p>\n"),
            (
                "***foo** bar*",
                "<p><em><strong>foo</strong> bar</em></p>\n",
            ),
            (
                "****foo****",
                "<p><strong><strong>foo</strong></strong></p>\n",
            ),
            ("unmatched * marker", "<p>unmatched * marker</p>\n"),
            ("雪 **强调** 雨", "<p>雪 <strong>强调</strong> 雨</p>\n"),
        ];

        for (input, expected) in cases {
            run_minimal(input, expected);
        }
    }

    #[test]
    fn intraword_underscores_inside_emph() {
        run("_foo_bar_baz_", "<p><em>foo_bar_baz</em></p>\n");
    }

    #[test]
    fn lone_underscore_inside_emph() {
        run("foo *_*", "<p>foo <em>_</em></p>\n");
    }

    #[test]
    fn crossed_delimiters_leave_marker_inside_emph() {
        run("*foo _bar* baz_", "<p><em>foo _bar</em> baz_</p>\n");
    }

    #[derive(Debug)]
    struct CustomEmphasis;

    impl NodeValue for CustomEmphasis {}

    struct CustomRenderer;
    impl crate::DocumentNodeRenderer<CustomEmphasis> for CustomRenderer {
        fn render(
            &self,
            node: crate::NodeRef<'_>,
            _: &CustomEmphasis,
            ctx: &mut crate::DocumentRenderContext<'_>,
            out: &mut crate::DocumentWriter,
        ) {
            out.write_str("<x>");
            ctx.render_children(node.id(), out);
            out.write_str("</x>");
        }
    }
    #[test]
    fn unicode_marker_uses_byte_offsets_for_source_maps() {
        let mut md = MarkdownIt::empty();
        crate::plugins::cmark::block::paragraph::add(&mut md);

        md.add_document_renderer::<CustomEmphasis, _>("html", CustomRenderer);
        add_with::<'🦀', 1, true>(&mut md, |document| document.create_node(CustomEmphasis));

        let root = md.parse_document("a 🦀雪🦀 b");

        assert_eq!(md.render_document(&root), "<p>a <x>雪</x> b</p>\n",);

        let paragraph = root.children(root.root())[0];
        let wrapper = root.node(root.children(paragraph)[1]);
        assert_eq!(wrapper.srcmap().unwrap().get_byte_offsets(), (2, 13),);
    }

    #[test]
    fn repeated_unicode_marker_counts_chars_and_consumes_bytes() {
        let mut md = MarkdownIt::empty();
        crate::plugins::cmark::block::paragraph::add(&mut md);

        md.add_document_renderer::<CustomEmphasis, _>("html", CustomRenderer);
        add_with::<'🦀', 2, true>(&mut md, |document| document.create_node(CustomEmphasis));

        let root = md.parse_document("🦀🦀雪🦀🦀");

        assert_eq!(md.render_document(&root), "<p><x>雪</x></p>\n",);

        let paragraph = root.children(root.root())[0];
        let wrapper = root.node(root.children(paragraph)[0]);
        assert_eq!(wrapper.srcmap().unwrap().get_byte_offsets(), (0, 19),);

        let text = root.node(wrapper.children()[0]);
        assert_eq!(text.srcmap().unwrap().get_byte_offsets(), (8, 11),);
    }
}
