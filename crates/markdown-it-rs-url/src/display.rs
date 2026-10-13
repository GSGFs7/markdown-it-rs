use crate::elide::fit_to_length;
use crate::host::display_host;
use crate::parse::ParsedUrl;
use crate::percent::decode_for_display;

pub fn decode_url_for_display(input: &str) -> String {
    let mut url = ParsedUrl::parse(input);
    if url.recodes_hostname()
        && let Some(host) = &mut url.hostname
    {
        *host = display_host(host).into();
    }
    // Decode once after formatting, including escapes in the recoded hostname.
    decode_for_display(&url.format())
}

/// Shorten a URL for display by removing common prefixes and a sole root slash.
pub fn format_url_for_humans(input: &str, max_length: usize) -> String {
    let mut url = ParsedUrl::parse(input);
    let common_prefix = url.slashes
        && url.protocol.is_none_or(|protocol| {
            protocol.eq_ignore_ascii_case("http:") || protocol.eq_ignore_ascii_case("https:")
        });
    let mailto = url
        .protocol
        .is_some_and(|protocol| protocol.eq_ignore_ascii_case("mailto:"));
    if common_prefix || mailto {
        if let Some(host) = &mut url.hostname {
            *host = display_host(host).into();
        }
        url.protocol = None;
        url.slashes = false;
        if common_prefix
            && url.pathname.as_deref() == Some("/")
            && url.search.is_none()
            && url.hash.is_none()
        {
            url.pathname = None;
        }
    }
    fit_to_length(decode_for_display(&url.format()), max_length)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recodes_display_hostnames_only_for_markdown_it_protocols() {
        for (prefix, host) in [
            ("http://", "☃.net"),
            ("https://", "☃.net"),
            ("//", "☃.net"),
            ("httttp://", "xn--n3h.net"),
            ("ftp://", "xn--n3h.net"),
            ("HTTP://", "xn--n3h.net"),
            ("mailto:user@", "☃.net"),
            ("MAILTO:user@", "xn--n3h.net"),
        ] {
            let source = format!("{prefix}xn--n3h.net?x=%CE%B1");
            assert_eq!(
                decode_url_for_display(&source),
                format!("{prefix}{host}?x=α")
            );
        }
    }

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
    fn display_decode_preserves_url_structure() {
        assert_eq!(
            decode_url_for_display("http://xn--n3h.net/"),
            "http://☃.net/"
        );
        assert_eq!(
            decode_url_for_display("//xn--n3h.net/%CE%B1%CE%B2"),
            "//☃.net/αβ"
        );
        assert_eq!(
            decode_url_for_display("mailto:user@xn--n3h.net"),
            "mailto:user@☃.net"
        );
    }

    #[test]
    fn display_decode_replaces_invalid_utf8_bytes() {
        for (source, expected) in [
            ("%Ccom", "�om"),
            ("%CC", "�"),
            ("%80%FF", "��"),
            ("%E2%82", "��"),
            ("%E2%28%A1", "�(�"),
            ("%C0%AF", "��"),
            ("%E0%80%AF", "���"),
            ("%ED%A0%80", "���"),
            ("%F4%90%80%80", "����"),
            ("%F0%80%80%AF", "����"),
            ("%F0%9F%98%80", "😀"),
            ("%CC%2f%41", "�%2FA"),
            ("%2E%252E", ".%252E"),
            ("%CC%CE%B1", "�α"),
            ("%GG%1%", "%GG%1%"),
        ] {
            assert_eq!(decode_url_for_display(source), expected, "{source}");
        }
    }

    #[test]
    fn display_decode_does_not_decode_percent_twice() {
        assert_eq!(
            decode_url_for_display("https://example.com/hello%2E%252Ehello"),
            "https://example.com/hello.%252Ehello"
        );
    }

    #[test]
    fn display_decode_matches_markdown_it_exclusion_set() {
        assert_eq!(
            decode_url_for_display(
                "http://cdecl.ridiculousfish.com/?q=int+%28*f%29+%28float+*%29%3B"
            ),
            "http://cdecl.ridiculousfish.com/?q=int+(*f)+(float+*)%3B"
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
