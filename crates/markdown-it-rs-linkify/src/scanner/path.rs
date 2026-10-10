use super::text::{Text, alnum};
use crate::unicode;

fn pair_end(
    text: &Text,
    start: usize,
    end: usize,
    depth: usize,
    open: u16,
    close: u16,
) -> Option<usize> {
    let mut p = start + 1;
    for _ in 0..1000 {
        if text.at(p, end) == Some(close) {
            return Some(p + 1);
        }

        let c = text.at(p, end)?;
        if text.is_zcc(p, end) {
            return None;
        }

        if c == open {
            if depth == 1 {
                return None;
            }
            p = pair_end(text, p, end, depth - 1, open, close)?;
        } else {
            p += 1;
        }
    }
    (text.at(p, end) == Some(close)).then_some(p + 1)
}

fn path_atom(text: &Text, p: usize, end: usize) -> Option<usize> {
    let c = text.at(p, end)?;
    if let Some(close) = match c {
        0x28 => Some(0x29),
        0x5b => Some(0x5d),
        0x7b => Some(0x7d),
        _ => None,
    } {
        return pair_end(text, p, end, 4, c, close);
    }

    if matches!(c, 0x22 | 0x27) {
        let mut last = p + 1;
        while last < end && last <= p + 100 && text.units[last] != c && !text.is_zcc(last, end) {
            last += 1;
        }

        if last > p + 1 && text.at(last, end) == Some(c) {
            return Some(last + 1);
        }

        if c == 0x27 && (text.pseudo_end(p + 1, end).is_some() || text.at(p + 1, end) == Some(0x2d))
        {
            return Some(p + 1);
        }

        return None;
    }
    if c == 0x2e {
        let mut last = p;
        while text.at(last, end) == Some(0x2e) && last < p + 20 {
            last += 1;
        }

        if last >= p + 2 {
            let tail = if text.at(last, end) == Some(0x3a) {
                last + 1
            } else {
                last
            };
            if text
                .at(tail, end)
                .is_some_and(|c| alnum(c) || c <= 127 && b"%/&".contains(&(c as u8)))
            {
                return Some(tail + 1);
            }
        }

        return (p + 1 < end && !text.is_zcc(p + 1, end) && text.units[p + 1] != c)
            .then_some(p + 1);
    }

    if c == 0x2d {
        let mut last = p + 1;
        while last < end && last < p + 20 && text.units[last] == c {
            last += 1;
        }
        return Some(last);
    }

    if matches!(c, 0x2c | 0x3b | 0x3f) {
        return (p + 1 < end && !text.is_zcc(p + 1, end) && (c != 0x3f || text.units[p + 1] != c))
            .then_some(p + 1);
    }

    if c == 0x21 {
        let mut last = p + 1;
        while last < end && last < p + 20 && text.units[last] == c {
            last += 1;
        }
        return (last < end && !text.is_zcc(last, end) && text.units[last] != c).then_some(last);
    }

    if c <= 127 && b"\\/:%@#&=_~*".contains(&(c as u8)) {
        return Some(p + 1);
    }

    let (cp, _) = text.cp(p, end)?;
    (!unicode::zpcc(cp) && !unicode::separator(cp)).then_some(p + 1)
}

pub(super) fn path_end(text: &Text, start: usize, end: usize) -> usize {
    if !text
        .at(start, end)
        .is_some_and(|c| matches!(c, 0x2f | 0x3f | 0x23))
    {
        return start;
    }

    let mut p = start + 1;
    for _ in 0..10000 {
        let Some(next) = path_atom(text, p, end) else {
            break;
        };
        p = next;
    }

    if p > start + 1 || text.units[start] == 0x2f {
        p
    } else {
        start
    }
}
