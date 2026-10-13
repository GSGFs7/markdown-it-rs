use std::borrow::Cow;

/// Raw Punycode, as used by punycode.js: no IDNA case folding, Unicode
/// normalization, or hostname validation. The URL parser owns validation.
pub(crate) fn normalize_host(host: &str) -> Option<Cow<'_, str>> {
    if host.is_ascii() {
        return Some(Cow::Borrowed(host));
    }
    let mapped = map_separators(host);
    let mut result = String::with_capacity(mapped.len());
    for (index, label) in mapped.split('.').enumerate() {
        if index != 0 {
            result.push('.');
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

pub(crate) fn display_host(host: &str) -> String {
    let mapped = map_separators(host);
    let mut result = String::with_capacity(mapped.len());
    for (index, label) in mapped.split('.').enumerate() {
        if index != 0 {
            result.push('.');
        }
        if let Some(encoded) = label.strip_prefix("xn--") {
            // punycode.js rejects non-ASCII input instead of interpreting it as
            // encoded digits. Any failed label leaves the entire host intact.
            if !encoded.is_ascii() {
                return host.to_owned();
            }
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

fn map_separators(host: &str) -> Cow<'_, str> {
    if host.contains(['\u{3002}', '\u{ff0e}', '\u{ff61}']) {
        Cow::Owned(host.replace(['\u{3002}', '\u{ff0e}', '\u{ff61}'], "."))
    } else {
        Cow::Borrowed(host)
    }
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
    fn encodes_with_raw_punycode() {
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
    fn preserves_unicode_without_idna_mapping() {
        for (host, expected) in [
            ("É.com", "xn--dca.com"),
            ("e\u{301}.com", "xn--e-xbb.com"),
            ("ＦＯＯ.com", "xn--uh7csaa.com"),
            ("☃。Bar", "xn--n3h.Bar"),
        ] {
            assert_eq!(normalize_host(host).as_deref(), Some(expected), "{host}");
        }
    }
}
