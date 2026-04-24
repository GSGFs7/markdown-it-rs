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

    idna::domain_to_ascii_cow(host.as_bytes(), idna::AsciiDenyList::URL).ok()
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
