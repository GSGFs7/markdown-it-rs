use super::text::{Text, alnum, digit};

// The ordering mirrors the alternatives in get_domain/get_domain_root:
// punycode first, then one pseudo-letter, then a greedy multi-letter label.
fn label_ends(text: &Text, start: usize, end: usize, root: bool) -> Vec<usize> {
    let mut result = Vec::new();
    if text.ascii(start, b"xn--", end) {
        let mut p = start + 4;
        while p < end && p < start + 63 && (alnum(text.units[p]) || text.units[p] == b'-' as u16) {
            p += 1;
        }
        result.extend((start + 5..=p).rev());
    }

    let Some(first) = text.pseudo_end(start, end) else {
        return result;
    };

    let mut p = first;
    let mut ends = vec![first];
    for _ in 1..63 {
        if !root && text.at(p, end) == Some(b'-' as u16) {
            p += 1;
        } else if let Some(next) = text.pseudo_end(p, end) {
            p = next;
            ends.push(p);
        } else {
            break;
        }
    }

    if root {
        result.extend(ends.into_iter().rev());
    } else {
        result.push(first);
        result.extend(ends.into_iter().skip(1).rev());
    }

    result
}

#[derive(Clone, Copy)]
pub(super) enum Host {
    /// with protocol URL
    Explicit,
    /// start with "//"
    Relative,
    /// without protocol, needs a TLD
    Fuzzy,
    /// with "mailto:"
    Mail,
    /// bare email addr
    FuzzyMail,
}

fn ipv6_end(text: &Text, start: usize, end: usize, mail: bool) -> Option<usize> {
    let prefix = if mail {
        b"[IPv6:".as_slice()
    } else {
        b"[".as_slice()
    };

    if !text.ascii(start, prefix, end) {
        return None;
    }

    let first = start + prefix.len();
    let mut last = first;
    while last < end && (alnum(text.units[last]) || matches!(text.units[last], 0x3a | 0x2e)) {
        last += 1;
    }

    if text.at(last, end) != Some(b']' as u16) {
        return None;
    }

    let address: String = text.units[first..last]
        .iter()
        .map(|&c| c as u8 as char)
        .collect();
    address.parse::<std::net::Ipv6Addr>().ok().map(|_| last + 1)
}

fn port_end(text: &Text, pos: usize, end: usize) -> Option<usize> {
    if text.at(pos, end) == Some(b':' as u16) {
        let mut p = pos + 1;
        while p < end && p < pos + 6 && digit(text.units[p]) {
            p += 1;
        }

        for last in (pos + 2..=p).rev() {
            let mut value = 0u32;
            for &c in &text.units[pos + 1..last] {
                value = value * 10 + u32::from(c - b'0' as u16);
            }

            let count = last - pos - 1;
            if value <= 65535
                && (count < 5 || text.units[pos + 1] != b'0' as u16)
                && text.terminator(last, end)
            {
                return Some(last);
            }
        }
    }

    text.terminator(pos, end).then_some(pos)
}

fn finish_host(text: &Text, pos: usize, end: usize, host: Host) -> Option<usize> {
    if matches!(host, Host::Mail | Host::FuzzyMail | Host::Fuzzy) {
        text.terminator(pos, end).then_some(pos)
    } else {
        port_end(text, pos, end)
    }
}

pub(super) fn host_end(text: &Text, start: usize, end: usize, host: Host) -> Option<usize> {
    if !matches!(host, Host::Fuzzy)
        && let Some(last) = ipv6_end(
            text,
            start,
            end,
            matches!(host, Host::Mail | Host::FuzzyMail),
        )
        && let Some(last) = finish_host(text, last, end, host)
    {
        return Some(last);
    }

    if matches!(host, Host::Relative)
        && text.ascii(start, b"localhost", end)
        && let Some(last) = finish_host(text, start + 9, end, host)
    {
        return Some(last);
    }

    let max = if matches!(host, Host::Mail | Host::FuzzyMail) {
        4
    } else {
        10
    };
    let min = if matches!(host, Host::Relative | Host::Fuzzy | Host::FuzzyMail) {
        1
    } else {
        0
    };

    let mut starts = vec![start];
    for _ in 0..max {
        let Some(dot) = label_ends(text, *starts.last().unwrap(), end, false)
            .into_iter()
            .find(|&p| text.at(p, end) == Some(b'.' as u16))
        else {
            break;
        };
        starts.push(dot + 1);
    }

    for count in (min..starts.len()).rev() {
        let root = matches!(host, Host::Relative | Host::FuzzyMail);
        let ends = if matches!(host, Host::Fuzzy) {
            tld_ends(text, starts[count], end)
        } else {
            label_ends(text, starts[count], end, root)
        };
        for last in ends {
            if let Some(last) = finish_host(text, last, end, host) {
                return Some(last);
            }
        }
    }

    None
}

// Exact default set in linkify-it 6.1.0, rather than the changing IANA list.
const TLDS_2CH: &str = "a:cdefgilmnoqrstuwxz|b:abdefghijmnorstvwyz|c:acdfghiklmnoruvwxyz|d:ejkmoz|e:cegrstu|f:ijkmor|g:abdefghilmnpqrstuwy|h:kmnrtu|i:delmnoqrst|j:emop|k:eghimnprwyz|l:abcikrstuvy|m:acdeghklmnopqrstuvwxyz|n:acefgilopruz|o:m|p:aefghklmnrstwy|q:a|r:eosuw|s:abcdeghijklmnortuvxyz|t:cdfghjklmnortvwz|u:agksyz|v:aceginu|w:fs|y:et|z:amw";
const TLDS: &[&str] = &[
    "museum", "coop", "info", "name", "aero", "asia", "shop", "biz", "com", "edu", "gov", "net",
    "org", "pro", "web", "xxx",
];

fn tld_ends(text: &Text, pos: usize, end: usize) -> Vec<usize> {
    let mut result = Vec::new();
    for tld in TLDS {
        if text.ascii(pos, tld.as_bytes(), end) {
            result.push(pos + tld.len());
        }
    }

    if pos + 2 <= end {
        let a = text.units[pos];
        let b = text.units[pos + 1];
        if a <= 127 && b <= 127 {
            let a = (a as u8).to_ascii_lowercase();
            let b = (b as u8).to_ascii_lowercase();
            if TLDS_2CH
                .split('|')
                .any(|group| group.as_bytes()[0] == a && group.as_bytes()[2..].contains(&b))
            {
                result.push(pos + 2);
            }
        }
        if matches!(a, 0x440 | 0x420) && matches!(b, 0x444 | 0x424) {
            result.push(pos + 2);
        }
    }

    if text.ascii(pos, b"xn--", end) {
        let mut p = pos + 4;
        while p < end && p < pos + 63 && (alnum(text.units[p]) || text.units[p] == b'-' as u16) {
            p += 1;
        }
        result.extend((pos + 5..=p).rev());
    }
    result
}
