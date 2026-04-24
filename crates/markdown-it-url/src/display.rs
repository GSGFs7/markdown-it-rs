use crate::authority::split_authority;
use crate::elide::fit_to_length;
use crate::host::display_host;
use crate::parse::{MailtoUrl, split_authority_suffix, split_mailto_url, strip_ascii_prefix};
use crate::percent::decode_for_display;

// provide a human read url
pub fn format_url_for_humans(input: &str, max_length: usize) -> String {
    fit_to_length(display_url(input), max_length)
}

// --- helper method ---

fn display_url(input: &str) -> String {
    if let Some(rest) = strip_ascii_prefix(input, "http://")
        .or_else(|| strip_ascii_prefix(input, "https://"))
        .or_else(|| input.strip_prefix("//"))
    {
        // delete protocol
        return display_authority_url(rest);
    }

    if let Some(mailto) = split_mailto_url(input) {
        return display_mailto_url(mailto);
    }

    decode_for_display(input)
}

fn display_authority_url(input: &str) -> String {
    // authority: domain, port, userinfo
    let (authority, suffix) = split_authority_suffix(input);
    let authority = display_authority(authority);

    if suffix == "/" {
        authority
    } else {
        authority + &decode_for_display(suffix)
    }
}

fn display_mailto_url(mailto: MailtoUrl<'_>) -> String {
    let Some((local, domain)) = mailto.address.rsplit_once('@') else {
        return decode_for_display(mailto.body);
    };

    decode_for_display(local) + "@" + &display_host(domain) + &decode_for_display(mailto.query)
}

fn display_authority(authority: &str) -> String {
    let parts = split_authority(authority);

    let mut result = String::with_capacity(authority.len());
    if let Some(userinfo) = parts.userinfo {
        result.push_str(&decode_for_display(userinfo));
        result.push('@');
    }

    result.push_str(&display_host(parts.host));
    result.push_str(parts.port);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_common_prefixes() {
        assert_eq!(
            format_url_for_humans("https://example.com/", usize::MAX),
            "example.com"
        );
        assert_eq!(
            format_url_for_humans("//example.com/a", usize::MAX),
            "example.com/a"
        );
        assert_eq!(
            format_url_for_humans("mailto:user@example.com", usize::MAX),
            "user@example.com"
        );
    }

    #[test]
    fn keeps_unknown_scheme() {
        assert_eq!(
            format_url_for_humans("JAVASCRIPT:alert(1)", usize::MAX),
            "JAVASCRIPT:alert(1)"
        );
    }

    #[test]
    fn decodes_safe_percent_encoded_text() {
        assert_eq!(
            format_url_for_humans(
                "https://example.com/%E3%82%AF%E3%83%AD?q=%E3%81%82%20b",
                usize::MAX
            ),
            "example.com/クロ?q=あ b"
        );
    }

    #[test]
    fn keeps_reserved_percent_encoded_text() {
        assert_eq!(
            format_url_for_humans("https://example.com/a%2Fb?q=hello%252Fhello", usize::MAX),
            "example.com/a%2Fb?q=hello%252Fhello"
        );
    }

    #[test]
    fn decodes_punycode_domains() {
        assert_eq!(
            format_url_for_humans("https://xn--pckwg.com/%E3%82%AF%E3%83%AD", usize::MAX),
            "クロ.com/クロ"
        );
    }

    #[test]
    fn elides_middle_of_long_path_and_query() {
        assert_eq!(
            format_url_for_humans(
                "https://img.gsgfs.moe/images/avif/c4/df/c4df021dd9203af39656a6b5c2779fe8276464ac52530b870eace19a00f23c05.avif?caillo=true",
                50
            ),
            "img.gsgfs.moe/images/avif/c4/df/c4df021dd9203af39…"
        );
    }

    #[test]
    fn handles_tiny_limits() {
        assert_eq!(format_url_for_humans("https://example.com/a", 0), "…");
        assert_eq!(format_url_for_humans("https://example.com/a", 1), "…");
    }
}
