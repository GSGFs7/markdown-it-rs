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

use crate::common::utils::normalize_reference;
use crate::parser::block::{BlockRule, BlockState};
use crate::parser::core::CoreRule;
use crate::parser::inline::{InlineRule, InlineState};
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

impl InlineRule for FootnoteReferenceScanner {
    const MARKER: char = '[';
    const NAMES: &'static [&'static str] = &["footnote_reference"];

    fn check(state: &mut InlineState) -> Option<usize> {
        scan_footnote_reference(state).map(|(_, _, len)| len)
    }

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let (_label, normalized, len) = scan_footnote_reference(state)?;

        let env = state.root_ext.get_mut::<FootnoteEnv>()?;

        let number = match env.numbers.get(&normalized).copied() {
            Some(number) => number,
            None => {
                let number = env.order.len() + 1;
                env.order.push(normalized.clone());
                env.numbers.insert(normalized.clone(), number);
                number
            }
        };

        let count = env.ref_counts.entry(normalized.clone()).or_insert(0);
        *count += 1;

        let node = Node::new(FootnoteReference {
            number,
            sub_id: *count,
        });

        Some((node, len))
    }
}

// ^[inline note]
struct FootnoteInlineScanner;

impl InlineRule for FootnoteInlineScanner {
    const MARKER: char = '^';
    const NAMES: &'static [&'static str] = &["footnote_inline"];

    fn check(state: &mut InlineState) -> Option<usize> {
        scan_inline_footnote(state).map(|(_, _, len)| len)
    }

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let (content_start, content_end, len) = scan_inline_footnote(state)?;

        let env = state.root_ext.get_or_insert_default::<FootnoteEnv>();
        let number = env.order.len() + 1;
        let normalized = format!("\0inline:{}", number);

        env.defined.insert(normalized.clone());
        env.order.push(normalized.clone());
        env.numbers.insert(normalized.clone(), number);
        env.ref_counts.insert(normalized.clone(), 1);

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
}

// --- helper method ---

fn scan_footnote_definition(state: &mut BlockState) -> Option<(String, String, usize)> {
    if state.line_indent(state.line) >= state.md.max_indent {
        return None;
    }

    let line = state.get_line(state.line);
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

    let content_source_offset = state.line_offsets[state.line].first_nonspace + content_start;

    Some((label, normalized, content_source_offset))
}

fn find_footnote_definition_end(state: &BlockState, start_line: usize) -> usize {
    let mut line = start_line + 1;
    let mut end_line = start_line + 1;

    while line < state.line_max {
        if state.is_empty(line) {
            line += 1;
            continue;
        }

        if state.line_indent(line) < FOOTNOTE_INDENT {
            break;
        }

        end_line = line + 1;
        line += 1;
    }

    end_line
}

fn scan_footnote_reference(state: &mut InlineState) -> Option<(String, String, usize)> {
    let rest = &state.src[state.pos..state.pos_max];
    // something[^x]
    // ---------^^
    if !rest.starts_with("[^") {
        return None;
    }

    let label_start = state.pos + 2;
    let mut label_end = None;
    // something[^x]
    // -----------^  (find this)
    for (offset, ch) in state.src[label_start..state.pos_max].char_indices() {
        match ch {
            // found the end
            ']' => {
                label_end = Some(label_start + offset);
                break;
            }
            // something[^x
            // ]
            //
            // the above method not allowed
            '\n' => return None,
            _ => {}
        }
    }

    let label_end = label_end?;
    // something[^x]
    // -----------^  (empty)
    if label_end == label_start {
        return None;
    }

    let label = state.src[label_start..label_end].to_owned();
    let normalized = normalize_reference(&label);
    // something[^x]
    // -----------^  (empty)
    if normalized.is_empty() {
        return None;
    }

    let env = state.root_ext.get::<FootnoteEnv>()?;
    if !env.defined.contains(&normalized) {
        return None;
    }

    Some((label, normalized, label_end + 1 - state.pos))
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
    // square brackets nest level
    let mut level = 1;
    let mut content_end = None;

    state.pos = content_start;

    // find ']'
    while let Some(ch) = state.src[state.pos..state.pos_max].chars().next() {
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

    state.pos = old_pos;

    let content_end = content_end?;
    if content_end == content_start {
        return None;
    }

    Some((content_start, content_end, content_end + 1 - start))
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

// --- pub method ---

pub fn add(md: &mut MarkdownIt) {
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
        .after::<crate::parser::inline::builtin::InlineParserRule>()
        .before_named("sourcepos");
}

#[cfg(test)]
mod tests {
    use crate as markdown_it;

    fn render(input: &str) -> String {
        let mut md = markdown_it::MarkdownIt::empty();
        markdown_it::plugins::cmark::add(&mut md);
        markdown_it::plugins::extra::footnote::add(&mut md);

        md.parse(input).render()
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
