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

    if let Ok(host) = idna::domain_to_ascii_cow(host.as_bytes(), idna::AsciiDenyList::URL) {
        return Some(host);
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

    let (display, result) = idna::domain_to_unicode(host);
    if result.is_ok() {
        display
    } else {
        host.to_owned()
    }
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
}
