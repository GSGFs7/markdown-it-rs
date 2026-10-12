use std::borrow::Cow;

use crate::percent::encode_url;

// punycode, "クロ.com" -> "xn--pckwg.com"
pub(crate) fn normalize_host(host: &str) -> Option<Cow<'_, str>> {
    if host.is_empty() {
        return Some(Cow::Borrowed(""));
    }

    // Preserve outer IPv6 brackets; encode the hostname inside them.
    if is_ipv6_host(host) {
        // markdown-it compatibility: forbidden host characters disable bracket preservation.
        if host.contains(is_host_terminator) {
            return None;
        }
        return Some(match encode_url(&host[1..host.len() - 1]) {
            Cow::Borrowed(_) => Cow::Borrowed(host),
            Cow::Owned(inner) => Cow::Owned(format!("[{inner}]")),
        });
    }

    // Punycode conversion in markdown-it leaves ASCII labels unchanged,
    // including their case. Reject characters that belong in an encoded URL
    // suffix instead of accepting them as part of the hostname.
    if host.is_ascii() {
        return host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'%'))
            .then_some(Cow::Borrowed(host));
    }

    // Validate ASCII hostname characters before IDNA accepts URL punctuation.
    if host.bytes().any(|byte| {
        byte.is_ascii()
            && !byte.is_ascii_alphanumeric()
            && !matches!(byte, b'+' | b'-' | b'_' | b'.')
    }) {
        return None;
    }

    if let Ok(normalized) = idna::domain_to_ascii_cow(host.as_bytes(), idna::AsciiDenyList::URL) {
        let mut labels = normalized.split('.');
        let mut result = String::with_capacity(normalized.len());

        for (index, original) in host.split('.').enumerate() {
            let normalized = labels.next()?;
            if index != 0 {
                result.push('.');
            }
            result.push_str(if original.is_ascii() {
                original
            } else {
                normalized
            });
        }

        if labels.next().is_none() {
            return Some(Cow::Owned(result));
        }
    }

    // markdown-it compatibility: use raw Punycode when IDNA rejects a label.
    if host.is_ascii() {
        return None;
    }

    let mut result = String::with_capacity(host.len());
    for (index, label) in host.split('.').enumerate() {
        if index != 0 {
            result.push('.');
        }

        if label.bytes().any(|byte| {
            byte.is_ascii() && !byte.is_ascii_alphanumeric() && !matches!(byte, b'+' | b'-' | b'_')
        }) {
            return None;
        }

        if label.is_ascii() {
            result.push_str(label);
        } else {
            result.push_str("xn--");
            result.push_str(&idna::punycode::encode_str(label)?);
        }
    }

    Some(Cow::Owned(result))
}

pub(crate) fn is_host_terminator(ch: char) -> bool {
    matches!(
        ch,
        '%' | '/'
            | '?'
            | ';'
            | '#'
            | '\''
            | '{'
            | '}'
            | '|'
            | '\\'
            | '^'
            | '`'
            | '<'
            | '>'
            | '"'
            | ' '
            | '\r'
            | '\n'
            | '\t'
    )
}

pub(crate) fn display_host(host: &str) -> String {
    if is_ipv6_host(host) {
        return host.to_owned();
    }

    let mut result = String::with_capacity(host.len());
    for (index, label) in host.split('.').enumerate() {
        if index != 0 {
            result.push('.');
        }

        if let Some(encoded) = label.strip_prefix("xn--") {
            // markdown-it compatibility: decode raw Punycode without IDNA validation.
            let Some(display) = idna::punycode::decode_to_string(&encoded.to_ascii_lowercase())
            else {
                return host.to_owned();
            };
            result.push_str(&display);
        } else {
            result.push_str(label);
        }
    }

    result
}

fn is_ipv6_host(host: &str) -> bool {
    host.starts_with('[') && host.ends_with(']')
}

#[cfg(test)]
mod tests {
    use super::{display_host, normalize_host};

    #[test]
    fn display_uses_raw_punycode() {
        for (host, expected) in [
            ("xn--.com", ".com"),
            ("xn--a.com", "\u{80}.com"),
            ("xn--abc-.com", "abc.com"),
            ("xn--ABC-.com", "abc.com"),
            ("XN--ABC-.com", "XN--ABC-.com"),
            ("xn--n3h.net", "☃.net"),
            ("xn--xn.com", "xn--xn.com"),
            ("xn--n3h.xn--xn.com", "xn--n3h.xn--xn.com"),
            ("xn--.xn--xn.com", "xn--.xn--xn.com"),
        ] {
            assert_eq!(display_host(host), expected, "{host}");
        }
    }

    #[test]
    fn falls_back_to_raw_punycode_for_markdown_it_compatibility() {
        assert_eq!(
            normalize_host("xn--γ.com").as_deref(),
            Some("xn--xn---emd.com")
        );
    }

    #[test]
    fn preserves_ascii_label_case() {
        assert_eq!(normalize_host("FOO.Bar").as_deref(), Some("FOO.Bar"));
        assert_eq!(normalize_host("☃.Bar").as_deref(), Some("xn--n3h.Bar"));
    }

    #[test]
    fn rejects_characters_that_must_be_percent_encoded() {
        assert_eq!(normalize_host("foo.bar.`baz"), None);
    }
}
