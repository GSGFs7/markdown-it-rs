//! Link reference definition
//!
//! `[label]: /url "title"`
//!
//! <https://spec.commonmark.org/0.30/#link-reference-definition>
//!
//! This plugin parses markdown link reference definitions and stores them in
//! the document-level [`ReferenceMap`]. Link rules read that map directly, so
//! external reference sources only need to insert a [`ReferenceMap`] into the
//! document root extensions.
//!
use crate::MarkdownIt;
use crate::common::utils::unescape_all;
use crate::document::NodeId;
use crate::links::prepare_link_destination;
use crate::parser::block::{BlockRule, BlockState};
use crate::parser::inline::helpers::full_link;
// Compatibility re-exports; canonical location is [`crate::reference`].
pub use crate::reference::{CustomReferenceMap, DefaultReferenceMap, ReferenceMap};
use crate::render::EmptyRenderer;

/// Add plugin that parses markdown link references
pub fn add(md: &mut MarkdownIt) {
    md.block.add_rule::<ReferenceScanner>();
    md.add_renderer::<Definition, _>("html", EmptyRenderer);
    md.add_renderer::<Definition, _>("text", EmptyRenderer);
}

#[derive(Debug)]
pub struct Definition {
    pub label: String,
    pub destination: String,
    pub title: Option<String>,
}

#[doc(hidden)]
pub struct ReferenceScanner;

impl BlockState<'_> {
    fn markdown_it(&self) -> &MarkdownIt {
        self.md
    }

    fn start_line(&self) -> Option<(usize, &str)> {
        if self.line_indent(self.line) >= self.md.max_indent {
            return None;
        }
        Some((self.line, self.get_line(self.line)))
    }

    fn continuation_line(&mut self, line: usize) -> Option<String> {
        if line >= self.line_max || self.is_empty(line) {
            return None;
        }
        let is_continuation = self.line_indent(line) >= self.md.max_indent
            || self.line_offsets[line].indent_nonspace < 0;
        if !is_continuation {
            let old_line = self.line;
            self.line = line;
            let terminated = self.test_rules_at_line();
            self.line = old_line;
            if terminated {
                return None;
            }
        }
        // Reference continuations start at the first nonspace character,
        // including inside multiline labels and titles.
        let mut content = self.get_line(line).to_owned();
        content.push('\n');
        Some(content)
    }

    fn references(&mut self) -> &mut ReferenceMap {
        self.root_ext.get_or_insert_default::<ReferenceMap>()
    }
}

impl BlockRule for ReferenceScanner {
    const MARKERS: &'static [char] = &['['];
    const NAMES: &'static [&'static str] = &["reference"];

    fn check(_: &mut BlockState<'_>) -> Option<()> {
        None // can't interrupt anything
    }

    fn run(state: &mut BlockState<'_>) -> Option<(Option<NodeId>, usize)> {
        let (definition, lines) = scan_reference(state)?;
        Some((Some(state.document.create_node(definition)), lines))
    }
}

fn scan_reference(state: &mut BlockState<'_>) -> Option<(Definition, usize)> {
    let (start_line, first_line) = state.start_line()?;
    let mut chars = first_line.chars();

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

    let mut next_line = start_line + 1;
    let mut str = first_line.to_owned();
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
                if pos == str.len() && !append_next_reference_line(state, &mut next_line, &mut str)
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
        href = prepare_link_destination(state.markdown_it().link_formatter.as_ref(), &res.str)?;
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
    } else {
        pos = dest_end_pos;
        next_line = dest_end_next_line;
    }

    // skip trailing spaces until the rest of the line
    loop {
        match str[pos..].chars().next() {
            Some(ch @ (' ' | '\t')) => pos += ch.len_utf8(),
            Some('\n') | None => break,
            Some(_) if title.as_ref().is_some_and(|title| !title.is_empty()) => {
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

    let references = state.references();
    if !references.insert(str[1..label_end].to_owned(), href.clone(), title.clone()) {
        return None;
    }

    Some((
        Definition {
            label: str[1..label_end].to_owned(),
            destination: href,
            title,
        },
        next_line - start_line,
    ))
}

fn append_next_reference_line(
    state: &mut BlockState<'_>,
    next_line: &mut usize,
    str: &mut String,
) -> bool {
    let Some(line) = state.continuation_line(*next_line) else {
        return false;
    };
    str.push_str(&line);
    *next_line += 1;
    true
}

fn skip_reference_whitespace(
    state: &mut BlockState<'_>,
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
    state: &mut BlockState<'_>,
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
