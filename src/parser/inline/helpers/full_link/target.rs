use super::config::InlineTargetFallback;
use crate::MarkdownIt;
use crate::common::utils::unescape_all;

#[derive(Debug)]
pub(super) struct InlineLinkTarget {
    pub(super) href: Option<String>,
    pub(super) title: Option<String>,
    pub(super) end: usize,
}

/// Result of trying the `(...)` suffix of a link candidate.
#[derive(Debug)]
pub(super) enum LinkTargetScan {
    /// A complete inline destination and optional title.
    Inline(InlineLinkTarget),
    /// No inline destination; continue looking for a reference at this offset.
    Reference { resume: usize },
    /// An inline destination was attempted but rejected without reference fallback.
    Reject,
}

pub struct ParseLinkFragmentResult {
    /// end position
    pub pos: usize,
    /// number of linebreaks inside
    pub lines: usize,
    /// parsed result
    pub str: String,
}

/// Helper function used to parse `<href>` part of the links with optional brackets.
pub fn parse_link_destination(
    str: &str,
    start: usize,
    max: usize,
) -> Option<ParseLinkFragmentResult> {
    let mut chars = str[start..max].chars().peekable();
    let mut pos = start;

    if let Some('<') = chars.peek() {
        chars.next(); // skip '<'
        pos += 1;
        loop {
            match chars.next() {
                Some('\n' | '<') | None => return None,
                Some('>') => {
                    return Some(ParseLinkFragmentResult {
                        pos: pos + 1,
                        lines: 0,
                        str: unescape_all(&str[start + 1..pos]).into_owned(),
                    });
                }
                Some('\\') => {
                    let x = chars.next()?;
                    pos += 1 + x.len_utf8();
                }
                Some(x) => {
                    pos += x.len_utf8();
                }
            }
        }
    } else {
        let mut level: u32 = 0;
        loop {
            match chars.next() {
                // space + ascii control characters
                Some('\0'..=' ' | '\x7f') | None => break,
                Some('\\') => match chars.next() {
                    Some(' ') | None => {
                        // [a](/url\ "title")
                        //          ^--- this space can't be escape in CommonMark
                        // it should be:
                        //  <a href="/url%5C" title="title">a</a>
                        //               ^^^--- backslash here
                        pos += 1;
                        break;
                    }
                    Some(x) => pos += 1 + x.len_utf8(),
                },
                Some('(') => {
                    level += 1;
                    if level > 32 {
                        return None;
                    }
                    pos += 1;
                }
                Some(')') => {
                    if level == 0 {
                        break;
                    }
                    level -= 1;
                    pos += 1;
                }
                Some(x) => {
                    pos += x.len_utf8();
                }
            }
        }

        if level != 0 {
            return None;
        }

        Some(ParseLinkFragmentResult {
            pos,
            lines: 0,
            str: unescape_all(&str[start..pos]).into_owned(),
        })
    }
}

/// Helper function used to parse `"title"` part of the links (with `'title'` or `(title)` alternative syntax).
pub fn parse_link_title(str: &str, start: usize, max: usize) -> Option<ParseLinkFragmentResult> {
    let mut chars = str[start..max].chars();
    let mut pos = start + 1;
    let mut lines = 0;

    let marker = match chars.next() {
        Some('"') => '"',
        Some('\'') => '\'',
        Some('(') => ')',
        None | Some(_) => return None,
    };

    loop {
        match chars.next() {
            Some(ch) if ch == marker => {
                return Some(ParseLinkFragmentResult {
                    pos: pos + 1,
                    lines,
                    str: unescape_all(&str[start + 1..pos]).into_owned(),
                });
            }
            Some('(') if marker == ')' => {
                return None;
            }
            Some('\n') => {
                pos += 1;
                lines += 1;
            }
            Some('\\') => {
                let x = chars.next()?;
                pos += 1 + x.len_utf8();
                if x == '\n' {
                    // [foo]: /url "
                    // hello
                    // \
                    // world
                    // "
                    //
                    // physical line break, must be recorded
                    // otherwise, some problems may occur during parsing
                    lines += 1;
                }
            }
            Some(x) => {
                pos += x.len_utf8();
            }
            None => {
                return None;
            }
        }
    }
}

// [link](  <href>  "title"  )
//        ^^ skipping these spaces
pub(super) fn skip_link_spaces(source: &str, mut pos: usize, max: usize) -> usize {
    while pos < max && matches!(source.as_bytes()[pos], b' ' | b'\t' | b'\n') {
        pos += 1;
    }
    pos
}

/// Try the suffix beginning at `start`, mirroring markdown-it's link/image
/// target fallback decision.
pub(super) fn scan_link_target(
    md: &MarkdownIt,
    source: &str,
    start: usize,
    max: usize,
    fallback: InlineTargetFallback,
) -> LinkTargetScan {
    if !source.get(start..max).is_some_and(|s| s.starts_with('(')) {
        return LinkTargetScan::Reference { resume: start };
    }

    // [link](  <href>  "title"  )
    //        ^^ skipping these spaces
    let target_start = skip_link_spaces(source, start + 1, max);
    if target_start >= max {
        // An opening parenthesis followed only by spaces cannot resolve a reference.
        return LinkTargetScan::Reject;
    }

    let (target, resume) = scan_inline_link_target(md, source, target_start, max);
    match target {
        Some(target) => LinkTargetScan::Inline(target),
        None if fallback == InlineTargetFallback::Reject => LinkTargetScan::Reject,
        None => LinkTargetScan::Reference { resume },
    }
}

/// Attempt an inline destination/title and return the cursor to probe for a
/// reference if the attempt fails.
pub(super) fn scan_inline_link_target(
    md: &MarkdownIt,
    source: &str,
    start: usize,
    max: usize,
) -> (Option<InlineLinkTarget>, usize) {
    let mut pos = start;
    if pos >= max {
        return (None, max);
    }

    let mut href = None;
    let mut title = None;

    // [link](  <href>  "title"  )
    //          ^^^^^^ parsing link destination
    if let Some(result) = parse_link_destination(source, pos, max) {
        let candidate = md.link_formatter.normalize_link(&result.str);
        if md.link_formatter.validate_link(&candidate).is_some() {
            pos = result.pos;
            href = Some(candidate);
        }

        // [link](  <href>  "title"  )
        //                ^^ skipping these spaces
        let title_separator_start = pos;
        pos = skip_link_spaces(source, pos, max);
        if pos > title_separator_start
            && let Some(result) = parse_link_title(source, pos, max)
        {
            title = Some(result.str);
            // [link](  <href>  "title"  )
            //                         ^^ skipping these spaces
            pos = skip_link_spaces(source, result.pos, max);
        }
    }

    let target = source[pos..max]
        .starts_with(')')
        .then_some(InlineLinkTarget {
            href,
            title,
            end: pos + 1,
        });
    // markdown-it advances past the failed delimiter before probing for a
    // reference at that cursor.
    let next = source[pos..max]
        .chars()
        .next()
        .map_or(pos + 1, |ch| pos + ch.len_utf8());
    (target, next)
}
