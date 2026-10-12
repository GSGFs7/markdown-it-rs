use crate::authority::split_authority;
use crate::host::{is_host_terminator, normalize_host};
use crate::parse::{split_authority_url, split_mailto_url};
use crate::percent::encode_url;

pub fn format_url_for_computers(input: &str) -> String {
    // mdurl parses URLs after JavaScript String.trim(). Keep interior whitespace
    // for percent encoding; Rust's whitespace set differs for NEL and BOM.
    let input = input.trim_matches(|ch| {
        matches!(
            ch,
            '\u{0009}'..='\u{000d}'
                | ' '
                | '\u{00a0}'
                | '\u{1680}'
                | '\u{2000}'..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
        )
    });
    if let Some(url) = split_authority_url(input) {
        return normalize_authority_url(url.prefix, url.authority, url.suffix);
    }

    if split_mailto_url(input).is_some() {
        return normalize_mailto_url(input);
    }

    encode_url(input).into_owned()
}

// --- helper method ---

// authority, such as: postgresql://db_user:db_passwd@localhost:5432/db?searchpath=schema
fn normalize_authority_url(prefix: &str, authority: &str, suffix: &str) -> String {
    // markdown-it compatibility: forbidden host characters begin the URL suffix.
    let host_start = authority.rfind('@').map_or(0, |pos| pos + 1);
    let host_end = authority[host_start..]
        .find(is_host_terminator)
        .map_or(authority.len(), |pos| host_start + pos);
    let (host_authority, host_suffix) = authority.split_at(host_end);
    let parts = split_authority(host_authority);

    // encode each part
    let Some(host) = normalize_host(parts.host) else {
        let mut result = encode_url(prefix).into_owned();
        if parts.host.starts_with('[') && !parts.host.ends_with(']') && !parts.port.is_empty() {
            // markdown-it compatibility: place the port before an unclosed bracketed host.
            if let Some(userinfo) = parts.userinfo {
                result.push_str(encode_url(userinfo).as_ref());
                result.push('@');
            }
            result.push_str(parts.port);
            result.push_str(encode_url(parts.host).as_ref());
            result.push_str(encode_url(host_suffix).as_ref());
        } else {
            result.push_str(encode_url(authority).as_ref());
        }
        result.push_str(encode_url(suffix).as_ref());
        return result;
    };

    // splicing
    let mut result = String::with_capacity(prefix.len() + authority.len() + suffix.len());
    result.push_str(prefix);

    if let Some(userinfo) = parts.userinfo {
        result.push_str(encode_url(userinfo).as_ref());
        result.push('@');
    }

    result.push_str(&host);
    result.push_str(parts.port);
    result.push_str(encode_url(parts.suffix).as_ref());
    result.push_str(encode_url(host_suffix).as_ref());
    result.push_str(encode_url(suffix).as_ref());
    result
}

// "mailto:测试@クロ.com?subject=你好" -> "mailto:%E6%B5%8B%E8%AF%95@xn--pckwg.com?subject=%E4%BD%A0%E5%A5%BD"
fn normalize_mailto_url(input: &str) -> String {
    let Some(mailto) = split_mailto_url(input) else {
        return encode_url(input).into_owned();
    };

    let Some((local, domain)) = mailto.address.rsplit_once('@') else {
        return encode_url(input).into_owned();
    };

    let Some(domain) = normalize_host(domain) else {
        return encode_url(input).into_owned();
    };

    let mut result = String::with_capacity(input.len());
    result.push_str(&input[..input.len() - mailto.body.len()]);
    result.push_str(encode_url(local).as_ref());
    result.push('@');
    result.push_str(&domain);
    result.push_str(encode_url(mailto.query).as_ref());
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::split_host_port;
    use crate::parse::is_url_scheme;

    #[test]
    fn trims_javascript_whitespace_before_normalizing() {
        for whitespace in [
            '\t', '\n', '\u{000b}', '\u{000c}', '\r', ' ', '\u{00a0}', '\u{1680}', '\u{2000}',
            '\u{200a}', '\u{2028}', '\u{2029}', '\u{202f}', '\u{205f}', '\u{3000}', '\u{feff}',
        ] {
            for url in ["/url", "https://example.com/path", "mailto:x@example.com"] {
                assert_eq!(
                    format_url_for_computers(&format!("{whitespace}{url}{whitespace}")),
                    url
                );
            }
        }
        assert_eq!(format_url_for_computers("/url\\\n"), "/url%5C");
        assert_eq!(format_url_for_computers("a\nb"), "a%0Ab");
        assert_eq!(format_url_for_computers("/url%0A"), "/url%0A");
        assert_eq!(
            format_url_for_computers("\u{0085}/url\u{0085}"),
            "%C2%85/url%C2%85"
        );
    }

    #[test]
    fn encodes_spaces_and_unicode() {
        assert_eq!(
            format_url_for_computers("https://example.com/a c/クロ"),
            "https://example.com/a%20c/%E3%82%AF%E3%83%AD"
        )
    }

    #[test]
    fn encodes_existing_valid_escapes() {
        assert_eq!(
            format_url_for_computers("https://example.com/a c"),
            "https://example.com/a%20c"
        )
    }

    #[test]
    fn encodes_invalid_percent() {
        assert_eq!(
            format_url_for_computers("https://example.com/100%"),
            "https://example.com/100%25"
        )
    }

    #[test]
    fn encodes_html_sensitive_chars() {
        assert_eq!(
            format_url_for_computers("https://example.com/<tags>"),
            "https://example.com/%3Ctags%3E"
        )
    }

    #[test]
    fn relocates_invalid_hostname_suffix_after_port() {
        for (source, expected) in [
            (
                "irc://foo.bar:2233r:2:33/baaz",
                "irc://foo.bar:33:2233r:2/baaz",
            ),
            ("http://foo:bad:80/path", "http://foo:80:bad/path"),
            ("//user@foo:bad:80/path", "//user@foo:80:bad/path"),
            ("http://foo:80/path", "http://foo:80/path"),
            ("http://[::1]:80/path", "http://[::1]:80/path"),
        ] {
            assert_eq!(format_url_for_computers(source), expected, "{source}");
            assert_eq!(crate::decode_url_for_display(source), expected, "{source}");
        }
    }

    #[test]
    fn normalizes_host_before_forbidden_character_suffix() {
        for (source, expected) in [
            (
                "https://examp�;.com/?searc?search=](uri)",
                "https://xn--examp-pm43a;.com/?searc?search=%5D(uri)",
            ),
            ("https://中;.com/path", "https://xn--fiq;.com/path"),
            (
                "https://中:8080;.com/path",
                "https://xn--fiq:8080;.com/path",
            ),
            ("https://u:p@中;.com/path", "https://u:p@xn--fiq;.com/path"),
            ("//中|tail/path", "//xn--fiq%7Ctail/path"),
            ("https://中%20tail/path", "https://xn--fiq%20tail/path"),
        ] {
            assert_eq!(format_url_for_computers(source), expected, "{source}");
        }
    }

    #[test]
    fn rejects_url_punctuation_before_idna_conversion() {
        for (source, expected) in [
            ("https://中!", "https://%E4%B8%AD!"),
            ("https://中&", "https://%E4%B8%AD&"),
            ("https://中=", "https://%E4%B8%AD="),
            ("https://中,", "https://%E4%B8%AD,"),
            ("https://中!/path", "https://%E4%B8%AD!/path"),
            ("//中!", "//%E4%B8%AD!"),
            ("https://中", "https://xn--fiq"),
            ("https://中+", "https://xn--+-kq6a"),
            ("https://中_", "https://xn--_-kq6a"),
        ] {
            assert_eq!(format_url_for_computers(source), expected, "{source}");
        }
    }

    #[test]
    fn encode_internationalized_domain() {
        assert_eq!(
            format_url_for_computers("https://クロ.com"),
            "https://xn--pckwg.com"
        )
    }

    #[test]
    fn encode_internationalized_domain_with_path() {
        assert_eq!(
            format_url_for_computers("https://クロ.com/a c/クロ"),
            "https://xn--pckwg.com/a%20c/%E3%82%AF%E3%83%AD"
        )
    }

    #[test]
    fn encode_internationalized_protocol_relative_domain() {
        assert_eq!(
            format_url_for_computers("//クロ.com/a c"),
            "//xn--pckwg.com/a%20c"
        )
    }

    #[test]
    fn does_not_treat_relative_urls_as_hosts() {
        assert_eq!(
            format_url_for_computers("クロ.com/a c"),
            "%E3%82%AF%E3%83%AD.com/a%20c"
        )
    }

    #[test]
    fn preserves_ipv6_hosts() {
        assert_eq!(
            format_url_for_computers("https://[::1]/クロ"),
            "https://[::1]/%E3%82%AF%E3%83%AD"
        )
    }

    #[test]
    fn extracts_ports_before_relocating_unclosed_bracketed_hosts() {
        for (source, expected) in [
            (
                "http://[2001:db:1896/a[b]?x=[y]",
                "http://:1896%5B2001:db/a%5Bb%5D?x=%5By%5D",
            ),
            ("//[2001:db:1896/path", "//:1896%5B2001:db/path"),
            (
                "http://user:pass@[2001:db:1896/path",
                "http://user:pass@:1896%5B2001:db/path",
            ),
            ("http://[2001:db/path", "http://%5B2001:db/path"),
            ("http://[2001:db:1896]/path", "http://[2001:db:1896]/path"),
            (
                "http://[2001:db:1896]:8080/path",
                "http://[2001:db:1896]:8080/path",
            ),
        ] {
            assert_eq!(format_url_for_computers(source), expected, "{source}");
        }
    }

    #[test]
    fn encodes_brackets_when_forbidden_characters_interrupt_the_host() {
        for (source, expected) in [
            ("//[::f|ff:192.0.2.1]/", "//%5B::f%7Cff:192.0.2.1%5D/"),
            ("https://[::f|ff]/", "https://%5B::f%7Cff%5D/"),
            ("https://[::f{ff}]/", "https://%5B::f%7Bff%7D%5D/"),
            ("https://[::f<ff>]/", "https://%5B::f%3Cff%3E%5D/"),
        ] {
            assert_eq!(format_url_for_computers(source), expected, "{source}");
        }
    }

    #[test]
    fn encodes_inner_brackets_in_bracketed_hosts() {
        for (source, expected) in [
            (
                "//[::ffff9[[~:192.92.0.2.1]/",
                "//[::ffff9%5B%5B~:192.92.0.2.1]/",
            ),
            ("https://[::1[extra]]/", "https://[::1%5Bextra%5D]/"),
            ("https://[::1]extra]/", "https://[::1%5Dextra]/"),
            ("https://[::1]:8080/path", "https://[::1]:8080/path"),
        ] {
            assert_eq!(format_url_for_computers(source), expected, "{source}");
        }
    }

    #[test]
    fn normalizes_mailto_domain() {
        assert_eq!(
            format_url_for_computers("mailto:user@クロ.com"),
            "mailto:user@xn--pckwg.com"
        )
    }

    #[test]
    fn preserves_empty_userinfo_marker() {
        assert_eq!(
            format_url_for_computers("https://@クロ.com"),
            "https://@xn--pckwg.com"
        )
    }

    #[test]
    fn normalizes_host_with_empty_port() {
        assert_eq!(
            format_url_for_computers("https://クロ.com:/"),
            "https://xn--pckwg.com:/"
        )
    }

    #[test]
    fn scheme_verify() {
        assert!(is_url_scheme("http"));
        assert!(is_url_scheme("frp"));
        assert!(!is_url_scheme("123"));
        assert!(is_url_scheme("git+shh"));
        assert!(!is_url_scheme("fake_https"));
        assert!(is_url_scheme("my.app-protocol"));
    }

    #[test]
    fn ipv6_and_port() {
        assert_eq!(split_host_port("localhost:8080"), ("localhost", ":8080"));
        assert_eq!(split_host_port("localhost8080"), ("localhost8080", ""));
        assert_eq!(split_host_port("[::1]:8000"), ("[::1]", ":8000"));
        assert_eq!(split_host_port("[::1:8000"), ("[::1", ":8000"));
        assert_eq!(split_host_port("[::1]abc"), ("[::1]abc", ""));
        assert_eq!(split_host_port("gsgfs.moe:443"), ("gsgfs.moe", ":443"));
    }

    #[test]
    fn normalizes_host_before_query_and_fragment() {
        assert_eq!(
            format_url_for_computers("https://クロ.com?subject=你好 世界"),
            "https://xn--pckwg.com?subject=%E4%BD%A0%E5%A5%BD%20%E4%B8%96%E7%95%8C"
        );

        assert_eq!(
            format_url_for_computers("https://クロ.com#你好 世界"),
            "https://xn--pckwg.com#%E4%BD%A0%E5%A5%BD%20%E4%B8%96%E7%95%8C"
        );
    }

    #[test]
    fn invalid_scheme_does_not_normalize_host() {
        assert_eq!(
            format_url_for_computers("fake_https://クロ.com/a b"),
            "fake_https://%E3%82%AF%E3%83%AD.com/a%20b"
        );

        assert_eq!(
            format_url_for_computers("123://クロ.com"),
            "123://%E3%82%AF%E3%83%AD.com"
        );
    }

    #[test]
    fn complex_userinfo() {
        assert_eq!(
            normalize_authority_url(
                "postgresql://",
                "db_user:db_passwd@localhost:5432",
                "/db?searchpath=schema"
            ),
            "postgresql://db_user:db_passwd@localhost:5432/db?searchpath=schema"
        );
        assert_eq!(
            normalize_authority_url("some-app://", "us^er:pass%wd@example.com", "/"),
            "some-app://us%5Eer:pass%25wd@example.com/"
        );
        assert_eq!(
            normalize_authority_url("app://", "user&:_passwd@example.com", "/"),
            "app://user&:_passwd@example.com/"
        );
        assert_eq!(
            normalize_authority_url("https://", "你:好@クロ.com", "/"),
            "https://%E4%BD%A0:%E5%A5%BD@xn--pckwg.com/"
        );
        assert_eq!(
            normalize_authority_url("app://", "u:pクロ.com", "/"),
            "app://u:p%E3%82%AF%E3%83%AD.com/"
        );
        assert_eq!(
            normalize_authority_url("app://", "クロ.com", "/"),
            "app://xn--pckwg.com/"
        );
        assert_eq!(
            normalize_authority_url("app://", "u%p@クロ.com", "/"),
            "app://u%25p@xn--pckwg.com/"
        );
    }

    #[test]
    fn complex_mailto() {
        assert_eq!(
            format_url_for_computers("MAILTO:user.name+label@クロ.com?subject=hi"),
            "MAILTO:user.name+label@xn--pckwg.com?subject=hi"
        );
        assert_eq!(
            format_url_for_computers("mailto:测试@クロ.com?subject=你好 世界"),
            "mailto:%E6%B5%8B%E8%AF%95@xn--pckwg.com?subject=%E4%BD%A0%E5%A5%BD%20%E4%B8%96%E7%95%8C"
        );
        assert_eq!(
            format_url_for_computers("mailto:user name"),
            "mailto:user%20name"
        );
    }

    #[test]
    fn public_userinfo_url_normalization() {
        assert_eq!(
            format_url_for_computers("https://你:好@クロ.com/a b"),
            "https://%E4%BD%A0:%E5%A5%BD@xn--pckwg.com/a%20b"
        );
    }

    #[test]
    fn invalid_domain_fallback() {
        let host = "a".repeat(500) + ".com";
        let url = format!("https://{}/api", host);
        assert!(format_url_for_computers(&url).contains("/api"));
    }
}
