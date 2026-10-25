//! Fixed-length paired delimiters, such as `*emphasis*` and `~~strikethrough~~`.
//!
//! Common uses include:
//!
//!  - `*emphasis*` or `_emphasis_` -> `<em>emphasis</em>`
//!  - `**strong**` or `__strong__` -> `<strong>strong</strong>`
//!  - `~~strikethrough~~` -> `<s>strikethrough</s>`
//!  - `==marked==` -> `<mark>marked</mark>`
//!  - `++inserted++` -> `<ins>inserted</ins>`
//!  - `~subscript~` -> `<sub>subscript</sub>`
//!  - `^superscript^` -> `<sup>superscript</sup>`
//!
//! Register a custom pair with [add_with]:
//!  - `MARKER`: delimiter character
//!  - `LENGTH`: delimiter length (1–3)
//!  - `CAN_SPLIT_WORD`: allow pairs inside words. For example, CommonMark treats
//!    `foo*bar*baz` as emphasis but not `foo_bar_baz`.
//!  - `md`: parser instance
//!  - `f`: returns the node to create
//!
//! Superscript example:
//!
//! ```rust
//! use markdown_it::parser::inline::helpers::emph_pair;
//! use markdown_it::{MarkdownIt, NodeId, NodeRef, NodeRenderer, RenderContext, Writer};
//! #[derive(Debug)]
//! struct Superscript;
//! struct CustomRenderer;
//! impl NodeRenderer<Superscript> for CustomRenderer {
//!     fn render(&self, node: NodeRef<'_>, _: &Superscript, ctx: &mut RenderContext<'_>, out: &mut Writer) {
//!         out.write_str("<sup>"); ctx.render_children(node.id(), out); out.write_str("</sup>");
//!     }
//! }
//! let md = &mut MarkdownIt::empty();
//! emph_pair::add_with::<'^', 1, true>(md, |document| document.create_node(Superscript));
//! md.add_renderer::<Superscript, _>("html", CustomRenderer);
//! assert_eq!(md.render("e^iπ^+1=0").trim(), "e<sup>iπ</sup>+1=0");
//! ```
//!
//! These rules run after most inline rules, so `` *foo`bar*baz` `` parses as
//! `*foo<code>bar*baz</code>`.

use std::cmp::min;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use crate::document::{Document, NodeId, Text};
use crate::parser::core::{CoreRule, CoreState};
use crate::parser::inline::{InlineParserRule, InlineRule, InlineState};
use crate::MarkdownIt;

// --- pub ---

pub fn add_with<const MARKER: char, const LENGTH: u8, const CAN_SPLIT_WORD: bool>(
    md: &mut MarkdownIt,
    f: fn(&mut Document) -> NodeId,
) {
    register::<MARKER, LENGTH, CAN_SPLIT_WORD>(md, PairFactory::Static(f));
}

/// Like [`add_with`], but accepts a factory that may capture configuration.
///
/// All lengths of a marker must agree on `CAN_SPLIT_WORD`. Captured factories
/// cannot replace an already registered length.
pub fn add_with_factory<const MARKER: char, const LENGTH: u8, const CAN_SPLIT_WORD: bool>(
    md: &mut MarkdownIt,
    factory: impl Fn(&mut Document) -> NodeId + Send + Sync + 'static,
) {
    register::<MARKER, LENGTH, CAN_SPLIT_WORD>(md, PairFactory::Captured(Arc::new(factory)));
}

// --- inner ---

type NodeIdFn = fn(&mut Document) -> NodeId;

#[derive(Clone)]
enum PairFactory {
    Static(NodeIdFn),
    Captured(Arc<dyn Fn(&mut Document) -> NodeId + Send + Sync>),
}

impl PairFactory {
    fn create(&self, document: &mut Document) -> NodeId {
        match self {
            Self::Static(f) => f(document),
            Self::Captured(f) => f(document),
        }
    }
}

impl fmt::Debug for PairFactory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Static(_) => "Static",
            Self::Captured(_) => "Captured",
        })
    }
}

#[derive(Debug, Default)]
struct PairConfig<const MARKER: char> {
    can_split_word: Option<bool>,
    fns: [Option<PairFactory>; 3],
}

/// Snapshot of current registrations, used to validate a batch before installing rules.
#[derive(Debug, Default)]
pub(crate) struct PairRegistration {
    can_split_word: Option<bool>,
    occupied: u8,
}

impl PairRegistration {
    pub(crate) fn claim(&mut self, marker: char, length: u8, split: bool) {
        assert!(
            (1..=3).contains(&length),
            "emph_pair marker length must be 1, 2 or 3"
        );

        if let Some(existing) = self.can_split_word {
            assert_eq!(
                existing, split,
                "inline pairs using marker {marker:?}: can_split_word must agree"
            );
        }

        let bit = 1 << (length - 1);
        assert!(
            self.occupied & bit == 0,
            "inline pair marker {marker:?} length {length} is already registered"
        );

        self.can_split_word = Some(split);
        self.occupied |= bit;
    }
}

pub(crate) fn registration<const MARKER: char>(md: &MarkdownIt) -> PairRegistration {
    let Some(config) = md.ext.get::<PairConfig<MARKER>>() else {
        return PairRegistration::default();
    };

    PairRegistration {
        can_split_word: config.can_split_word,
        occupied: config
            .fns
            .iter()
            .enumerate()
            .fold(0, |mask, (index, f)| mask | ((f.is_some() as u8) << index)),
    }
}

fn register<const MARKER: char, const LENGTH: u8, const CAN_SPLIT_WORD: bool>(
    md: &mut MarkdownIt,
    factory: PairFactory,
) {
    assert!(
        (1..=3).contains(&LENGTH),
        "emph_pair marker length must be 1, 2 or 3"
    );

    let index = LENGTH as usize - 1;
    let mut claim = registration::<MARKER>(md);
    // Keep add_with's behavior of replacing an existing static factory.
    if matches!(factory, PairFactory::Static(_))
        && md
            .ext
            .get::<PairConfig<MARKER>>()
            .is_some_and(|config| matches!(config.fns[index], Some(PairFactory::Static(_))))
    {
        claim.occupied &= !(1 << index);
    }
    claim.claim(MARKER, LENGTH, CAN_SPLIT_WORD);

    if !md.has_rule::<EmphasisCleanup>() {
        md.add_rule::<EmphasisCleanup>()
            .after::<InlineParserRule>()
            .before_named("linkify")
            .before_named("extra::typographer")
            .before_named("smartquotes")
            .before_all();
    }

    let config = md.ext.get_or_insert_default::<PairConfig<MARKER>>();
    let needs_scanner = config.can_split_word.is_none();
    config.can_split_word = Some(CAN_SPLIT_WORD);
    config.fns[index] = Some(factory);
    if needs_scanner {
        let builder = md
            .inline
            .add_rule::<EmphPairScanner<MARKER, CAN_SPLIT_WORD>>();
        if MARKER == '*' || MARKER == '_' {
            builder.alias_named("emphasis");
        } else if MARKER == '~' {
            builder.alias_named("strikethrough");
        }
    }
}

// --- scanner ---

#[doc(hidden)]
pub struct EmphPairScanner<const MARKER: char, const CAN_SPLIT_WORD: bool>;

impl<const MARKER: char, const CAN_SPLIT_WORD: bool> InlineRule
    for EmphPairScanner<MARKER, CAN_SPLIT_WORD>
{
    const MARKER: char = MARKER;
    const NAMES: &'static [&'static str] = &["emph_pair"];

    fn check(_context: &mut InlineState<'_>) -> Option<usize> {
        None
    }

    fn run(state: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        if state.remaining().chars().next()? != MARKER {
            return None;
        }

        let (src, start, end) = state
            .inline_ext
            .get::<super::delimiters::EmphasisSourceStart>()
            .map_or((state.src.as_ref(), state.pos, state.pos_max), |boundary| {
                (
                    &state.src[boundary.0..state.pos_max],
                    state.pos - boundary.0,
                    state.pos_max - boundary.0,
                )
            });
        let scanned = super::delimiters::scan_delimiter_run(
            state.markdown_it(),
            src,
            start,
            end,
            CAN_SPLIT_WORD,
        );
        let scanned_bytes = scanned.byte_length();

        // Avoid temporary nodes for non-opening runs before any node exists.
        // Preserve node indices and newline handling.
        if state.nodes().is_empty() && !MARKER.is_whitespace() && !scanned.can_open {
            state.push_text(state.pos, state.pos + scanned_bytes);
            return Some((None, scanned_bytes));
        }

        state.flush_text();

        let mut closer = state.document.create_node(EmphMarker {
            marker: MARKER,
            length: scanned.length,
            remaining: scanned.length,
            open: scanned.can_open,
            close: scanned.can_close,
        });
        #[cfg(feature = "sourcemap")]
        let srcmap = state.get_map(state.pos, state.pos + scanned_bytes);
        #[cfg(feature = "sourcemap")]
        state.document.node_mut(closer).set_srcmap(srcmap);

        closer = scan_and_match::<MARKER>(state, closer);

        // scan_and_match may set a map spanning earlier delimiters. Push directly
        // so tokenize does not overwrite it with this delimiter's span.
        state.nodes_mut().push(closer);
        Some((None, scanned_bytes))
    }
}

#[derive(Debug, Clone, Copy)]
#[doc(hidden)]
pub struct EmphMarker {
    // Delimiter character.
    pub marker: char,

    // Length of the delimiter run.
    pub length: usize,

    // Delimiters not yet matched.
    pub remaining: usize,

    // Whether this run can open or close a pair.
    pub open: bool,
    pub close: bool,
}

impl EmphMarker {
    /// Whether all delimiters in this run are matched.
    fn is_empty(&self) -> bool {
        self.remaining == 0
    }

    /// Append unmatched delimiters as literal text.
    fn append_to(&self, content: &mut String) {
        content.reserve(self.remaining * self.marker.len_utf8());
        for _ in 0..self.remaining {
            content.push(self.marker);
        }
    }

    /// Convert unmatched delimiters into a text payload.
    fn into_text(self) -> Text {
        let mut encoded = [0; 4];
        Text {
            content: self.marker.encode_utf8(&mut encoded).repeat(self.remaining),
        }
    }
}

/// Cached lower search bounds, indexed by marker and
/// `(closer.open as usize) * 3 + length_class`.
///
/// Values are node indices and must be clamped when earlier nodes are removed.
#[derive(Debug, Default)]
struct OpenersBottom(HashMap<char, [usize; 6]>);

impl OpenersBottom {
    fn get(&self, marker: char, parameter: usize) -> usize {
        self.0.get(&marker).map_or(0, |bounds| bounds[parameter])
    }

    fn set(&mut self, marker: char, parameter: usize, value: usize) {
        self.0.entry(marker).or_default()[parameter] = value;
    }

    /// Clamp bounds after nodes at or past `removal_index` were removed.
    fn rebase_after_removal(&mut self, removal_index: usize) {
        for bound in self.0.values_mut().flatten() {
            *bound = (*bound).min(removal_index);
        }
    }
}

/// Match the closing delimiter at the end of the node stack with an opener,
/// nesting any matched content under the new node.
fn scan_and_match<const MARKER: char>(state: &mut InlineState, closer_token: NodeId) -> NodeId {
    if state.nodes().is_empty() {
        return closer_token;
    } // must have at least opener and closer

    let mut closer = *state
        .document
        .node(closer_token)
        .cast::<EmphMarker>()
        .unwrap();
    if !closer.close {
        return closer_token;
    }

    // Lower bounds from previous failed searches, cached per marker, length
    // class, and whether the closer can also open.
    // https://github.com/commonmark/cmark/commit/34250e12ccebdc6372b8b49c44fab57c72443460
    let length_class = if matches!(MARKER, '*' | '_') {
        closer.length % 3
    } else {
        min(closer.length, 3) - 1
    };
    let openers_parameter = (closer.open as usize) * 3 + length_class;
    let min_opener_idx = state
        .inline_ext
        .get_or_insert_default::<OpenersBottom>()
        .get(MARKER, openers_parameter);

    // Snapshot factories to avoid repeated extension lookups.
    let fns = state
        .markdown_it()
        .ext
        .get::<PairConfig<MARKER>>()
        .unwrap()
        .fns
        .clone();

    let closer_idx = state.nodes().len() - 1;
    let mut idx = closer_idx;
    // Earliest opener removed by a match; later indices shift and need clamping.
    let mut removed_from: Option<usize> = None;
    while idx > min_opener_idx {
        idx -= 1;

        let Some(mut opener) = state
            .document
            .node(state.nodes()[idx])
            .cast::<EmphMarker>()
            .copied()
        else {
            continue;
        };

        if opener.open && opener.marker == closer.marker && !is_odd_match(&opener, &closer) {
            while closer.remaining > 0 && opener.remaining > 0 {
                let max_marker_len = min(3, min(opener.remaining, closer.remaining));
                // No factory means no pair is registered for these lengths.
                let Some((marker_len, marker_fn)) = (1..=max_marker_len)
                    .rev()
                    .find_map(|len| fns[len - 1].as_ref().map(|f| (len, f)))
                else {
                    break;
                };

                closer.remaining -= marker_len;
                opener.remaining -= marker_len;

                let new_token = marker_fn.create(state.document);
                let children = state.nodes_mut().split_off(idx + 1);
                state.document.attach_children(new_token, children);

                #[cfg(feature = "sourcemap")]
                {
                    let opener_token = *state.nodes().last().unwrap();
                    update_pair_sourcemaps(
                        state.document,
                        opener_token,
                        closer_token,
                        new_token,
                        marker_len * MARKER.len_utf8(),
                    );
                }

                // Drop the opener once fully matched.
                if opener.remaining == 0 {
                    let removed = state.nodes_mut().pop().unwrap();
                    state.document.discard_node(removed);
                }

                removed_from = Some(removed_from.map_or(idx, |earliest| earliest.min(idx)));
                state.nodes_mut().push(new_token);
            }
        }

        if opener.remaining > 0 {
            let id = state.nodes()[idx];
            *state
                .document
                .node_mut(id)
                .cast_mut::<EmphMarker>()
                .unwrap() = opener; // Reuse the allocation.
        } // otherwise node was already deleted

        if closer.remaining == 0 {
            break;
        }
    }

    let openers = state.inline_ext.get_or_insert_default::<OpenersBottom>();
    if let Some(removed_from) = removed_from {
        openers.rebase_after_removal(removed_from);
    } else {
        // Cache the failure bound to keep the scan linear.
        // https://github.com/commonmark/cmark/issues/178#issuecomment-270417442
        openers.set(MARKER, openers_parameter, closer_idx);
    }

    // Keep a partially matched closer so its remaining delimiters become text.
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
    // The rule of three applies only to CommonMark `*`/`_` emphasis.
    if !matches!(opener.marker, '*' | '_') {
        return false;
    }

    // If either delimiter can both open and close, their combined run length
    // must not be a multiple of 3 unless both lengths are.
    let adjacent = opener.close || closer.open;
    let both_multiples = opener.length.is_multiple_of(3) && closer.length.is_multiple_of(3);
    adjacent && (opener.length + closer.length).is_multiple_of(3) && !both_multiples
}

// --- post process ---

#[derive(Debug, Default)]
pub(crate) struct EmphasisCleanup;

impl CoreRule for EmphasisCleanup {
    const NAMES: &'static [&'static str] = &["emphasis_cleanup"];

    fn run(&self, state: &mut CoreState<'_>) {
        let document = &mut *state.document;
        let mut stack = vec![(document.root(), false)];
        while let Some((node, expanded)) = stack.pop() {
            if expanded {
                document.rewrite_children(node, |document, children| {
                    fragments_join_children(document, children);
                });
                continue;
            }

            if document.children(node).is_empty() {
                continue;
            }

            stack.push((node, true));
            for &child in document.children(node) {
                if !document.children(child).is_empty() {
                    stack.push((child, false));
                }
            }
        }
    }
}

/// Merge adjacent text nodes and fix source maps after emphasis matching.
///
/// Unmatched delimiters become text, while matched delimiters may leave source
/// maps split across nested nodes.
///
fn fragments_join_children(document: &mut Document, nodes: &mut Vec<NodeId>) {
    let mut write = 0; // result
    for read in 0..nodes.len() {
        let id = nodes[read];
        let marker = document.node(id).cast::<EmphMarker>().copied();

        // Empty text or a fully matched delimiter run.
        if document
            .node(id)
            .cast::<Text>()
            .is_some_and(|text| text.content.is_empty())
            || marker.is_some_and(|marker| marker.is_empty())
        {
            document.discard_node(id);
            continue;
        }

        // Can this merge into the previous text node?
        if write > 0
            && document.node(nodes[write - 1]).is::<Text>()
            && (marker.is_some() || document.node(id).is::<Text>())
        {
            let previous = nodes[write - 1];

            if let Some(marker) = marker {
                // Append directly to avoid a temporary String.
                let content = &mut document
                    .node_mut(previous)
                    .cast_mut::<Text>()
                    .unwrap()
                    .content;
                marker.append_to(content);
            } else {
                let content =
                    std::mem::take(&mut document.node_mut(id).cast_mut::<Text>().unwrap().content);
                document
                    .node_mut(previous)
                    .cast_mut::<Text>()
                    .unwrap()
                    .content += &content;
            }

            // Extend the source map to include this node.
            #[cfg(feature = "sourcemap")]
            if let (Some(first), Some(last)) =
                (document.node(previous).srcmap(), document.node(id).srcmap())
            {
                document.node_mut(previous).set_srcmap(Some(
                    crate::common::sourcemap::SourcePos::new(
                        first.get_byte_offsets().0,
                        last.get_byte_offsets().1,
                    ),
                ));
            }

            document.discard_node(id);
        } else {
            if let Some(marker) = marker {
                document.node_mut(id).replace_value(marker.into_text());
            }
            nodes[write] = id;
            write += 1;
        }
    }

    nodes.truncate(write);
}

#[cfg(feature = "sourcemap")]
fn update_pair_sourcemaps(
    document: &mut Document,
    opener: NodeId,
    closer: NodeId,
    pair: NodeId,
    mark_bytes: usize,
) {
    let (Some(opener_map), Some(closer_map)) = (
        document.node(opener).srcmap(),
        document.node(closer).srcmap(),
    ) else {
        return;
    };
    let (open_start, open_end) = opener_map.get_byte_offsets();
    let (close_start, close_end) = closer_map.get_byte_offsets();
    for (node, start, end) in [
        (opener, open_start, open_end - mark_bytes),
        (closer, close_start + mark_bytes, close_end),
        (pair, open_end - mark_bytes, close_start + mark_bytes),
    ] {
        document
            .node_mut(node)
            .set_srcmap(Some(crate::common::sourcemap::SourcePos::new(start, end)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Preset;

    #[test]
    fn literal_closers_stay_text_before_later_emphasis() {
        let md = MarkdownIt::with_preset(Preset::CommonMark);
        let source = "a_ ".repeat(10_000) + "**雪**";
        let document = md.parse(&source);
        let paragraph = document.children(document.root())[0];
        let children = document.children(paragraph);
        assert_eq!(children.len(), 2);
        assert_eq!(
            document.node(children[0]).cast::<Text>().unwrap().content,
            "a_ ".repeat(10_000)
        );
        #[cfg(feature = "sourcemap")]
        assert_eq!(
            document
                .node(children[0])
                .srcmap()
                .unwrap()
                .get_byte_offsets(),
            (0, 30_000)
        );
        #[cfg(feature = "sourcemap")]
        assert_eq!(
            document
                .node(children[1])
                .srcmap()
                .unwrap()
                .get_byte_offsets(),
            (30_000, source.len())
        );
        assert_eq!(
            md.render_html(&document),
            "<p>".to_owned() + &"a_ ".repeat(10_000) + "<strong>雪</strong></p>\n"
        );
    }

    #[test]
    fn literal_marker_runs_preserve_inline_boundaries() {
        for (source, expected) in [
            ("a_ *b*", "<p>a_ <em>b</em></p>\n"),
            ("_a_b_", "<p><em>a_b</em></p>\n"),
            ("a_b", "<p>a_b</p>\n"),
            ("a_ &amp; b_", "<p>a_ &amp; b_</p>\n"),
            ("a_ \\_ b_", "<p>a_ _ b_</p>\n"),
            ("a_  \nb_", "<p>a_<br />\nb_</p>\n"),
            ("a_ `b_` c_", "<p>a_ <code>b_</code> c_</p>\n"),
            (
                "[a_ **雪**](url)",
                "<p><a href=\"url\">a_ <strong>雪</strong></a></p>\n",
            ),
            ("*a_ b*", "<p><em>a_ b</em></p>\n"),
            (
                "*__[b_a_)___>_",
                "<p>*<strong>[b_a_)</strong><em>&gt;</em></p>\n",
            ),
            (
                "_`__a*_[a_雪*雪*",
                "<p><em>`__a*</em>[a_雪<em>雪</em></p>\n",
            ),
        ] {
            run(source, expected);
        }
    }

    #[test]
    fn cached_openers_bottom_is_rebased_after_matches_remove_nodes() {
        // A `*` match removes nodes; the cached `_` bound must still allow the
        // trailing `_b_` to match.
        for (source, expected) in [
            (" *a_(c_*_b_)9", "<p><em>a_(c_</em><em>b</em>)9</p>\n"),
            (
                " *)😀é\n90é(c_*_b_)9",
                "<p><em>)😀é\n90é(c_</em><em>b</em>)9</p>\n",
            ),
            (
                " *)😀é\r90é(c_*_b_)9!!(< foo -->\n\n    <!-- fo",
                "<p><em>)😀é\n90é(c_</em><em>b</em>)9!!(&lt; foo --&gt;</p>\n<pre><code>&lt;!-- fo\n</code></pre>\n",
            ),
            (
                "> *a_(c_*_b_)9",
                "<blockquote>\n<p><em>a_(c_</em><em>b</em>)9</p>\n</blockquote>\n",
            ),
        ] {
            run(source, expected);
        }
    }

    #[test]
    fn failed_closer_at_cached_bound_can_open_later_emphasis() {
        // A failed closer may still open a later pair; the bound is inclusive.
        run("a*b*c", "<p>a<em>b</em>c</p>\n");
    }

    #[test]
    #[cfg(feature = "sourcemap")]
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
            .set_srcmap(Some(crate::common::sourcemap::SourcePos::new(0, 8)));
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
            .set_srcmap(Some(crate::common::sourcemap::SourcePos::new(8, 11)));
        let last = document.create_node(EmphMarker {
            marker: '_',
            length: 3,
            remaining: 1,
            open: false,
            close: true,
        });
        document
            .node_mut(last)
            .set_srcmap(Some(crate::common::sourcemap::SourcePos::new(11, 12)));
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
    #[cfg(feature = "sourcemap")]
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
            .set_srcmap(Some(crate::common::sourcemap::SourcePos::new(1, 2)));
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
        let node = md.parse(input);

        #[cfg(feature = "sourcemap")]
        for event in node.events(node.root()) {
            assert!(event.node().srcmap().is_some());
        }
        assert_eq!(md.render_html(&node), output);
    }

    /// Parse with only CommonMark paragraph and emphasis rules.
    fn run_minimal(input: &str, output: &str) {
        let md = &mut MarkdownIt::empty();
        crate::plugins::cmark::block::paragraph::add(md);
        crate::plugins::cmark::inline::emphasis::add(md);

        let node = md.parse(input);
        #[cfg(feature = "sourcemap")]
        for event in node.events(node.root()) {
            assert!(event.node().srcmap().is_some());
        }
        assert_eq!(md.render_html(&node), output, "parser for {input:?}");
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

    struct CustomRenderer;
    impl crate::NodeRenderer<CustomEmphasis> for CustomRenderer {
        fn render(
            &self,
            node: crate::NodeRef<'_>,
            _: &CustomEmphasis,
            ctx: &mut crate::RenderContext<'_>,
            out: &mut crate::Writer,
        ) {
            out.write_str("<x>");
            ctx.render_children(node.id(), out);
            out.write_str("</x>");
        }
    }
    #[test]
    #[cfg(feature = "sourcemap")]
    fn unicode_marker_uses_byte_offsets_for_source_maps() {
        let mut md = MarkdownIt::empty();
        crate::plugins::cmark::block::paragraph::add(&mut md);

        md.add_renderer::<CustomEmphasis, _>("html", CustomRenderer);
        add_with::<'🦀', 1, true>(&mut md, |document| document.create_node(CustomEmphasis));

        let root = md.parse("a 🦀雪🦀 b");

        assert_eq!(md.render_html(&root), "<p>a <x>雪</x> b</p>\n",);

        let paragraph = root.children(root.root())[0];
        let wrapper = root.node(root.children(paragraph)[1]);
        assert_eq!(wrapper.srcmap().unwrap().get_byte_offsets(), (2, 13),);
    }

    #[test]
    fn repeated_unicode_marker_counts_chars_and_consumes_bytes() {
        let mut md = MarkdownIt::empty();
        crate::plugins::cmark::block::paragraph::add(&mut md);

        md.add_renderer::<CustomEmphasis, _>("html", CustomRenderer);
        add_with::<'🦀', 2, true>(&mut md, |document| document.create_node(CustomEmphasis));

        let root = md.parse("🦀🦀雪🦀🦀");

        assert_eq!(md.render_html(&root), "<p><x>雪</x></p>\n",);

        #[cfg(feature = "sourcemap")]
        {
            let paragraph = root.children(root.root())[0];
            let wrapper = root.node(root.children(paragraph)[0]);
            #[cfg(feature = "sourcemap")]
            assert_eq!(wrapper.srcmap().unwrap().get_byte_offsets(), (0, 19),);

            #[cfg(feature = "sourcemap")]
            let text = root.node(wrapper.children()[0]);
            #[cfg(feature = "sourcemap")]
            assert_eq!(text.srcmap().unwrap().get_byte_offsets(), (8, 11),);
        }
    }
}
