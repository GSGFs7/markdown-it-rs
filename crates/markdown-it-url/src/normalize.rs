use crate::authority::split_authority;
use crate::host::normalize_host;
use crate::parse::{split_authority_url, split_mailto_url};
use crate::percent::encode_url;

pub fn format_url_for_computers(input: &str) -> String {
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
    let parts = split_authority(authority);

    // encode each part
    let Some(host) = normalize_host(parts.host) else {
        let mut result = encode_url(prefix).into_owned();
        result.push_str(encode_url(authority).as_ref());
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
    result.push_str("mailto:");
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
        assert_eq!(split_host_port("[::1:8000"), ("[::1:8000", ""));
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
            "mailto:user.name+label@xn--pckwg.com?subject=hi"
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
