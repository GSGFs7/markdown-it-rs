//! Link reference definition
//!
//! `[label]: /url "title"`
//!
//! <https://spec.commonmark.org/0.30/#link-reference-definition>
//!
//! This plugin parses markdown link references. Check documentation on [ReferenceMap]
//! to see how you can use and/or extend it if you have external source for references.
//!
use std::collections::HashMap;
use std::fmt::Debug;

use derive_more::{Deref, DerefMut};
use downcast_rs::{Downcast, impl_downcast};

use crate::common::utils::{normalize_reference, unescape_all};
use crate::generics::inline::full_link;
use crate::parser::block::{BlockRule, BlockState};
use crate::{MarkdownIt, Node, NodeValue};

/// Storage for parsed references
///
/// if you have some external source for your link references, you can add them like this:
///
/// ```rust
/// use markdown_it::parser::block::builtin::BlockParserRule;
/// use markdown_it::parser::core::{CoreRule, Root};
/// use markdown_it::plugins::cmark::block::reference::{ReferenceMap, DefaultReferenceMap, CustomReferenceMap};
/// use markdown_it::{MarkdownIt, Node};
///
/// let md = &mut MarkdownIt::empty();
/// markdown_it::plugins::cmark::add(md);
///
/// #[derive(Debug, Default)]
/// struct RefMapOverride(DefaultReferenceMap);
/// impl CustomReferenceMap for RefMapOverride {
///     fn get(&self, label: &str) -> Option<(&str, Option<&str>)> {
///         // override a specific link
///         if label == "rust" {
///             return Some((
///                 "https://www.rust-lang.org/",
///                 Some("The Rust Language"),
///             ));
///         }
///
///         self.0.get(label)
///     }
///
///     fn insert(&mut self, label: String, destination: String, title: Option<String>) -> bool {
///         self.0.insert(label, destination, title)
///     }
/// }
///
/// struct AddCustomReferences;
/// impl CoreRule for AddCustomReferences {
///     fn run(root: &mut Node, _: &MarkdownIt) {
///         let data = root.cast_mut::<Root>().unwrap();
///         data.ext.insert(ReferenceMap::new(RefMapOverride::default()));
///     }
/// }
///
/// md.add_rule::<AddCustomReferences>()
///     .before::<BlockParserRule>();
///
/// let html = md.parse("[rust]").render();
/// assert_eq!(
///     html.trim(),
///     r#"<p><a href="https://www.rust-lang.org/" title="The Rust Language">rust</a></p>"#
/// );
/// ```
///
/// You can also view all references that user created by adding the following rule:
///
/// ```rust
/// use markdown_it::parser::core::{CoreRule, Root};
/// use markdown_it::plugins::cmark::block::reference::{ReferenceMap, DefaultReferenceMap};
/// use markdown_it::{MarkdownIt, Node};
///
/// let md = &mut MarkdownIt::empty();
/// markdown_it::plugins::cmark::add(md);
///
/// let ast = md.parse("[hello]: world");
/// let root = ast.node_value.downcast_ref::<Root>().unwrap();
/// let refmap = root.ext.get::<ReferenceMap>()
///     .map(|m| m.downcast_ref::<DefaultReferenceMap>().expect("expect references to be handled by default map"));
///
/// let mut labels = vec![];
/// if let Some(refmap) = refmap {
///     for (label, _dest, _title) in refmap.iter() {
///         labels.push(label);
///     }
/// }
///
/// assert_eq!(labels, ["hello"]);
/// ```
///
#[derive(Debug, Deref, DerefMut)]
#[deref(forward)]
#[deref_mut(forward)]
pub struct ReferenceMap(Box<dyn CustomReferenceMap>);

impl ReferenceMap {
    pub fn new(custom_map: impl CustomReferenceMap + 'static) -> Self {
        Self(Box::new(custom_map))
    }
}

impl Default for ReferenceMap {
    fn default() -> Self {
        Self::new(DefaultReferenceMap::new())
    }
}

pub trait CustomReferenceMap: Debug + Downcast + Send + Sync {
    /// Insert new element to the reference map. You may return false if it's not a valid label to stop parsing.
    fn insert(&mut self, label: String, destination: String, title: Option<String>) -> bool;

    /// Get an element referenced by `label` from the map, returns destination and optional title.
    fn get(&self, label: &str) -> Option<(&str, Option<&str>)>;
}

impl_downcast!(CustomReferenceMap);

#[derive(Default, Debug)]
pub struct DefaultReferenceMap(HashMap<ReferenceMapKey, ReferenceMapEntry>);

impl DefaultReferenceMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str, Option<&str>)> {
        Box::new(
            self.0
                .iter()
                .map(|(a, b)| (a.label.as_str(), b.destination.as_str(), b.title.as_deref())),
        )
    }
}

impl CustomReferenceMap for DefaultReferenceMap {
    fn insert(&mut self, label: String, destination: String, title: Option<String>) -> bool {
        let Some(key) = ReferenceMapKey::new(label) else {
            return false;
        };
        self.0
            .entry(key)
            .or_insert(ReferenceMapEntry::new(destination, title));
        true
    }

    fn get(&self, label: &str) -> Option<(&str, Option<&str>)> {
        let key = ReferenceMapKey::new(label.to_owned())?;
        self.0
            .get(&key)
            .map(|r| (r.destination.as_str(), r.title.as_deref()))
    }
}

#[derive(Debug, Default)]
/// Reference label
struct ReferenceMapKey {
    pub label: String,
    normalized: String,
}

impl PartialEq for ReferenceMapKey {
    fn eq(&self, other: &Self) -> bool {
        self.normalized == other.normalized
    }
}

impl Eq for ReferenceMapKey {}

impl std::hash::Hash for ReferenceMapKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.normalized.hash(state);
    }
}

impl ReferenceMapKey {
    pub fn new(label: String) -> Option<Self> {
        let normalized = normalize_reference(&label);

        if normalized.is_empty() {
            // CommonMark 0.20 disallows empty labels
            return None;
        }

        Some(Self { label, normalized })
    }
}

#[derive(Debug, Default)]
/// Reference value
struct ReferenceMapEntry {
    pub destination: String,
    pub title: Option<String>,
}

impl ReferenceMapEntry {
    pub fn new(destination: String, title: Option<String>) -> Self {
        Self { destination, title }
    }
}

/// Add plugin that parses markdown link references
pub fn add(md: &mut MarkdownIt) {
    md.block.add_rule::<ReferenceScanner>();
}

#[derive(Debug)]
pub struct Definition {
    pub label: String,
    pub destination: String,
    pub title: Option<String>,
}
impl NodeValue for Definition {
    fn render(&self, _: &Node, _: &mut dyn crate::Renderer) {}
}

#[doc(hidden)]
pub struct ReferenceScanner;
impl BlockRule for ReferenceScanner {
    const NAMES: &'static [&'static str] = &["reference"];

    fn check(_: &mut BlockState) -> Option<()> {
        None // can't interrupt anything
    }

    fn run(state: &mut BlockState) -> Option<(Node, usize)> {
        if state.line_indent(state.line) >= state.md.max_indent {
            return None;
        }

        let mut chars = state.get_line(state.line).chars();

        let Some('[') = chars.next() else {
            return None;
        };

        // Simple check to quickly interrupt scan on [link](url) at the start of line.
        // Can be useful on practice: https://github.com/markdown-it/markdown-it/issues/54
        loop {
            match chars.next() {
                Some('\\') => {
                    chars.next();
                }
                Some(']') => {
                    if let Some(':') = chars.next() {
                        break;
                    } else {
                        return None;
                    }
                }
                Some(_) => {}
                None => break,
            }
        }

        let start_line = state.line;
        let mut next_line = start_line + 1;
        let mut str = state.get_line(start_line).to_owned();
        str.push('\n');

        let mut pos = 1; // skip '['
        let label_end;

        loop {
            let ch = str[pos..].chars().next()?;
            match ch {
                '[' => return None,
                ']' => {
                    label_end = pos;
                    pos += 1;
                    break;
                }
                '\n' => {
                    pos += 1;
                    if pos == str.len()
                        && !append_next_reference_line(state, &mut next_line, &mut str)
                    {
                        return None;
                    }
                }
                '\\' => {
                    pos += 1;
                    let escaped = str[pos..].chars().next()?;
                    pos += escaped.len_utf8();
                    if escaped == '\n'
                        && pos == str.len()
                        && !append_next_reference_line(state, &mut next_line, &mut str)
                    {
                        return None;
                    }
                }
                _ => pos += ch.len_utf8(),
            }
        }

        let Some(':') = str[pos..].chars().next() else {
            return None;
        };
        pos += 1;

        // [label]:   destination   'title'
        //         ^^^ skip optional whitespace here
        skip_reference_whitespace(state, &mut next_line, &mut str, &mut pos);

        // [label]:   destination   'title'
        //            ^^^^^^^^^^^ parse this
        let href;
        {
            let res = full_link::parse_link_destination(&str, pos, str.len())?;
            if pos == res.pos {
                return None;
            }
            href = state.md.link_formatter.normalize_link(&res.str);
            state.md.link_formatter.validate_link(&href)?;
            pos = res.pos;
        }

        // save cursor state, we could require to rollback later
        let dest_end_pos = pos;
        let dest_end_next_line = next_line;

        // [label]:   destination   'title'
        //                       ^^^ skipping those spaces
        let start = pos;
        skip_reference_whitespace(state, &mut next_line, &mut str, &mut pos);

        // [label]:   destination   'title'
        //                          ^^^^^^^ parse this
        let mut title = None;
        if pos != start {
            if let Some(res) = parse_reference_title(state, &mut next_line, &mut str, pos) {
                title = Some(res.str);
                pos = res.pos;
            } else {
                pos = dest_end_pos;
                next_line = dest_end_next_line;
            }
        }

        // skip trailing spaces until the rest of the line
        loop {
            match str[pos..].chars().next() {
                Some(ch @ (' ' | '\t')) => pos += ch.len_utf8(),
                Some('\n') | None => break,
                Some(_) if title.is_some() => {
                    // garbage at the end of the line after title,
                    // but it could still be a valid reference if we roll back
                    title = None;
                    pos = dest_end_pos;
                    next_line = dest_end_next_line;
                }
                Some(_) => {
                    // garbage at the end of the line
                    return None;
                }
            }
        }

        let references = state.root_ext.get_or_insert_default::<ReferenceMap>();
        if !references.insert(str[1..label_end].to_owned(), href.clone(), title.clone()) {
            return None;
        }

        Some((
            Node::new(Definition {
                label: str[1..label_end].to_owned(),
                destination: href,
                title,
            }),
            next_line - start_line,
        ))
    }
}

fn append_next_reference_line(
    state: &mut BlockState,
    next_line: &mut usize,
    str: &mut String,
) -> bool {
    if *next_line >= state.line_max || state.is_empty(*next_line) {
        return false;
    }

    let is_continuation = state.line_indent(*next_line) >= state.md.max_indent
        || state.line_offsets[*next_line].indent_nonspace < 0;

    if !is_continuation {
        let old_state_line = state.line;
        state.line = *next_line;
        let terminated = state.test_rules_at_line();
        state.line = old_state_line;
        if terminated {
            return false;
        }
    }

    let (line, _) = state.get_lines(*next_line, *next_line + 1, state.blk_indent, true);
    str.push_str(&line);
    *next_line += 1;
    true
}

fn skip_reference_whitespace(
    state: &mut BlockState,
    next_line: &mut usize,
    str: &mut String,
    pos: &mut usize,
) {
    while let Some(ch) = str[*pos..].chars().next() {
        match ch {
            ' ' | '\t' => *pos += ch.len_utf8(),
            '\n' => {
                *pos += 1;
                if *pos == str.len() && !append_next_reference_line(state, next_line, str) {
                    break;
                }
            }
            _ => break,
        }
    }
}

fn parse_reference_title(
    state: &mut BlockState,
    next_line: &mut usize,
    str: &mut String,
    start: usize,
) -> Option<full_link::ParseLinkFragmentResult> {
    let marker = match str[start..].chars().next() {
        Some('"') => '"',
        Some('\'') => '\'',
        Some('(') => ')',
        None | Some(_) => return None,
    };

    let mut pos = start + 1;
    let mut lines = 0;

    loop {
        let ch = str[pos..].chars().next()?;
        if ch == marker {
            return Some(full_link::ParseLinkFragmentResult {
                pos: pos + ch.len_utf8(),
                lines,
                str: unescape_all(&str[start + 1..pos]).into_owned(),
            });
        }

        match ch {
            '(' if marker == ')' => return None,
            '\n' => {
                pos += 1;
                lines += 1;
                if pos == str.len() && !append_next_reference_line(state, next_line, str) {
                    return None;
                }
            }
            '\\' => {
                pos += 1;
                let escaped = str[pos..].chars().next()?;
                pos += escaped.len_utf8();
                if escaped == '\n' {
                    lines += 1;
                    if pos == str.len() && !append_next_reference_line(state, next_line, str) {
                        return None;
                    }
                }
            }
            _ => pos += ch.len_utf8(),
        }
    }
}
