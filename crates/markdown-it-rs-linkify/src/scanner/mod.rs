//! Native implementation of the default linkify-it 6.1.0 detection grammar.
//! Positions inside the scanner are UTF-16 units: JavaScript's bounded regexes
//! count units, not UTF-8 bytes or Unicode scalar values. Only valid original
//! string boundaries are exported. No URL normalization takes place here.
use crate::{Link, LinkKind, unicode};

mod email;
mod host;
mod path;
mod text;

use email::mail_name_end;
use host::{Host, host_end};
use path::path_end;
use text::Text;

#[derive(Clone, Copy)]
struct Candidate {
    start: usize,
    end: usize,
    kind: LinkKind,
}

fn schema_boundary(text: &Text, start: usize) -> bool {
    if start == 0 {
        return true;
    }

    let prev = if start >= 2
        && (0xdc00..=0xdfff).contains(&text.units[start - 1])
        && (0xd800..=0xdbff).contains(&text.units[start - 2])
    {
        start - 2
    } else {
        start - 1
    };
    let (cp, _) = text.cp(prev, text.units.len()).unwrap();
    cp != 0x5f && (unicode::zpcc(cp) || unicode::separator(cp))
}

fn fuzzy_boundary(text: &Text, start: usize) -> bool {
    if start == 0 {
        return true;
    }

    let prev = if start >= 2
        && (0xdc00..=0xdfff).contains(&text.units[start - 1])
        && (0xd800..=0xdbff).contains(&text.units[start - 2])
    {
        start - 2
    } else {
        start - 1
    };
    let (cp, _) = text.cp(prev, text.units.len()).unwrap();
    !matches!(cp, 0x2e | 0x3a | 0x2f | 0x2d | 0x5f | 0x40)
        && (unicode::zpcc(cp)
            || matches!(
                cp,
                0x24 | 0x2b | 0x3c | 0x3d | 0x3e | 0x5e | 0x60 | 0x7c | 0xff5c
            ))
}

fn next_schema(text: &Text, cursor: &mut usize, pos: usize) -> Option<Candidate> {
    let end = text.units.len();
    *cursor = (*cursor).max(pos.saturating_sub(1));

    let mut search_from = *cursor;
    while *cursor < end {
        let start = *cursor;
        *cursor += 1;
        if !matches!(
            text.units[start],
            0x68 | 0x48 | 0x66 | 0x46 | 0x6d | 0x4d | 0x2f
        ) {
            continue;
        }

        // Global schema regex includes the preceding boundary in its match.
        if start < pos || start > 0 && start - 1 < search_from || !schema_boundary(text, start) {
            continue;
        }

        let Some((prefix, kind)) = [
            (b"https:".as_slice(), LinkKind::Url),
            (b"http:", LinkKind::Url),
            (b"ftp:", LinkKind::Url),
            (b"mailto:", LinkKind::Email),
            (b"//", LinkKind::Url),
        ]
        .into_iter()
        .find(|(prefix, _)| text.ascii(start, prefix, end)) else {
            continue;
        };

        let tail = start + prefix.len();
        *cursor = tail;
        search_from = tail;
        let limit = (tail + 10000).min(end);
        let last = if kind == LinkKind::Email {
            mail_name_end(text, tail, limit)
                .filter(|&p| text.at(p, limit) == Some(0x40))
                .and_then(|p| host_end(text, p + 1, limit, Host::Mail))
        } else if prefix == b"//" {
            if start > 0 && matches!(text.units[start - 1], 0x3a | 0x2f) {
                None
            } else {
                host_end(text, tail, limit, Host::Relative).map(|p| path_end(text, p, limit))
            }
        } else if text.ascii(tail, b"//", limit) {
            host_end(text, tail + 2, limit, Host::Explicit).map(|p| path_end(text, p, limit))
        } else {
            None
        };

        if let Some(last) = last {
            return Some(Candidate {
                start,
                end: last,
                kind,
            });
        }
    }

    None
}

fn next_fuzzy(text: &Text, cursor: &mut usize, pos: usize) -> Option<Candidate> {
    let end = text.units.len();
    *cursor = (*cursor).max(pos.saturating_sub(1));
    let search_from = *cursor;
    while *cursor < end {
        let start = *cursor;
        *cursor += 1;
        if start < pos
            || start > 0 && start - 1 < search_from
            || !fuzzy_boundary(text, start)
            || matches!(
                text.units[start],
                0x24 | 0x2b | 0x3c | 0x3d | 0x3e | 0x5e | 0x60 | 0x7c | 0xff5c
            )
        {
            continue;
        }

        if let Some(last) = host_end(text, start, end, Host::Fuzzy) {
            let last = path_end(text, last, end);
            *cursor = last;
            return Some(Candidate {
                start,
                end: last,
                kind: LinkKind::Url,
            });
        }
    }

    None
}

fn next_email(text: &Text, cursor: &mut usize, pos: usize) -> Option<Candidate> {
    let end = text.units.len();
    *cursor = (*cursor).max(pos.saturating_sub(1));
    while *cursor < end {
        let at = *cursor;
        *cursor += 1;

        if text.units[at] != 0x40 {
            continue;
        }

        let Some(last) = host_end(text, at + 1, end, Host::FuzzyMail) else {
            continue;
        };

        *cursor = last;

        let first = at.saturating_sub(65);
        for start in first..at {
            let boundary = start == first
                || text.cp(start - 1, end).is_some_and(|(cp, _)| {
                    unicode::separator(cp) || unicode::zcc(cp) || matches!(cp, 0x22 | 0x28)
                });
            if boundary && start >= pos && mail_name_end(text, start, at) == Some(at) {
                return Some(Candidate {
                    start,
                    end: last,
                    kind: LinkKind::Email,
                });
            }
        }
    }

    None
}

fn prefer(a: Option<Candidate>, b: Option<Candidate>) -> Option<Candidate> {
    match (a, b) {
        (Some(a), Some(b)) if b.start < a.start || b.start == a.start && b.end > a.end => Some(b),
        (Some(a), _) => Some(a),
        (_, b) => b,
    }
}

pub(super) fn scan(source: &str, fuzzy: bool) -> Vec<Link> {
    // Avoid a UTF-16 copy and full grammar scans for ordinary prose. Every
    // default non-fuzzy match necessarily contains one of these ASCII markers.
    if !fuzzy && !source.contains(':') && !source.contains('@') && !source.contains('/') {
        return Vec::new();
    }

    let text = Text::new(source);
    let mut result = Vec::new();
    let (mut schema_cursor, mut fuzzy_cursor, mut email_cursor) = (0, 0, 0);
    let (mut schema, mut url, mut email): (
        Option<Candidate>,
        Option<Candidate>,
        Option<Candidate>,
    ) = (None, None, None);

    let mut pos = 0;
    loop {
        if schema.is_none_or(|c| c.start < pos) {
            schema = next_schema(&text, &mut schema_cursor, pos);
        }
        if fuzzy && url.is_none_or(|c| c.start < pos) {
            url = next_fuzzy(&text, &mut fuzzy_cursor, pos);
        }
        if email.is_none_or(|c| c.start < pos) {
            email = next_email(&text, &mut email_cursor, pos);
        }

        let Some(candidate) = prefer(prefer(schema, email), url) else {
            break;
        };
        pos = candidate.end;

        let start = text.bytes[candidate.start];
        // JS may truncate a validator between surrogate halves at maxLength.
        // Rust strings cannot represent that raw text; retain the complete
        // scalar prefix rather than losing the whole URL or exporting bad bytes.
        let boundary = if text.bytes[candidate.end] == usize::MAX {
            candidate.end - 1
        } else {
            candidate.end
        };

        let end = text.bytes[boundary];
        if start != usize::MAX && end > start {
            result.push(Link {
                start,
                end,
                kind: candidate.kind,
            });
        }

        if schema.is_some_and(|c| {
            c.start == candidate.start && c.end == candidate.end && c.kind == candidate.kind
        }) {
            schema = None;
        }

        if url.is_some_and(|c| {
            c.start == candidate.start && c.end == candidate.end && c.kind == candidate.kind
        }) {
            url = None;
        }

        if email.is_some_and(|c| {
            c.start == candidate.start && c.end == candidate.end && c.kind == candidate.kind
        }) {
            email = None;
        }
    }
    result
}
