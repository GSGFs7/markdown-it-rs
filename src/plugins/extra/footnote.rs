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
//! let html = md.parse(input).render();
//!
//! assert!(html.contains(r##"href="#fn1""##));
//! assert!(html.contains(r##"href="#fn2""##));
//! assert!(html.contains("first line\nsecond line"));
//! assert!(html.contains("<strong>note</strong>"));
//! ```

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crate::common::utils::normalize_reference;
use crate::document::{NodeDraft, NodeRef};
use crate::parser::block::{BlockRule, BlockState, DocumentBlockRule};
use crate::parser::core::{CoreRule, DocumentCoreRule};
use crate::parser::document_parser::{DocumentBlockState, DocumentInlineState};
use crate::parser::extset::RootExtSet;
use crate::parser::inline::{
    InlineProbeContext,
    InlineProbeKind,
    InlineProbeResult,
    InlineRule,
    InlineState,
    LegacyInlineRule,
};
use crate::render::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    DocumentWriter,
    EmptyDocumentRenderer,
    PlainTextBlockDocumentRenderer,
    write_html_close,
    write_html_open,
    write_html_self_close,
    write_html_text,
};
use crate::{MarkdownIt, Node, NodeValue, Renderer};

// [^1]: somthing
#[derive(Debug)]
struct FootnoteDefinition {
    normalized: String,
}

impl NodeValue for FootnoteDefinition {
    fn render(&self, _node: &Node, _fmt: &mut dyn Renderer) {
        // it empty
    }
}

// something[^1]
#[derive(Debug)]
struct FootnoteReference {
    number: usize,
    sub_id: usize,
}

impl NodeValue for FootnoteReference {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        let mut attrs = node.attrs.clone();
        attrs.push(("class".into(), "footnote-ref".into()));

        fmt.open("sup", &attrs);
        fmt.open(
            "a",
            &[
                (
                    "href".into(),
                    "#".to_string() + footnote_id(self.number).as_str(),
                ),
                ("id".into(), footnote_ref_id(self.number, self.sub_id)),
            ],
        );
        fmt.text(&format!("[{}]", self.number));
        fmt.close("a");
        fmt.close("sup");
    }
}

// rendered footnote definition at the bottom of HTML
#[derive(Debug)]
struct FootnoteSection;

impl NodeValue for FootnoteSection {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        fmt.cr();
        fmt.self_close("hr", &[("class".into(), "footnotes-sep".into())]);
        fmt.cr();
        fmt.open("section", &[("class".into(), "footnotes".into())]);
        fmt.cr();
        fmt.open("ol", &[("class".into(), "footnotes-list".into())]);
        fmt.cr();
        fmt.contents(&node.children);
        fmt.cr();
        fmt.close("ol");
        fmt.cr();
        fmt.close("section");
        fmt.cr();
    }
}

// rendered footnote reference
#[derive(Debug)]
struct FootnoteItem {
    number: usize,
}

impl NodeValue for FootnoteItem {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        let attrs = [
            ("id".into(), footnote_id(self.number)),
            ("class".into(), "footnote-item".into()),
        ];

        fmt.open("li", &attrs);
        fmt.contents(&node.children);
        fmt.close("li");
        fmt.cr();
    }
}

#[derive(Debug)]
struct FootnoteBackref {
    number: usize,
    sub_id: usize,
}

impl NodeValue for FootnoteBackref {
    fn render(&self, _node: &Node, fmt: &mut dyn Renderer) {
        fmt.text(" ");
        fmt.open(
            "a",
            &[
                (
                    "href".into(),
                    format!("#{}", footnote_ref_id(self.number, self.sub_id)),
                ),
                ("class".into(), "footnote-backref".into()),
            ],
        );
        // a arrow
        fmt.text_raw("&#8617;");
        fmt.close("a");
    }
}

// --- scanner ---

const FOOTNOTE_INDENT: i32 = 4;

struct FootnoteDefinitionScanner;

impl BlockRule for FootnoteDefinitionScanner {
    const MARKERS: &'static [char] = &['['];
    const NAMES: &'static [&'static str] = &["footnote_definition"];

    fn check(state: &mut BlockState) -> Option<()> {
        scan_footnote_definition(state).map(|_| ())
    }

    fn run(state: &mut BlockState) -> Option<(Node, usize)> {
        let (_label, normalized, content_source_offset) = scan_footnote_definition(state)?;

        let env = state.root_ext.get_or_insert_default::<FootnoteEnv>();
        env.defined.insert(normalized.clone());

        let start_line = state.line;
        let end_line = find_footnote_definition_end(state, start_line);
        let old_node = std::mem::replace(
            &mut state.node,
            Node::new(FootnoteDefinition { normalized }),
        );
        let old_line_offset = state.line_offsets[start_line].clone();
        let old_blk_indent = state.blk_indent;
        let old_line_max = state.line_max;

        state.blk_indent += FOOTNOTE_INDENT as usize;
        state.line_offsets[start_line].first_nonspace = content_source_offset;
        state.line_offsets[start_line].indent_nonspace = state.blk_indent as i32;
        state.line = start_line;
        state.line_max = end_line;

        state.md.block.tokenize_nested(state);
        let next_line = state.line;

        state.line = start_line;
        state.line_max = old_line_max;
        state.blk_indent = old_blk_indent;
        state.line_offsets[start_line] = old_line_offset;

        let node = std::mem::replace(&mut state.node, old_node);
        Some((node, next_line - start_line))
    }
}

struct FootnoteReferenceScanner;

impl LegacyInlineRule for FootnoteReferenceScanner {
    const MARKER: char = '[';
    const NAMES: &'static [&'static str] = &["footnote_reference"];

    fn check(state: &mut InlineState) -> Option<usize> {
        scan_footnote_reference(state).map(|(_, _, len)| len)
    }

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let (_label, normalized, len) = scan_footnote_reference(state)?;

        let env = state.root_ext.get_mut::<FootnoteEnv>()?;
        let (number, sub_id) = allocate_reference(env, &normalized);

        let node = Node::new(FootnoteReference { number, sub_id });

        Some((node, len))
    }
}

// ^[inline note]
struct FootnoteInlineScanner;

impl LegacyInlineRule for FootnoteInlineScanner {
    const MARKER: char = '^';
    const NAMES: &'static [&'static str] = &["footnote_inline"];

    fn check(state: &mut InlineState) -> Option<usize> {
        if !state.src[state.pos..state.pos_max].starts_with("^[") {
            return None;
        }
        let scanned = scan_inline_footnote(state).map(|(_, _, len)| len);
        scanned.or_else(|| {
            let scans = &state.root_ext.get::<FootnoteEnv>()?.scans;
            scans.exhausted().then_some(state.pos_max - state.pos)
        })
    }

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let scanned = scan_inline_footnote(state);
        if state.src[state.pos..state.pos_max].starts_with("^[")
            && state.root_ext.get::<FootnoteEnv>()?.scans.exhausted()
        {
            return Some((
                Node::new(crate::parser::inline::Text {
                    content: state.src[state.pos..state.pos_max].to_owned(),
                }),
                state.pos_max - state.pos,
            ));
        }
        let (content_start, content_end, len) = scanned?;

        let scans = state.root_ext.get::<FootnoteEnv>()?.scans.clone();
        let _content = scans.enter_content(state.md.max_nesting)?;

        let env = state.root_ext.get_or_insert_default::<FootnoteEnv>();
        let (normalized, number) = allocate_inline(env);

        let mut definition = Node::new(FootnoteDefinition { normalized });
        definition.children.push(parse_inline_footnote_content(
            state,
            content_start,
            content_end,
        ));

        let mut reference = Node::new(FootnoteReference { number, sub_id: 1 });
        reference.children.push(definition);

        Some((reference, len))
    }
}

struct FootnoteFinalizeRule;

impl CoreRule for FootnoteFinalizeRule {
    const NAMES: &'static [&'static str] = &["footnote_tail"];

    fn run(root: &mut Node, _md: &MarkdownIt) {
        let Some(env) = root
            .cast::<crate::parser::core::Root>()
            .unwrap()
            .ext
            .get::<FootnoteEnv>()
        else {
            // if not found, skip
            return;
        };

        let order = env.order.clone();
        let numbers = env.numbers.clone();
        let ref_counts = env.ref_counts.clone();
        env.scans.clear_cache();

        let mut definitions = HashMap::<String, Vec<Node>>::new();
        collect_footnote_definitions(&mut root.children, &mut definitions);
        // not any defs
        if order.is_empty() {
            return;
        }

        let mut section = Node::new(FootnoteSection);
        for normalized in order {
            let Some(children) = definitions.remove(&normalized) else {
                continue;
            };
            let Some(number) = numbers.get(&normalized).copied() else {
                continue;
            };
            let refs = ref_counts.get(&normalized).copied().unwrap_or(1);

            let mut item = Node::new(FootnoteItem { number });
            item.children = children;
            add_backrefs(&mut item, number, refs);
            section.children.push(item);
        }

        if !section.children.is_empty() {
            root.children.push(section);
        }
    }

    fn document_rule() -> Option<DocumentCoreRule> {
        Some(DocumentCoreRule::FinalizeDraft(footnote_draft_finalize))
    }
}

/// Ensures [`DocumentFootnoteEnv`] exists even for inline-only documents, where
/// the block rule never creates it.
struct FootnotePreparation;

impl CoreRule for FootnotePreparation {
    fn run(_root: &mut Node, _md: &MarkdownIt) {}

    fn document_rule() -> Option<DocumentCoreRule> {
        Some(DocumentCoreRule::PrepareState(|_source, _md, root_ext| {
            root_ext.get_or_insert_default::<DocumentFootnoteEnv>();
        }))
    }
}

// --- plugin state ---

#[derive(Debug, Default)]
struct FootnoteEnv {
    defined: HashSet<String>,
    /// the order of footnote item
    order: Vec<String>,
    /// O(1) index for order
    numbers: HashMap<String, usize>,
    ref_counts: HashMap<String, usize>,
    scans: FootnoteScans,
}

/// Shared scan state. Never locked across child parsing; owns the source
/// identity because inline buffers may be freed and reused.
#[derive(Debug, Clone, Default)]
struct FootnoteScans(Arc<Mutex<FootnoteScanState>>);

#[derive(Debug, Default)]
struct FootnoteScanState {
    source: String,
    ends: HashMap<(usize, usize, u32, i32, u32), Option<usize>>,
    scan_depth: u32,
    content_depth: u32,
    steps: usize,
    #[cfg(test)]
    misses: usize,
}

struct FootnoteDepthGuard {
    scans: FootnoteScans,
    content: bool,
}

struct FootnoteScanGuard {
    depth: FootnoteDepthGuard,
    key: (usize, usize, u32, i32, u32),
}

impl FootnoteScanGuard {
    fn finish(self, end: Option<usize>) -> Option<usize> {
        self.depth
            .scans
            .0
            .lock()
            .unwrap()
            .ends
            .insert(self.key, end);
        end
    }
}

impl Drop for FootnoteDepthGuard {
    fn drop(&mut self) {
        let mut state = self.scans.0.lock().unwrap();
        if self.content {
            state.content_depth -= 1;
        } else {
            state.scan_depth -= 1;
        }
    }
}

impl FootnoteScans {
    // Bounds recursion even when callers raise `max_nesting`.
    const MAX_DEPTH: u32 = 64;
    const STEPS_PER_BYTE: usize = 32;

    fn enter_content(&self, max_nesting: u32) -> Option<FootnoteDepthGuard> {
        let mut state = self.0.lock().unwrap();
        if state.content_depth >= max_nesting.min(Self::MAX_DEPTH) {
            return None;
        }
        state.content_depth += 1;
        Some(FootnoteDepthGuard {
            scans: self.clone(),
            content: true,
        })
    }

    fn begin(
        &self,
        source: &str,
        window: (usize, usize, u32, i32),
        max_nesting: u32,
    ) -> Result<Option<usize>, FootnoteScanGuard> {
        let key;
        {
            let mut state = self.0.lock().unwrap();
            if state.source != source {
                state.source = source.to_owned();
                state.ends.clear();
                state.steps = 0;
                #[cfg(test)]
                {
                    state.misses = 0;
                }
            }
            let available = max_nesting
                .min(Self::MAX_DEPTH)
                .saturating_sub(state.scan_depth + state.content_depth);
            key = (window.0, window.1, window.2, window.3, available);
            if let Some(end) = state.ends.get(&key) {
                return Ok(*end);
            }
            // Charge rejected probes too: other rules may retry after our limit.
            if state.steps >= source.len().saturating_mul(Self::STEPS_PER_BYTE) {
                return Ok(None);
            }
            state.steps += 1;
            if available == 0 {
                return Ok(None);
            }
            state.scan_depth += 1;
            #[cfg(test)]
            {
                state.misses += 1;
            }
        }
        Err(FootnoteScanGuard {
            depth: FootnoteDepthGuard {
                scans: self.clone(),
                content: false,
            },
            key,
        })
    }

    fn step(&self) -> Option<()> {
        let mut state = self.0.lock().unwrap();
        if state.steps >= state.source.len().saturating_mul(Self::STEPS_PER_BYTE) {
            return None;
        }
        state.steps += 1;
        Some(())
    }

    fn exhausted(&self) -> bool {
        let state = self.0.lock().unwrap();
        !state.source.is_empty()
            && state.steps >= state.source.len().saturating_mul(Self::STEPS_PER_BYTE)
    }

    fn clear_cache(&self) {
        let mut state = self.0.lock().unwrap();
        state.source = String::new();
        state.ends = HashMap::new();
    }
}

/// Cross-block footnote state for the direct parser. Inline rules only get a
/// shared `&RootExtSet`, so numbering state sits behind a mutex; the lock is
/// never held while parsing children.
#[derive(Debug, Default)]
struct DocumentFootnoteEnv(Mutex<FootnoteEnv>);

fn allocate_reference(env: &mut FootnoteEnv, normalized: &str) -> (usize, usize) {
    let number = match env.numbers.get(normalized).copied() {
        Some(number) => number,
        None => {
            let number = env.order.len() + 1;
            env.order.push(normalized.to_owned());
            env.numbers.insert(normalized.to_owned(), number);
            number
        }
    };

    let count = env.ref_counts.entry(normalized.to_owned()).or_insert(0);
    *count += 1;
    (number, *count)
}

fn allocate_inline(env: &mut FootnoteEnv) -> (String, usize) {
    let number = env.order.len() + 1;
    let normalized = format!("\0inline:{}", number);
    env.defined.insert(normalized.clone());
    env.order.push(normalized.clone());
    env.numbers.insert(normalized.clone(), number);
    env.ref_counts.insert(normalized.clone(), 1);
    // A new definition can change reference probing, so start a fresh scan
    // budget; repeated paragraphs then don't inherit earlier work.
    env.scans.clear_cache();
    (normalized, number)
}

// --- helper method ---

fn scan_footnote_definition(state: &mut BlockState) -> Option<(String, String, usize)> {
    if state.line_indent(state.line) >= state.md.max_indent {
        return None;
    }
    let (label, normalized, content_start) = scan_definition_line(state.get_line(state.line))?;
    Some((
        label,
        normalized,
        state.line_offsets[state.line].first_nonspace + content_start,
    ))
}

fn scan_definition_line(line: &str) -> Option<(String, String, usize)> {
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

    let label = line[label_start..label_end].to_owned();
    let normalized = normalize_reference(&label);
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

    Some((label, normalized, content_start))
}

fn find_footnote_definition_end(state: &BlockState, start_line: usize) -> usize {
    definition_end(
        start_line,
        state.line_max,
        |line| state.is_empty(line),
        |line| state.line_indent(line),
    )
}

fn definition_end(
    start_line: usize,
    line_max: usize,
    is_empty: impl Fn(usize) -> bool,
    line_indent: impl Fn(usize) -> i32,
) -> usize {
    let mut line = start_line + 1;
    let mut end_line = start_line + 1;

    while line < line_max {
        if is_empty(line) {
            line += 1;
            continue;
        }

        if line_indent(line) < FOOTNOTE_INDENT {
            break;
        }

        end_line = line + 1;
        line += 1;
    }

    end_line
}

fn scan_footnote_reference(state: &mut InlineState) -> Option<(String, String, usize)> {
    let env = state.root_ext.get::<FootnoteEnv>()?;
    parse_reference_label(&state.src[state.pos..state.pos_max], &env.defined)
}

fn parse_reference_label(rest: &str, defined: &HashSet<String>) -> Option<(String, String, usize)> {
    // something[^x]
    // ---------^^
    if !rest.starts_with("[^") {
        return None;
    }

    // something[^x]
    // -----------^  (find this)
    let end = rest[2..].find(['\n', ']'])? + 2;
    // something[^x
    // ]
    //
    // the above method not allowed
    if rest.as_bytes().get(end) != Some(&b']') {
        return None;
    }
    // something[^x]
    // -----------^  (empty)
    if end == 2 {
        return None;
    }

    let label = rest[2..end].to_owned();
    let normalized = normalize_reference(&label);
    // something[^x]
    // -----------^  (empty)
    if normalized.is_empty() {
        return None;
    }
    if !defined.contains(&normalized) {
        return None;
    }

    Some((label, normalized, end + 1))
}

fn scan_inline_footnote(state: &mut InlineState) -> Option<(usize, usize, usize)> {
    let rest = &state.src[state.pos..state.pos_max];
    // something^[note]
    // ---------^^
    if !rest.starts_with("^[") {
        return None;
    }

    let start = state.pos;
    let content_start = start + 2;
    let old_pos = state.pos;
    let scans = state
        .root_ext
        .get_or_insert_default::<FootnoteEnv>()
        .scans
        .clone();
    let window = (content_start, state.pos_max, state.level, state.link_level);
    let end_offset = match scans.begin(&state.src, window, state.md.max_nesting) {
        Ok(end) => end,
        Err(guard) => guard.finish(scan_inline_footnote_end(state, content_start, &scans)),
    };
    state.pos = old_pos;
    let content_end = content_start + end_offset?;
    if content_end == content_start {
        return None;
    }
    Some((content_start, content_end, content_end + 1 - start))
}

fn scan_inline_footnote_end(
    state: &mut InlineState,
    content_start: usize,
    scans: &FootnoteScans,
) -> Option<usize> {
    // square brackets nest level
    let mut level = 1;
    let mut content_end = None;

    state.pos = content_start;

    // find ']'
    while let Some(ch) = state.src[state.pos..state.pos_max].chars().next() {
        scans.step()?;
        if ch == ']' {
            level -= 1;
            if level == 0 {
                // all closed
                content_end = Some(state.pos);
                break;
            }
        }

        let prev_pos = state.pos;
        // skip entire token, such as "[text](url)"
        state.md.inline.skip_token(state);
        // if it's a normal '['
        if ch == '[' && prev_pos == state.pos - 1 {
            level += 1;
        }
    }

    let content_end = content_end?;
    Some(content_end - content_start)
}

fn parse_inline_footnote_content(
    state: &mut InlineState,
    content_start: usize,
    content_end: usize,
) -> Node {
    let mut paragraph = Node::new(crate::plugins::cmark::block::paragraph::Paragraph);
    paragraph.srcmap = state.get_map(content_start, content_end);

    let old_node = std::mem::replace(&mut state.node, paragraph);
    let old_pos = state.pos;
    let old_pos_max = state.pos_max;

    state.pos = content_start;
    state.pos_max = content_end;
    state.md.inline.tokenize(state);

    state.pos = old_pos;
    state.pos_max = old_pos_max;

    std::mem::replace(&mut state.node, old_node)
}

fn collect_footnote_definitions(
    nodes: &mut Vec<Node>,
    definitions: &mut HashMap<String, Vec<Node>>,
) {
    let mut idx = 0;
    while idx < nodes.len() {
        if let Some(definition) = nodes[idx].cast::<FootnoteDefinition>() {
            // if find defs
            let normalized = definition.normalized.clone();
            let mut node = nodes.remove(idx);
            collect_footnote_definitions(&mut node.children, definitions);
            definitions
                .entry(normalized)
                .or_insert_with(|| std::mem::take(&mut node.children));
        } else {
            collect_footnote_definitions(&mut nodes[idx].children, definitions);
            // find all children nodes
            idx += 1;
        }
    }
}

fn footnote_id(number: usize) -> String {
    format!("fn{}", number)
}

fn footnote_ref_id(number: usize, sub_id: usize) -> String {
    if sub_id == 1 {
        format!("fnref{}", number)
    } else {
        format!("fnref{}:{}", number, sub_id)
    }
}

fn add_backrefs(item: &mut Node, number: usize, refs: usize) {
    let last_child_is_paragraph = item
        .children
        .last()
        .is_some_and(|node| node.is::<crate::plugins::cmark::block::paragraph::Paragraph>());

    if last_child_is_paragraph {
        let container = &mut item.children.last_mut().unwrap().children;
        for sub_id in 1..=refs {
            container.push(Node::new(FootnoteBackref { number, sub_id }));
        }
    } else {
        for sub_id in 1..=refs {
            item.children
                .push(Node::new(FootnoteBackref { number, sub_id }));
        }
    }
}

// --- direct parser implementation ---

fn scan_document_definition(state: &DocumentBlockState<'_>) -> Option<(String, String, usize)> {
    if state.line_indent(state.line) >= state.md.max_indent {
        return None;
    }
    let (label, normalized, content_start) = scan_definition_line(state.get_line(state.line))?;
    Some((
        label,
        normalized,
        state.line_offsets[state.line].first_nonspace + content_start,
    ))
}

impl DocumentBlockRule for FootnoteDefinitionScanner {
    fn check(state: &mut DocumentBlockState<'_>) -> Option<()> {
        // Runs during paragraph-interruption tests, so it must stay pure.
        scan_document_definition(state).map(|_| ())
    }

    fn run(state: &mut DocumentBlockState<'_>) -> Option<(NodeDraft, usize)> {
        let (_label, normalized, content_source_offset) = scan_document_definition(state)?;

        state
            .root_ext
            .get_or_insert_default::<DocumentFootnoteEnv>()
            .0
            .lock()
            .unwrap()
            .defined
            .insert(normalized.clone());

        let start_line = state.line;
        let end_line = definition_end(
            start_line,
            state.line_max,
            |line| state.is_empty(line),
            |line| state.line_indent(line),
        );

        let old_node = std::mem::replace(
            &mut state.node,
            NodeDraft::new(FootnoteDefinition { normalized }),
        );
        let old_line_offset = state.line_offsets[start_line].clone();
        let old_blk_indent = state.blk_indent;
        let old_line_max = state.line_max;

        state.blk_indent += FOOTNOTE_INDENT as usize;
        state.line_offsets[start_line].first_nonspace = content_source_offset;
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
        Some((node, next_line - start_line))
    }
}

impl InlineRule for FootnoteReferenceScanner {
    const MARKER: char = '[';
    const NAMES: &'static [&'static str] = &["footnote_reference"];

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
        let env = state.root_ext?.get::<DocumentFootnoteEnv>()?;
        let mut env = env.0.lock().unwrap();
        let (_, normalized, len) = parse_reference_label(state.remaining(), &env.defined)?;
        let (number, sub_id) = allocate_reference(&mut env, &normalized);
        Some((
            Some(NodeDraft::new(FootnoteReference { number, sub_id })),
            len,
        ))
    }

    fn probe(context: &mut InlineProbeContext<'_>) -> InlineProbeResult {
        let Some(env) = context
            .root_ext()
            .and_then(|root| root.get::<DocumentFootnoteEnv>())
        else {
            return InlineProbeResult::NoMatch;
        };
        let env = env.0.lock().unwrap();
        match parse_reference_label(context.remaining(), &env.defined) {
            Some((_, _, len)) => InlineProbeResult::Match {
                len,
                kind: InlineProbeKind::Token,
            },
            None => InlineProbeResult::NoMatch,
        }
    }
}

/// Find the matching `]` for an inline footnote starting at the probe cursor;
/// returns the closing bracket's offset from the content start.
fn probe_inline_footnote_end(mut context: InlineProbeContext<'_>) -> Option<usize> {
    let scans = context
        .root_ext()?
        .get::<DocumentFootnoteEnv>()?
        .0
        .lock()
        .unwrap()
        .scans
        .clone();
    let (source, start, end) = context.source_window();
    match scans.begin(
        source,
        (start, end, context.depth(), context.link_level()),
        context.markdown_it().max_nesting,
    ) {
        Ok(end) => end,
        Err(guard) => guard.finish(scan_probed_footnote_end(&mut context, &scans)),
    }
}

fn scan_probed_footnote_end(
    context: &mut InlineProbeContext<'_>,
    scans: &FootnoteScans,
) -> Option<usize> {
    let initial_len = context.remaining().len();
    let mut level = 1usize;
    while let Some(ch) = context.remaining().chars().next() {
        scans.step()?;
        let before = context.remaining().len();
        if ch == ']' {
            level -= 1;
            if level == 0 {
                return Some(initial_len - before);
            }
        }

        context.next_token()?;

        let consumed = before - context.remaining().len();
        // A bare `[` opens a nested bracket; tokenized brackets are opaque.
        if ch == '[' && consumed == 1 {
            level += 1;
        }
    }
    None
}

impl InlineRule for FootnoteInlineScanner {
    const MARKER: char = '^';
    const NAMES: &'static [&'static str] = &["footnote_inline"];

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
        if !state.remaining().starts_with("^[") {
            return None;
        }
        let scanned = probe_inline_footnote_end(state.probe_from(2));
        let env = state.root_ext?.get::<DocumentFootnoteEnv>()?;
        let scans = env.0.lock().unwrap().scans.clone();
        if scans.exhausted() {
            return Some((
                Some(NodeDraft::new(crate::parser::inline::Text {
                    content: state.remaining().to_owned(),
                })),
                state.remaining().len(),
            ));
        }
        let end_offset = scanned?;
        if end_offset == 0 {
            return None;
        }
        let content_start = 2;
        let content_end = content_start + end_offset;
        let len = content_end + 1;

        // Allocate before parsing so nested footnotes get later numbers.
        let _content = scans.enter_content(state.markdown_it().max_nesting)?;
        let (normalized, number) = {
            let mut env = env.0.lock().unwrap();
            allocate_inline(&mut env)
        };

        let children = state.parse_subrange_at_current_depth(content_start..content_end)?;
        let mut paragraph = NodeDraft::new(crate::plugins::cmark::block::paragraph::Paragraph);
        paragraph.set_srcmap(state.get_map(state.pos + content_start, state.pos + content_end));
        paragraph.children_mut().extend(children);

        let mut definition = NodeDraft::new(FootnoteDefinition { normalized });
        definition.push_child(paragraph);

        let mut reference = NodeDraft::new(FootnoteReference { number, sub_id: 1 });
        reference.push_child(definition);
        Some((Some(reference), len))
    }

    fn probe(context: &mut InlineProbeContext<'_>) -> InlineProbeResult {
        if !context.remaining().starts_with("^[") {
            return InlineProbeResult::NoMatch;
        }
        let scanned = probe_inline_footnote_end(context.probe_from(2));
        let exhausted = context
            .root_ext()
            .and_then(|root| root.get::<DocumentFootnoteEnv>())
            .is_some_and(|env| env.0.lock().unwrap().scans.exhausted());
        if exhausted {
            return InlineProbeResult::Match {
                len: context.remaining().len(),
                kind: InlineProbeKind::Text,
            };
        }
        match scanned {
            Some(end_offset) if end_offset > 0 => InlineProbeResult::Match {
                len: end_offset + 3,
                kind: InlineProbeKind::Token,
            },
            _ => InlineProbeResult::NoMatch,
        }
    }
}

fn collect_footnote_definitions_draft(
    nodes: &mut Vec<NodeDraft>,
    definitions: &mut HashMap<String, Vec<NodeDraft>>,
) {
    let mut idx = 0;
    while idx < nodes.len() {
        let normalized = nodes[idx]
            .cast::<FootnoteDefinition>()
            .map(|definition| definition.normalized.clone());
        if let Some(normalized) = normalized {
            let mut node = nodes.remove(idx);
            collect_footnote_definitions_draft(node.children_mut(), definitions);
            definitions
                .entry(normalized)
                .or_insert_with(|| std::mem::take(node.children_mut()));
        } else {
            collect_footnote_definitions_draft(nodes[idx].children_mut(), definitions);
            idx += 1;
        }
    }
}

fn add_backrefs_draft(item: &mut NodeDraft, number: usize, refs: usize) {
    let last_child_is_paragraph = item
        .children()
        .last()
        .is_some_and(|node| node.is::<crate::plugins::cmark::block::paragraph::Paragraph>());

    if last_child_is_paragraph {
        let container = &mut item.children_mut().last_mut().unwrap().children_mut();
        for sub_id in 1..=refs {
            container.push(NodeDraft::new(FootnoteBackref { number, sub_id }));
        }
    } else {
        for sub_id in 1..=refs {
            item.push_child(NodeDraft::new(FootnoteBackref { number, sub_id }));
        }
    }
}

/// Move referenced definitions to the footnote section, discarding unused ones.
///
/// Runs on the [`NodeDraft`], so moving a definition is just moving a `Vec`
/// element — no arena surgery.
fn footnote_draft_finalize(root: &mut NodeDraft, root_ext: &RootExtSet) {
    let Some(env) = root_ext.get::<DocumentFootnoteEnv>() else {
        return;
    };
    let env = env.0.lock().unwrap();
    env.scans.clear_cache();

    // Remove every definition, even unused ones, matching the legacy finalizer.
    let mut definitions = HashMap::<String, Vec<NodeDraft>>::new();
    collect_footnote_definitions_draft(root.children_mut(), &mut definitions);
    if env.order.is_empty() || definitions.is_empty() {
        return;
    }

    let mut section = NodeDraft::new(FootnoteSection);
    for normalized in &env.order {
        let Some(children) = definitions.remove(normalized) else {
            continue;
        };
        let Some(number) = env.numbers.get(normalized).copied() else {
            continue;
        };
        let refs = env.ref_counts.get(normalized).copied().unwrap_or(1);

        let mut item = NodeDraft::new(FootnoteItem { number });
        item.children_mut().extend(children);
        add_backrefs_draft(&mut item, number, refs);
        section.push_child(item);
    }

    if !section.children().is_empty() {
        root.push_child(section);
    }
}

// --- document renderers ---

struct FootnoteReferenceDocumentRenderer;

impl DocumentNodeRenderer<FootnoteReference> for FootnoteReferenceDocumentRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        value: &FootnoteReference,
        _context: &mut DocumentRenderContext<'_>,
        output: &mut DocumentWriter,
    ) {
        let mut attrs = node.attrs().to_vec();
        attrs.push(("class".into(), "footnote-ref".into()));

        write_html_open(output, "sup", &attrs);
        write_html_open(
            output,
            "a",
            &[
                ("href".into(), format!("#{}", footnote_id(value.number))),
                ("id".into(), footnote_ref_id(value.number, value.sub_id)),
            ],
        );
        write_html_text(output, &format!("[{}]", value.number));
        write_html_close(output, "a");
        write_html_close(output, "sup");
    }
}

struct FootnoteSectionDocumentRenderer;

impl DocumentNodeRenderer<FootnoteSection> for FootnoteSectionDocumentRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        _value: &FootnoteSection,
        context: &mut DocumentRenderContext<'_>,
        output: &mut DocumentWriter,
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

struct FootnoteItemDocumentRenderer;

impl DocumentNodeRenderer<FootnoteItem> for FootnoteItemDocumentRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        value: &FootnoteItem,
        context: &mut DocumentRenderContext<'_>,
        output: &mut DocumentWriter,
    ) {
        let attrs = [
            ("id".into(), footnote_id(value.number)),
            ("class".into(), "footnote-item".into()),
        ];

        write_html_open(output, "li", &attrs);
        context.render_children(node.id(), output);
        write_html_close(output, "li");
        context.cr(output);
    }
}

struct FootnoteBackrefDocumentRenderer;

impl DocumentNodeRenderer<FootnoteBackref> for FootnoteBackrefDocumentRenderer {
    fn render(
        &self,
        _node: NodeRef<'_>,
        value: &FootnoteBackref,
        _context: &mut DocumentRenderContext<'_>,
        output: &mut DocumentWriter,
    ) {
        write_html_text(output, " ");
        write_html_open(
            output,
            "a",
            &[
                (
                    "href".into(),
                    format!("#{}", footnote_ref_id(value.number, value.sub_id)),
                ),
                ("class".into(), "footnote-backref".into()),
            ],
        );
        // ↩
        output.write_str("&#8617;");
        write_html_close(output, "a");
    }
}

// --- pub method ---

pub fn add(md: &mut MarkdownIt) {
    md.add_rule::<FootnotePreparation>()
        .before::<crate::parser::block::builtin::BlockParserRule>();
    md.block
        .add_rule::<FootnoteDefinitionScanner>()
        .before::<crate::plugins::cmark::block::reference::ReferenceScanner>();
    md.block.add_document_rule::<FootnoteDefinitionScanner>();
    md.inline
        .add_migrated_rule::<FootnoteInlineScanner>()
        .before_named("link");
    md.inline
        .add_migrated_rule::<FootnoteReferenceScanner>()
        // [...] contains [^...]
        // let footnote rule try it first
        .before_named("link");
    md.add_rule::<FootnoteFinalizeRule>()
        .after::<crate::parser::inline::builtin::InlineParserRule>()
        .before_named("sourcepos");
    md.add_document_renderer::<FootnoteDefinition, _>("html", EmptyDocumentRenderer);
    md.add_document_renderer::<FootnoteDefinition, _>("text", EmptyDocumentRenderer);
    md.add_document_renderer::<FootnoteReference, _>("html", FootnoteReferenceDocumentRenderer);
    md.add_document_renderer::<FootnoteReference, _>("text", EmptyDocumentRenderer);
    md.add_document_renderer::<FootnoteSection, _>("html", FootnoteSectionDocumentRenderer);
    md.add_document_renderer::<FootnoteSection, _>("text", PlainTextBlockDocumentRenderer);
    md.add_document_renderer::<FootnoteItem, _>("html", FootnoteItemDocumentRenderer);
    md.add_document_renderer::<FootnoteItem, _>("text", PlainTextBlockDocumentRenderer);
    md.add_document_renderer::<FootnoteBackref, _>("html", FootnoteBackrefDocumentRenderer);
    md.add_document_renderer::<FootnoteBackref, _>("text", EmptyDocumentRenderer);
}

#[cfg(test)]
mod tests {
    use super::{DocumentFootnoteEnv, FootnoteEnv, FootnoteScans};
    use crate as markdown_it;
    use crate::MarkdownIt;

    #[test]
    fn footnote_scan_cache_keeps_success_and_failure_and_context() {
        let scans = FootnoteScans::default();
        let source = "^[note]";
        let window = (2, source.len(), 0, 0);
        let guard = scans.begin(source, window, 100).err().unwrap();
        assert_eq!(guard.finish(Some(4)), Some(4));
        assert_eq!(scans.begin(source, window, 100).ok(), Some(Some(4)));
        let different_window = (2, source.len() - 1, 0, 0);
        scans
            .begin(source, different_window, 100)
            .err()
            .unwrap()
            .finish(None);
        assert_eq!(scans.begin(source, different_window, 100).ok(), Some(None));
        assert_eq!(scans.0.lock().unwrap().misses, 2);
        for changed in [(2, source.len(), 1, 0), (2, source.len(), 0, 1)] {
            scans
                .begin(source, changed, 100)
                .err()
                .unwrap()
                .finish(None);
        }
        assert!(scans.begin("^[xxxx]", window, 100).is_err());
    }

    #[test]
    fn malformed_footnote_scans_have_bounded_work_in_both_parsers() {
        for limit in [2, 100] {
            let mut md = MarkdownIt::new();
            markdown_it::plugins::extra::footnote::add(&mut md);
            md.max_nesting = limit;
            for n in [32, 128, 512] {
                let source = format!("{}x", "^[".repeat(n));
                let legacy = md.parse(&source);
                let direct = md.parse_document_direct(&source);
                assert_eq!(legacy.render(), format!("<p>{source}</p>\n"));
                assert_eq!(md.render_document(&direct), legacy.render());
                let legacy_scans = &legacy
                    .cast::<crate::parser::core::Root>()
                    .unwrap()
                    .ext
                    .get::<FootnoteEnv>()
                    .unwrap()
                    .scans;
                let root = direct.node(direct.root());
                let env = root
                    .cast::<crate::parser::core::Root>()
                    .unwrap()
                    .ext
                    .get::<DocumentFootnoteEnv>()
                    .unwrap()
                    .0
                    .lock()
                    .unwrap();
                for scans in [legacy_scans, &env.scans] {
                    let state = scans.0.lock().unwrap();
                    assert!(state.steps <= source.len() * FootnoteScans::STEPS_PER_BYTE);
                    assert!(state.misses <= state.steps + FootnoteScans::MAX_DEPTH as usize);
                    assert_eq!((state.scan_depth, state.content_depth), (0, 0));
                }
            }
        }
    }

    #[test]
    fn deep_closed_footnotes_respect_the_content_depth_limit() {
        let scans = FootnoteScans::default();
        let first = scans.enter_content(2).unwrap();
        let second = scans.enter_content(2).unwrap();
        assert!(scans.enter_content(2).is_none());
        drop(second);
        assert!(scans.enter_content(2).is_some());
        drop(first);
        for limit in [2, 100, 10_000] {
            let mut md = MarkdownIt::new();
            markdown_it::plugins::extra::footnote::add(&mut md);
            md.max_nesting = limit;
            let source = format!("{}x{}", "^[".repeat(256), "]".repeat(256));
            let direct = md.parse_document_direct(&source);
            let legacy = md.parse(&source);
            assert!(md.render_document(&direct).contains('x'));
            assert!(legacy.render().contains('x'));
        }
    }

    #[test]
    fn identical_paragraphs_do_not_share_an_exhausted_scan_budget() {
        let source = "text^[note]\n\n".repeat(100);
        let html = render(&source);
        assert_eq!(html.matches("class=\"footnote-ref\"").count(), 100);
        assert!(html.contains("id=\"fn100\""));
    }

    fn render(input: &str) -> String {
        let mut md = markdown_it::MarkdownIt::empty();
        markdown_it::plugins::cmark::add(&mut md);
        markdown_it::plugins::extra::footnote::add(&mut md);

        let html = md.parse(input).render();
        let direct = md.parse_document_direct(input);
        assert_eq!(
            md.render_document(&direct),
            html,
            "direct output for {input:?}"
        );
        html
    }

    #[test]
    fn basic_footnote() {
        let html = render("Here is a footnote.[^a]\n\n[^a]: Footnote text.");

        assert!(html.contains(r#"<sup class="footnote-ref">"#));
        assert!(html.contains(r##"href="#fn1""##));
        assert!(html.contains(r#"id="fn1""#));
        assert!(html.contains("Footnote text."));
        assert!(!html.contains("[^a]:"));
    }

    #[test]
    fn definition_can_appear_before_reference() {
        let html = render("[^a]: Footnote text.\n\nHere is a footnote.[^a]");

        assert!(html.contains(r##"href="#fn1""##));
        assert!(html.contains("Footnote text."));
    }

    #[test]
    fn undefined_reference_stays_text() {
        let html = render("Here is missing[^x].");

        assert_eq!(html, "<p>Here is missing[^x].</p>\n");
    }

    #[test]
    fn link_reference_still_works() {
        let html = render("[link]: /url\n\n[link]");

        assert_eq!(html, r#"<p><a href="/url">link</a></p>"#.to_owned() + "\n");
    }

    #[test]
    fn footnote_definition_is_not_link_reference() {
        let html = render("Text[^a]\n\n[^a]: Footnote text.");

        assert!(html.contains("Footnote text."));
        assert!(!html.contains(r#"href="Footnote""#));
    }

    #[test]
    fn numbers_follow_reference_order() {
        let html = render("A[^b] B[^a]\n\n[^a]: first\n[^b]: second");

        let ref_b = html.find(r##"href="#fn1""##).unwrap();
        let ref_a = html.find(r##"href="#fn2""##).unwrap();
        let second_defined = html.find("second").unwrap();
        let first_defined = html.find("first").unwrap();

        assert!(ref_b < ref_a);
        assert!(second_defined < first_defined);
    }

    #[test]
    fn repeated_reference_creates_one_footnote_item() {
        let html = render("A[^a] B[^a]\n\n[^a]: Footnote text.");

        assert_eq!(html.matches(r#"id="fn1""#).count(), 1);
        assert_eq!(html.matches(r##"href="#fn1""##).count(), 2);
        assert!(html.contains("Footnote text."));
    }

    #[test]
    fn footnote_definition_allows_indented_continuation_lines() {
        let html = render("Text[^a]\n\n[^a]: first line\n    second line\n\nAfter.");

        assert!(html.contains("first line\nsecond line"));
        assert!(html.contains("<p>After.</p>"));

        let paragraph = html.find("<p>After.</p>").unwrap();
        let footnotes = html.find(r#"<section class="footnotes">"#).unwrap();
        assert!(paragraph < footnotes);
    }

    #[test]
    fn footnote_definition_allows_tab_indented_continuation_lines() {
        let html = render("Text[^a]\n\n[^a]: first line\n\tsecond line\n\nAfter.");

        assert!(html.contains("first line\nsecond line"));
        assert!(html.contains("<p>After.</p>"));

        let paragraph = html.find("<p>After.</p>").unwrap();
        let footnotes = html.find(r#"<section class="footnotes">"#).unwrap();
        assert!(paragraph < footnotes);
    }

    #[test]
    fn footnote_definition_allows_tab_indented_multiple_paragraphs() {
        let html = render("Text[^a]\n\n[^a]: first paragraph\n\n\tsecond paragraph");

        assert!(html.contains("<p>first paragraph</p>"));
        assert!(html.contains(
            r##"<p>second paragraph <a href="#fnref1" class="footnote-backref">&#8617;</a></p>"##
        ));
        assert!(!html.contains("[^a]:"));
    }

    #[test]
    fn footnote_definition_allows_multiple_paragraphs() {
        let html = render("Text[^a]\n\n[^a]: first paragraph\n\n    second paragraph");

        assert!(html.contains("<p>first paragraph</p>"));
        assert!(html.contains(
            r##"<p>second paragraph <a href="#fnref1" class="footnote-backref">&#8617;</a></p>"##
        ));
        assert!(!html.contains("[^a]:"));
    }

    #[test]
    fn inline_footnote_creates_footnote_item() {
        let html = render("Text^[inline footnote].");

        assert!(html.contains(r##"href="#fn1""##));
        assert!(html.contains(r#"id="fn1""#));
        assert!(html.contains(
            r##"<p>inline footnote <a href="#fnref1" class="footnote-backref">&#8617;</a></p>"##
        ));
        assert!(!html.contains("^[inline footnote]"));
    }

    #[test]
    fn inline_footnote_parses_inline_markdown() {
        let html = render("Text^[inline **strong** note].");

        assert!(html.contains(r##"<p>inline <strong>strong</strong> note <a href="#fnref1" class="footnote-backref">&#8617;</a></p>"##));
    }

    #[test]
    fn inline_and_reference_footnotes_share_reference_order() {
        let html = render("A[^a] B^[inline]\n\n[^a]: named");

        let named_ref = html.find(r##"href="#fn1""##).unwrap();
        let inline_ref = html.find(r##"href="#fn2""##).unwrap();
        let named_definition = html.find("<p>named ").unwrap();
        let inline_definition = html.find("<p>inline ").unwrap();

        assert!(named_ref < inline_ref);
        assert!(named_definition < inline_definition);
    }

    #[test]
    fn inline_footnote_before_reference_gets_first_number() {
        let html = render("A^[inline] B[^a]\n\n[^a]: named");

        let inline_ref = html.find(r##"href="#fn1""##).unwrap();
        let named_ref = html.find(r##"href="#fn2""##).unwrap();
        let inline_definition = html.find("<p>inline ").unwrap();
        let named_definition = html.find("<p>named ").unwrap();

        assert!(inline_ref < named_ref);
        assert!(inline_definition < named_definition);
    }

    #[test]
    fn inline_footnote_allows_nested_brackets() {
        let html = render("Text^[literal [nested] brackets].");

        assert!(html.contains(r##"<p>literal [nested] brackets <a href="#fnref1" class="footnote-backref">&#8617;</a></p>"##));
        assert!(!html.contains("^[literal"));
    }

    #[test]
    fn inline_footnote_allows_escaped_closing_bracket() {
        let html = render(r"Text^[escaped \] bracket].");

        assert!(html.contains(
            r##"<p>escaped ] bracket <a href="#fnref1" class="footnote-backref">&#8617;</a></p>"##
        ));
        assert!(!html.contains(r"^[escaped \] bracket]"));
    }

    #[test]
    fn inline_footnote_allows_links_in_content() {
        let html = render("Text^[see [Rust](https://www.rust-lang.org/)].");

        assert!(html.contains(r##"<p>see <a href="https://www.rust-lang.org/">Rust</a> <a href="#fnref1" class="footnote-backref">&#8617;</a></p>"##));
    }

    #[test]
    fn footnote_definition_can_contain_inline_footnote() {
        let html = render("Text[^outer]\n\n[^outer]: outer^[inner]");

        assert!(html.contains(r##"href="#fn1""##));
        assert!(html.contains(r##"href="#fn2""##));

        let outer_definition = html.find("<p>outer").unwrap();
        let inner_definition = html.find("<p>inner ").unwrap();
        assert!(outer_definition < inner_definition);
    }

    #[test]
    fn single_paragraph_backref_renders_inside_paragraph() {
        let html = render("Text[^a]\n\n[^a]: see [Rust](https://www.rust-lang.org/)");

        assert!(html.contains(r##"<p>see <a href="https://www.rust-lang.org/">Rust</a> <a href="#fnref1" class="footnote-backref">&#8617;</a></p>"##));
        assert!(!html.contains(
            r##"</p>
 <a href="#fnref1" class="footnote-backref">"##
        ));
    }

    #[test]
    fn empty_inline_footnote_stays_text() {
        let html = render("Text^[]");

        assert_eq!(html, "<p>Text^[]</p>\n");
    }

    #[test]
    fn unclosed_inline_footnote_stays_text() {
        let html = render("Text^[missing");

        assert_eq!(html, "<p>Text^[missing</p>\n");
    }

    #[test]
    fn escaped_inline_footnote_marker_stays_text() {
        let html = render(r"Text\^[not footnote]");

        assert_eq!(html, "<p>Text^[not footnote]</p>\n");
    }
}
