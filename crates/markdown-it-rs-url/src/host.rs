use std::borrow::Cow;

// punycode, "クロ.com" -> "xn--pckwg.com"
pub(crate) fn normalize_host(host: &str) -> Option<Cow<'_, str>> {
    if host.is_empty() {
        return Some(Cow::Borrowed(""));
    }

    // protect IPv6, such as: [::1]
    if is_ipv6_host(host) {
        return Some(Cow::Borrowed(host));
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

    // markdown-it's `punycode.toASCII` is less strict than UTS #46. Keep
    // strict IDNA as the normal path, but encode non-ASCII labels directly as
    // a compatibility fallback (for example `xn--γ.com`).
    if host.is_ascii() {
        return None;
    }

    let mut result = String::with_capacity(host.len());
    for (index, label) in host.split('.').enumerate() {
        if index != 0 {
            result.push('.');
        }

        if label
            .bytes()
            .any(|byte| byte.is_ascii() && !byte.is_ascii_alphanumeric() && byte != b'-')
        {
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

pub(crate) fn display_host(host: &str) -> String {
    if is_ipv6_host(host) {
        return host.to_owned();
    }

    let mut result = String::with_capacity(host.len());
    for (index, label) in host.split('.').enumerate() {
        if index != 0 {
            result.push('.');
        }

        if label
            .get(..4)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("xn--"))
        {
            let (display, status) = idna::domain_to_unicode(label);
            result.push_str(if status.is_ok() { &display } else { label });
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
    use super::normalize_host;

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
