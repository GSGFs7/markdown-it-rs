//! Footnotes.
//!
//! This plugin supports named footnotes (`[^1]`) and inline footnotes (`^[2 notes]`).
//!
//! Named footnotes:
//!
//! ```md
//! Here is a footnote.[^note]
//!
//! [^note]: Footnote text.
//! ```
//!
//! A definition can span multiple lines when continuation lines are indented
//! by at least 4 spaces or a tab.
//!
//! ```md
//! Here is a footnote.[^note]
//!
//! [^note]: First paragraph.
//!
//!     Second paragraph.
//!     - list item
//! ```
//!
//! Inline footnotes use `^[...]`:
//!
//! ```md
//! Here is an inline footnote.^[Inline **markdown** is supported.]
//! ```
//!
//! use this plugin:
//!
//! ```rust
//! let mut md = markdown_it::MarkdownIt::empty();
//! markdown_it::plugins::cmark::add(&mut md);
//! markdown_it::plugins::extra::footnote::add(&mut md);
//!
//! let input = concat!(
//!     "Text[^named] and inline^[inline **note**].\n\n",
//!     "[^named]: first line\n",
//!     "    second line",
//! );
//! let html = md.render(input);
//!
//! assert!(html.contains(r##"href="#fn1""##));
//! assert!(html.contains(r##"href="#fn2""##));
//! assert!(html.contains("first line\nsecond line"));
//! assert!(html.contains("<strong>note</strong>"));
//! ```

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::{Arc, Mutex, OnceLock};

use crate::MarkdownIt;
use crate::common::extset::RootExtSet;
use crate::common::utils::normalize_reference;
use crate::document::{Document, Node, NodeId, StructuralEvent};
use crate::parser::block::{BlockRule, BlockState};
use crate::parser::core::{CoreRule, CoreState};
use crate::parser::inline::{InlineRule, InlineState};
use crate::render::{
    EmptyRenderer,
    NodeRenderer,
    PlainTextBlockRenderer,
    RenderContext,
    Writer,
    write_html_close,
    write_html_open,
    write_html_self_close,
    write_html_text,
};

const FOOTNOTE_INDENT: i32 = 4;
const NAMED_FOOTNOTE_PREFIX: &str = "[^";

// [^1]: somthing
#[derive(Debug)]
enum FootnoteDefinition {
    Named(String),
    Inline(usize),
}

// something[^1]
#[derive(Debug)]
struct FootnoteReference {
    number: usize,
    sub_id: usize,
}

impl FootnoteReference {
    fn item_id(number: usize) -> String {
        format!("fn{number}")
    }

    fn ref_id(number: usize, sub_id: usize) -> String {
        if sub_id == 1 {
            format!("fnref{number}")
        } else {
            format!("fnref{number}:{sub_id}")
        }
    }

    fn href(&self) -> String {
        format!("#{}", Self::item_id(self.number))
    }

    fn id(&self) -> String {
        Self::ref_id(self.number, self.sub_id)
    }
}

// rendered footnote definition at the bottom of HTML
#[derive(Debug)]
struct FootnoteSection;

// rendered footnote reference
#[derive(Debug)]
struct FootnoteItem {
    number: usize,
}

impl FootnoteItem {
    fn id(&self) -> String {
        FootnoteReference::item_id(self.number)
    }
}

#[derive(Debug)]
struct FootnoteBackref {
    number: usize,
    sub_id: usize,
}

impl FootnoteBackref {
    fn href(&self) -> String {
        format!("#{}", FootnoteReference::ref_id(self.number, self.sub_id))
    }
}

// --- plugin state ---

#[derive(Debug, Default)]
struct FootnoteEnv {
    /// Defined labels receive an index on their first reference.
    named: HashMap<String, Option<usize>>,
    /// One count per numbered footnote, in first-reference order.
    ref_counts: Vec<usize>,
    label_suffixes: OnceLock<LabelSuffixes>,
}

impl FootnoteEnv {
    fn define(&mut self, label: String) {
        if let std::collections::hash_map::Entry::Vacant(entry) = self.named.entry(label) {
            entry.insert(None);
            self.label_suffixes.take();
        }
    }

    fn reference_named(&mut self, label: &str) -> Option<FootnoteReference> {
        let index = *self.named.get_mut(label)?.get_or_insert_with(|| {
            let index = self.ref_counts.len();
            self.ref_counts.push(0);
            index
        });
        self.ref_counts[index] += 1;
        Some(FootnoteReference {
            number: index + 1,
            sub_id: self.ref_counts[index],
        })
    }

    fn reference_inline(&mut self) -> usize {
        let index = self.ref_counts.len();
        self.ref_counts.push(1);
        index
    }
}

#[derive(Debug)]
struct FootnoteScanBudget {
    ends: HashMap<(usize, usize, u32, i32, u32), Option<usize>>,
    remaining_steps: usize,
}

impl FootnoteScanBudget {
    fn new(len: usize) -> Self {
        Self {
            ends: HashMap::new(),
            remaining_steps: len.saturating_mul(FootnoteScans::STEPS_PER_BYTE),
        }
    }
}

#[derive(Debug)]
struct FootnoteScanState {
    budget: FootnoteScanBudget,
    scan_depth: u32,
    content_depth: u32,
}

struct FootnoteDepthGuard {
    scans: FootnoteScans,
    parent_budget: Option<FootnoteScanBudget>,
}

impl Drop for FootnoteDepthGuard {
    fn drop(&mut self) {
        let mut state = self.scans.0.lock().unwrap();
        if let Some(parent_budget) = self.parent_budget.take() {
            state.budget = parent_budget;
            state.content_depth -= 1;
        } else {
            state.scan_depth -= 1;
        }
    }
}

/// Scan state for one inline source, shared by nested sessions.
#[derive(Debug, Clone)]
struct FootnoteScans(Arc<Mutex<FootnoteScanState>>);

impl FootnoteScans {
    // Bounds recursion even when callers raise `max_nesting`.
    const MAX_DEPTH: u32 = 64;
    const STEPS_PER_BYTE: usize = 32;

    fn new(source_len: usize) -> Self {
        Self(Arc::new(Mutex::new(FootnoteScanState {
            budget: FootnoteScanBudget::new(source_len),
            scan_depth: 0,
            content_depth: 0,
        })))
    }

    fn enter_content(&self, content_len: usize, max_nesting: u32) -> Option<FootnoteDepthGuard> {
        let mut state = self.0.lock().unwrap();
        if state.content_depth >= max_nesting.min(Self::MAX_DEPTH) {
            return None;
        }
        state.content_depth += 1;
        // Each accepted body gets a budget proportional to its own size. Sibling
        // bodies are disjoint, and the number of body levels is bounded.
        let parent_budget =
            std::mem::replace(&mut state.budget, FootnoteScanBudget::new(content_len));
        Some(FootnoteDepthGuard {
            scans: self.clone(),
            parent_budget: Some(parent_budget),
        })
    }

    fn memoized_end(
        &self,
        window: (usize, usize, u32, i32),
        max_nesting: u32,
        scan: impl FnOnce() -> Option<usize>,
    ) -> Option<usize> {
        let key = {
            let mut state = self.0.lock().unwrap();
            let available = max_nesting
                .min(Self::MAX_DEPTH)
                .saturating_sub(state.scan_depth + state.content_depth);
            let key = (window.0, window.1, window.2, window.3, available);
            if let Some(end) = state.budget.ends.get(&key) {
                return *end;
            }
            // Rejected checks count too: other rules may retry after the limit.
            state.budget.remaining_steps = state.budget.remaining_steps.checked_sub(1)?;
            if available == 0 {
                return None;
            }
            state.scan_depth += 1;
            key
        };
        let _depth = FootnoteDepthGuard {
            scans: self.clone(),
            parent_budget: None,
        };
        // Nested scans use the same cache, so release the lock before scanning.
        let end = scan();
        self.0.lock().unwrap().budget.ends.insert(key, end);
        end
    }

    fn step(&self) -> Option<()> {
        let mut state = self.0.lock().unwrap();
        state.budget.remaining_steps = state.budget.remaining_steps.checked_sub(1)?;
        Some(())
    }
}

/// Cross-block footnote state for the direct parser. Inline rules only get a
/// shared `&RootExtSet`, so numbering state sits behind a mutex; the lock is
/// never held while parsing children.
#[derive(Debug, Default)]
struct SharedFootnoteEnv(Mutex<FootnoteEnv>);

// --- block definition scanner ---

struct FootnoteDefinitionScanner;

impl FootnoteDefinitionScanner {
    fn scan_line(line: &str) -> Option<(String, usize)> {
        // "[^x]: something"
        // -^^
        if !line.starts_with("[^") {
            return None;
        }

        let label_start = 2;
        let label_end = line[label_start..].find(']')? + label_start;
        // "[^x]: something"
        // ----^
        if label_end == label_start {
            return None;
        }

        let after_label = label_end + 1;
        // "[^x]: something"
        // -----^
        if !line[after_label..].starts_with(':') {
            return None;
        }

        let normalized = normalize_reference(&line[label_start..label_end]);
        // "[^x]: something"
        // ---^  (if not have this)
        if normalized.is_empty() {
            return None;
        }

        let mut content_start = after_label + 1;
        // "[^x]: something"
        // ------^  (skip whitespace)
        while matches!(line.as_bytes().get(content_start), Some(b' ' | b'\t')) {
            content_start += 1;
        }

        Some((normalized, content_start))
    }

    fn definition_end(state: &BlockState<'_>) -> usize {
        let first_continuation = state.line + 1;
        let mut end_line = first_continuation;

        for line in first_continuation..state.line_max {
            if state.is_empty(line) {
                continue;
            }

            if state.line_indent(line) < FOOTNOTE_INDENT {
                break;
            }

            end_line = line + 1;
        }

        end_line
    }

    fn scan(state: &BlockState<'_>) -> Option<(String, usize)> {
        if state.line_indent(state.line) >= state.md.max_indent {
            return None;
        }
        let (normalized, content_start) = Self::scan_line(state.get_line(state.line))?;
        Some((
            normalized,
            state.line_offsets[state.line].first_nonspace + content_start,
        ))
    }
}

impl BlockRule for FootnoteDefinitionScanner {
    const MARKERS: &'static [char] = &['['];
    const NAMES: &'static [&'static str] = &["footnote_definition"];

    fn check(state: &mut BlockState<'_>) -> Option<()> {
        // Runs during paragraph-interruption tests, so it must stay pure.
        Self::scan(state).map(|_| ())
    }

    fn run(state: &mut BlockState<'_>) -> Option<(Option<NodeId>, usize)> {
        let (normalized, content_source_offset) = Self::scan(state)?;

        state
            .root_ext
            .get_or_insert_default::<SharedFootnoteEnv>()
            .0
            .lock()
            .unwrap()
            .define(normalized.clone());

        let start_line = state.line;
        let end_line = Self::definition_end(state);

        let old_node = std::mem::replace(
            &mut state.node,
            state
                .document
                .create_node(FootnoteDefinition::Named(normalized)),
        );
        let old_line_offset = state.line_offsets[start_line].clone();
        let old_blk_indent = state.blk_indent;
        let old_line_max = state.line_max;

        state.blk_indent += FOOTNOTE_INDENT as usize;
        state.line_offsets[start_line].first_nonspace = content_source_offset;
        state.line_offsets[start_line].content_start = content_source_offset;
        state.line_offsets[start_line].indent_nonspace = state.blk_indent as i32;
        state.line = start_line;
        state.line_max = end_line;

        state.tokenize_nested();
        let next_line = state.line;

        state.line = start_line;
        state.line_max = old_line_max;
        state.blk_indent = old_blk_indent;
        state.line_offsets[start_line] = old_line_offset;

        let node = std::mem::replace(&mut state.node, old_node);
        Some((Some(node), next_line - start_line))
    }
}

// --- named reference index ---

/// A trie of normalized labels in reverse byte order.
#[derive(Debug, Default)]
struct LabelSuffixes {
    edges: HashMap<(usize, u8), usize>,
    labels: HashSet<usize>,
}

impl LabelSuffixes {
    fn new(named: &HashMap<String, Option<usize>>) -> Self {
        let mut suffixes = Self::default();
        for label in named.keys() {
            let mut node = 0;
            for byte in label.bytes().rev() {
                let next = suffixes.edges.len() + 1;
                node = *suffixes.edges.entry((node, byte)).or_insert(next);
            }
            suffixes.labels.insert(node);
        }
        suffixes
    }

    fn match_segment(
        &self,
        segment: &str,
        start: usize,
        end: usize,
        matches: &mut HashMap<usize, usize>,
    ) {
        // Normalization preserves the order and spelling of all `[^` markers.
        let normalized = normalize_reference(segment);
        let mut cursor = normalized.len();
        let mut node = 0;
        for ((raw, _), (folded, _)) in segment
            .rmatch_indices(NAMED_FOOTNOTE_PREFIX)
            .zip(normalized.rmatch_indices(NAMED_FOOTNOTE_PREFIX))
        {
            let mut label_start = folded + NAMED_FOOTNOTE_PREFIX.len();
            // A space after the marker belongs to the label's leading whitespace.
            label_start += usize::from(normalized.as_bytes().get(label_start) == Some(&b' '));
            for &byte in normalized.as_bytes()[label_start..cursor].iter().rev() {
                let Some(&next) = self.edges.get(&(node, byte)) else {
                    return;
                };
                node = next;
            }
            cursor = label_start;
            if self.labels.contains(&node) {
                matches.insert(start + raw, end);
            }
        }
    }
}

/// Absolute source ranges of matching references, shared by all inline windows.
#[derive(Debug, Default)]
struct FootnoteReferenceEnds(HashMap<usize, usize>);

impl FootnoteReferenceEnds {
    fn new(source: &str, env: &FootnoteEnv) -> Self {
        let mut ends = Self::default();
        if env.named.is_empty() {
            return ends;
        }
        let mut segment_start = 0;
        for delimiter in memchr::memchr2_iter(b']', b'\n', source.as_bytes()) {
            let start = segment_start;
            let end = delimiter + 1;
            segment_start = end;
            if source.as_bytes()[delimiter] != b']' {
                continue;
            }
            let segment = &source[start..delimiter];
            let Some(first) = segment.find(NAMED_FOOTNOTE_PREFIX) else {
                continue;
            };
            let first_pos = start + first;
            let label = &segment[first + NAMED_FOOTNOTE_PREFIX.len()..];
            if label.contains(NAMED_FOOTNOTE_PREFIX) {
                env.label_suffixes
                    .get_or_init(|| LabelSuffixes::new(&env.named))
                    .match_segment(&segment[first..], first_pos, end, &mut ends.0);
            } else if env.named.contains_key(&normalize_reference(label)) {
                ends.0.insert(first_pos, end);
            }
        }
        ends
    }
}

// --- named reference scanner ---

pub(crate) struct FootnoteReferenceScanner;

impl FootnoteReferenceScanner {
    fn end(state: &InlineState<'_>, env: &SharedFootnoteEnv) -> Option<usize> {
        if !state.remaining().starts_with(NAMED_FOOTNOTE_PREFIX) {
            return None;
        }
        let mut ext = state.shared_ext();
        let ends = ext.get_or_insert_with(|| {
            FootnoteReferenceEnds::new(state.src.as_ref(), &env.0.lock().unwrap())
        });
        ends.0
            .get(&state.pos)
            .copied()
            .filter(|&end| end <= state.pos_max)
    }
}

impl InlineRule for FootnoteReferenceScanner {
    const MARKER: char = '[';
    const NAMES: &'static [&'static str] = &["footnote_reference"];

    fn run(state: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        let env = state.root_ext?.get::<SharedFootnoteEnv>()?;
        let end = Self::end(state, env)?;
        let start = state.pos + NAMED_FOOTNOTE_PREFIX.len();
        let normalized = normalize_reference(&state.src[start..end - 1]);
        let reference = env.0.lock().unwrap().reference_named(&normalized)?;
        Some((Some(state.document.create_node(reference)), end - state.pos))
    }

    fn check(context: &mut InlineState<'_>) -> Option<usize> {
        let env = context.root_ext?.get::<SharedFootnoteEnv>()?;
        Some(Self::end(context, env)? - context.pos)
    }
}

// --- inline footnote scanner ---

// ^[inline note]
pub(crate) struct FootnoteInlineScanner;

impl FootnoteInlineScanner {
    /// Find the matching `]` and return its offset from the content start.
    fn scan_end(context: &mut InlineState<'_>, scans: &FootnoteScans) -> Option<usize> {
        let start = context.pos;
        let mut level = 1usize;
        while let Some(ch) = context.remaining().chars().next() {
            scans.step()?;
            if ch == ']' {
                level -= 1;
                if level == 0 {
                    return Some(context.pos - start);
                }
            }

            let consumed = context.skip_token()?;
            // A bare `[` opens a nested bracket; tokenized brackets are opaque.
            if ch == '[' && consumed == 1 {
                level += 1;
            }
        }
        None
    }

    /// Return the content byte range relative to the current inline cursor.
    fn scan(state: &mut InlineState<'_>) -> Option<Range<usize>> {
        const PREFIX: &str = "^[";
        if !state.remaining().starts_with(PREFIX) {
            return None;
        }
        let content_start = PREFIX.len();
        let mut child = state.child_state_at(
            content_start..state.remaining().len(),
            state.depth,
            state.link_level,
        )?;
        let scans = child
            .shared_ext()
            .get_or_insert_with(|| FootnoteScans::new(child.src.len()))
            .clone();
        let end = scans.memoized_end(
            (child.pos, child.pos_max, child.depth, child.link_level),
            child.markdown_it().max_nesting,
            || Self::scan_end(&mut child, &scans),
        )?;
        (end > 0).then_some(content_start..content_start + end)
    }
}

impl InlineRule for FootnoteInlineScanner {
    const MARKER: char = '^';
    const NAMES: &'static [&'static str] = &["footnote_inline"];

    fn run(state: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        let content = Self::scan(state)?;
        let env = state.root_ext?.get::<SharedFootnoteEnv>()?;
        let scans = state.shared_ext().get::<FootnoteScans>()?.clone();
        let len = content.end + 1;

        // Allocate before parsing so nested footnotes get later numbers.
        let _content = scans.enter_content(content.len(), state.markdown_it().max_nesting)?;
        let index = env.0.lock().unwrap().reference_inline();

        #[cfg(feature = "sourcemap")]
        let srcmap = state.get_map(state.pos + content.start, state.pos + content.end);
        let children = state.parse_child_at(content, state.depth, state.link_level, |_| {})?;
        let paragraph = state
            .document
            .create_node(crate::plugins::cmark::block::paragraph::Paragraph);
        #[cfg(feature = "sourcemap")]
        state.document.node_mut(paragraph).set_srcmap(srcmap);
        state.document.attach_children(paragraph, children);

        let definition = state
            .document
            .create_node(FootnoteDefinition::Inline(index));
        state.document.push_child(definition, paragraph);

        let reference = state.document.create_node(FootnoteReference {
            number: index + 1,
            sub_id: 1,
        });
        state.document.push_child(reference, definition);
        Some((Some(reference), len))
    }

    fn check(context: &mut InlineState<'_>) -> Option<usize> {
        Self::scan(context).map(|content| content.end + 1)
    }
}

// --- finalize ---

#[derive(Default)]
struct FootnoteFinalizeRule;

impl FootnoteFinalizeRule {
    /// Collect definitions in postorder so nested inline footnotes remain discoverable.
    fn collect(document: &mut Document, env: &FootnoteEnv) -> Vec<Option<Vec<NodeId>>> {
        let definitions: Vec<_> = document
            .events(document.root())
            .filter_map(|event| match event {
                StructuralEvent::Exit(node) | StructuralEvent::Leaf(node)
                    if node.is::<FootnoteDefinition>() =>
                {
                    Some(node.id())
                }
                _ => None,
            })
            .collect();

        let mut collected = vec![None; env.ref_counts.len()];
        for id in definitions {
            let definition = document
                .node(id)
                .cast::<FootnoteDefinition>()
                .expect("definition ID stays valid");
            let index = match definition {
                FootnoteDefinition::Named(label) => env.named.get(label).copied().flatten(),
                FootnoteDefinition::Inline(index) => Some(*index),
            };
            if let Some(index) = index {
                let slot = &mut collected[index];
                if slot.is_none() {
                    *slot = Some(document.take_children(id));
                }
            }

            // Detach all sibling definitions at once. Each stays alive until its turn.
            if let Some(parent) = document.parent(id) {
                let mut children = document.take_children(parent);
                children.retain(|&child| !document.node(child).is::<FootnoteDefinition>());
                document.attach_children(parent, children);
            }
            document.discard_node(id);
        }

        collected
    }

    fn add_backrefs(document: &mut Document, item: NodeId, number: usize, refs: usize) {
        let container = document
            .children(item)
            .last()
            .copied()
            .filter(|&id| {
                document
                    .node(id)
                    .is::<crate::plugins::cmark::block::paragraph::Paragraph>()
            })
            .unwrap_or(item);
        for sub_id in 1..=refs {
            let reference = document.create_node(FootnoteBackref { number, sub_id });
            document.push_child(container, reference);
        }
    }

    /// Move referenced definitions using stable arena IDs, and delete unused content.
    fn finalize(document: &mut Document, root_ext: &RootExtSet) {
        let Some(env) = root_ext.get::<SharedFootnoteEnv>() else {
            return;
        };

        let env = env.0.lock().unwrap();

        let definitions = Self::collect(document, &env);
        let mut section = None;
        for (index, children) in definitions.into_iter().enumerate() {
            let Some(children) = children else { continue };
            let section = *section.get_or_insert_with(|| document.create_node(FootnoteSection));
            let number = index + 1;
            let item = document.create_node(FootnoteItem { number });
            document.attach_children(item, children);
            Self::add_backrefs(document, item, number, env.ref_counts[index]);
            document.push_child(section, item);
        }

        if let Some(section) = section {
            document.push_child(document.root(), section);
        }
    }
}

impl CoreRule for FootnoteFinalizeRule {
    const NAMES: &'static [&'static str] = &["footnote_tail"];

    fn run(&self, state: &mut CoreState<'_>) {
        state.with_root_ext(|document, ext| Self::finalize(document, ext));
    }
}

// --- preparation ---

/// Ensures [`SharedFootnoteEnv`] exists even for inline-only documents, where
/// the block rule never creates it.
#[derive(Default)]
struct FootnotePreparation;

impl CoreRule for FootnotePreparation {
    fn run(&self, state: &mut CoreState<'_>) {
        state
            .root_ext_mut()
            .get_or_insert_default::<SharedFootnoteEnv>();
    }
}

// --- renderers ---

struct FootnoteReferenceRenderer;

impl NodeRenderer<FootnoteReference> for FootnoteReferenceRenderer {
    fn render(
        &self,
        node: &Node,
        value: &FootnoteReference,
        _context: &mut RenderContext<'_>,
        output: &mut Writer,
    ) {
        let mut attrs = node.attrs().to_vec();
        attrs.push(("class".into(), "footnote-ref".into()));

        write_html_open(output, "sup", &attrs);
        write_html_open(
            output,
            "a",
            &[("href".into(), value.href()), ("id".into(), value.id())],
        );
        write_html_text(output, &format!("[{}]", value.number));
        write_html_close(output, "a");
        write_html_close(output, "sup");
    }
}

struct FootnoteSectionRenderer;

impl NodeRenderer<FootnoteSection> for FootnoteSectionRenderer {
    fn render(
        &self,
        node: &Node,
        _value: &FootnoteSection,
        context: &mut RenderContext<'_>,
        output: &mut Writer,
    ) {
        context.cr(output);
        write_html_self_close(
            output,
            "hr",
            &[("class".into(), "footnotes-sep".into())],
            context.options().xhtml_out,
        );
        context.cr(output);
        write_html_open(output, "section", &[("class".into(), "footnotes".into())]);
        context.cr(output);
        write_html_open(output, "ol", &[("class".into(), "footnotes-list".into())]);
        context.cr(output);
        context.render_children(node.id(), output);
        context.cr(output);
        write_html_close(output, "ol");
        context.cr(output);
        write_html_close(output, "section");
        context.cr(output);
    }
}

struct FootnoteItemRenderer;

impl NodeRenderer<FootnoteItem> for FootnoteItemRenderer {
    fn render(
        &self,
        node: &Node,
        value: &FootnoteItem,
        context: &mut RenderContext<'_>,
        output: &mut Writer,
    ) {
        let attrs = [
            ("id".into(), value.id()),
            ("class".into(), "footnote-item".into()),
        ];

        write_html_open(output, "li", &attrs);
        context.render_children(node.id(), output);
        write_html_close(output, "li");
        context.cr(output);
    }
}

struct FootnoteBackrefRenderer(FootnoteOptions);

impl NodeRenderer<FootnoteBackref> for FootnoteBackrefRenderer {
    fn render(
        &self,
        _node: &Node,
        value: &FootnoteBackref,
        _context: &mut RenderContext<'_>,
        output: &mut Writer,
    ) {
        write_html_text(output, " ");
        let href = ("href".into(), value.href());
        let class = ("class".into(), "footnote-backref".into());
        if let Some(label) = &self.0.backref_aria_label {
            write_html_open(
                output,
                "a",
                &[href, class, ("aria-label".into(), label.clone())],
            );
        } else {
            write_html_open(output, "a", &[href, class]);
        }
        if let Some(text) = &self.0.backref_text {
            write_html_text(output, text);
        } else {
            output.write_str("&#8617;");
        }
        write_html_close(output, "a");
    }
}

// --- pub method ---

/// HTML rendering options for links from footnotes back to their references.
#[derive(Clone, Debug, Default)]
pub struct FootnoteOptions {
    /// Visible text of each backreference, escaped as HTML text.
    /// The default is `↩`.
    pub backref_text: Option<String>,
    /// Accessible name of each backreference. By default, no `aria-label` is added.
    pub backref_aria_label: Option<String>,
}

pub fn add(md: &mut MarkdownIt) {
    add_with_options(md, FootnoteOptions::default());
}

/// Register footnotes with custom HTML backreference options.
pub fn add_with_options(md: &mut MarkdownIt, options: FootnoteOptions) {
    md.add_rule::<FootnotePreparation>()
        .before::<crate::parser::block::BlockParserRule>();
    md.block
        .add_rule::<FootnoteDefinitionScanner>()
        .before::<crate::plugins::cmark::block::reference::ReferenceScanner>();
    md.inline
        .add_rule::<FootnoteInlineScanner>()
        .before_named("link");
    md.inline
        .add_rule::<FootnoteReferenceScanner>()
        // [...] contains [^...]
        // let footnote rule try it first
        .before_named("link");
    md.add_rule::<FootnoteFinalizeRule>()
        .after::<crate::parser::inline::InlineParserRule>()
        .before_named("sourcepos");
    md.add_renderer::<FootnoteDefinition, _>("html", EmptyRenderer);
    md.add_renderer::<FootnoteDefinition, _>("text", EmptyRenderer);
    md.add_renderer::<FootnoteReference, _>("html", FootnoteReferenceRenderer);
    md.add_renderer::<FootnoteReference, _>("text", EmptyRenderer);
    md.add_renderer::<FootnoteSection, _>("html", FootnoteSectionRenderer);
    md.add_renderer::<FootnoteSection, _>("text", PlainTextBlockRenderer);
    md.add_renderer::<FootnoteItem, _>("html", FootnoteItemRenderer);
    md.add_renderer::<FootnoteItem, _>("text", PlainTextBlockRenderer);
    md.add_renderer::<FootnoteBackref, _>("html", FootnoteBackrefRenderer(options));
    md.add_renderer::<FootnoteBackref, _>("text", EmptyRenderer);
}

#[cfg(test)]
mod tests;
