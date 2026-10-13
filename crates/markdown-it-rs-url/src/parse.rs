//! Component parsing and formatting compatible with mdurl 2.1.0.
//!
//! https://github.com/markdown-it/mdurl/blob/master/lib/parse.mjs
//! This deliberately follows mdurl's permissive parser rather than RFC/WHATWG
//! validation. Encoding and display must use the same component boundaries.

use std::borrow::Cow;

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ParsedUrl<'a> {
    pub(crate) protocol: Option<&'a str>,
    pub(crate) slashes: bool,
    pub(crate) auth: Option<Cow<'a, str>>,
    pub(crate) hostname: Option<Cow<'a, str>>,
    pub(crate) port: Option<&'a str>,
    pub(crate) pathname: Option<Cow<'a, str>>,
    pub(crate) search: Option<Cow<'a, str>>,
    pub(crate) hash: Option<Cow<'a, str>>,
}

impl<'a> ParsedUrl<'a> {
    /// Equivalent to `mdurl.parse(input, true)`.
    pub(crate) fn parse(input: &'a str) -> Self {
        let mut rest = input.trim_matches(is_javascript_whitespace);
        let mut url = Self::default();
        if let Some(end) = rest.find(':')
            && end > 0
            && rest[..end]
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
        {
            url.protocol = Some(&rest[..=end]);
            rest = &rest[end + 1..];
        }

        // Protocol lookup is deliberately case-sensitive, as in mdurl.
        let hostless = url.protocol == Some("javascript:");
        let slashes = rest.starts_with("//");
        if slashes && !hostless {
            url.slashes = true;
            rest = &rest[2..];
        }

        let mut path_prefix = "";
        if !hostless
            && (slashes
                || url
                    .protocol
                    .is_some_and(|protocol| !is_slashed_protocol(protocol)))
        {
            let auth_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
            if let Some(at) = rest[..auth_end].rfind('@') {
                url.auth = Some(Cow::Borrowed(&rest[..at]));
                rest = &rest[at + 1..];
            }

            let mut host_end = rest.find(is_host_terminator).unwrap_or(rest.len());
            // A bare trailing colon belongs to the path, not the port.
            if rest[..host_end].ends_with(':') {
                host_end -= 1;
            }
            let mut host = &rest[..host_end];
            rest = &rest[host_end..];
            if let Some(colon) = host.rfind(':')
                && host[colon + 1..].bytes().all(|byte| byte.is_ascii_digit())
            {
                let port = &host[colon + 1..];
                if !port.is_empty() {
                    url.port = Some(port);
                }
                host = &host[..colon];
            }

            let bracketed = host.starts_with('[') && host.ends_with(']');
            if !bracketed {
                let (end, suffix) = hostname_bounds(host);
                path_prefix = &host[suffix..];
                host = &host[..end];
            }
            // mdurl's length limits count JavaScript UTF-16 code units.
            if host.encode_utf16().count() > 255 {
                host = "";
            } else if bracketed {
                host = &host[1..host.len() - 1];
            }
            url.hostname = Some(Cow::Borrowed(host));
        }

        // Invalid hostname content moves in front of the path, after the port.
        // Split fragment before query so '?' inside a fragment stays there.
        let mut path: Cow<'a, str> = if path_prefix.is_empty() {
            Cow::Borrowed(rest)
        } else if rest.is_empty() {
            Cow::Borrowed(path_prefix)
        } else {
            Cow::Owned(format!("{path_prefix}{rest}"))
        };
        if let Some(hash) = path.find('#') {
            url.hash = Some(take_suffix(&mut path, hash));
        }
        if let Some(query) = path.find('?') {
            url.search = Some(take_suffix(&mut path, query));
        }
        if !path.is_empty() {
            url.pathname = Some(path);
        }
        url
    }

    pub(crate) fn recodes_hostname(&self) -> bool {
        matches!(self.protocol, None | Some("http:" | "https:" | "mailto:"))
    }

    /// Brackets are determined from the final hostname, after transformation.
    pub(crate) fn format(&self) -> String {
        let mut result = String::new();
        result.push_str(self.protocol.unwrap_or(""));
        if self.slashes {
            result.push_str("//");
        }
        if let Some(auth) = &self.auth
            && !auth.is_empty()
        {
            result.push_str(auth);
            result.push('@');
        }
        if let Some(host) = &self.hostname {
            if host.contains(':') {
                result.push('[');
                result.push_str(host);
                result.push(']');
            } else {
                result.push_str(host);
            }
        }
        if let Some(port) = self.port {
            result.push(':');
            result.push_str(port);
        }
        for component in [&self.pathname, &self.search, &self.hash]
            .into_iter()
            .flatten()
        {
            result.push_str(component);
        }
        result
    }
}

fn take_suffix<'a>(value: &mut Cow<'a, str>, at: usize) -> Cow<'a, str> {
    match value {
        Cow::Borrowed(text) => {
            let (head, tail) = text.split_at(at);
            *text = head;
            Cow::Borrowed(tail)
        }
        Cow::Owned(text) => Cow::Owned(text.split_off(at)),
    }
}

fn hostname_bounds(host: &str) -> (usize, usize) {
    let mut start = 0usize;
    for label in host.split('.') {
        if label.encode_utf16().count() > 63
            || label
                .chars()
                .any(|ch| ch.is_ascii() && !is_hostname_byte(ch as u8))
        {
            // mdurl's hostnamePartStart uses `(.*)` without dotAll. If an
            // invalid label contains a JS line separator, that regex fails:
            // the whole label is discarded and only later labels become path.
            if label.contains(['\u{2028}', '\u{2029}']) {
                return (
                    start.saturating_sub(1),
                    (start + label.len() + 1).min(host.len()),
                );
            }
            // mdurl substitutes non-ASCII UTF-16 units with 'x' for validation,
            // but only retains an ASCII prefix when a label fails validation.
            let valid = label
                .bytes()
                .take(63)
                .take_while(|&byte| is_hostname_byte(byte))
                .count();
            return (start + valid, start + valid);
        }
        start += label.len() + 1;
    }
    (host.len(), host.len())
}

fn is_hostname_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'_')
}

fn is_slashed_protocol(protocol: &str) -> bool {
    matches!(protocol, "http:" | "https:" | "ftp:" | "gopher:" | "file:")
}

fn is_host_terminator(ch: char) -> bool {
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

fn is_javascript_whitespace(ch: char) -> bool {
    matches!(ch, '\u{0009}'..='\u{000d}' | ' ' | '\u{00a0}' | '\u{1680}'
        | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}'
        | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

#[cfg(test)]
mod tests {
    use super::ParsedUrl;

    #[test]
    fn invalid_hostname_suffix_moves_after_port() {
        let url = ParsedUrl::parse("http://foo:bad:80/path?query#fragment");
        assert_eq!(url.hostname.as_deref(), Some("foo"));
        assert_eq!(url.port, Some("80"));
        assert_eq!(url.pathname.as_deref(), Some(":bad/path"));
        assert_eq!(url.search.as_deref(), Some("?query"));
        assert_eq!(url.hash.as_deref(), Some("#fragment"));
        assert_eq!(url.format(), "http://foo:80:bad/path?query#fragment");
    }

    #[test]
    fn invalid_labels_with_js_line_separators_follow_mdurl_regex_fallback() {
        for separator in ['\u{2028}', '\u{2029}'] {
            let input = format!("http://valid.foo{separator}!.com:80/path");
            let url = ParsedUrl::parse(&input);
            assert_eq!(url.hostname.as_deref(), Some("valid"));
            assert_eq!(url.pathname.as_deref(), Some("com/path"));
            assert_eq!(url.format(), "http://valid:80com/path");
        }
    }

    #[test]
    fn hostname_limits_count_utf16_before_punycode_conversion() {
        for (size, is_host) in [(31, true), (32, false)] {
            let label = "😀".repeat(size);
            let source = format!("http://{label}.com/path");
            let url = ParsedUrl::parse(&source);
            if is_host {
                assert_eq!(
                    url.hostname.as_deref(),
                    Some(format!("{label}.com").as_str())
                );
                assert_eq!(url.pathname.as_deref(), Some("/path"));
            } else {
                assert_eq!(url.hostname.as_deref(), Some(""));
                assert_eq!(
                    url.pathname.as_deref(),
                    Some(format!("{label}.com/path").as_str())
                );
            }
        }
        for size in [255, 256] {
            let host = ("x".repeat(50) + ".").repeat(4) + &"x".repeat(size - 204);
            let source = format!("http://{host}/path");
            let url = ParsedUrl::parse(&source);
            assert_eq!(
                url.hostname.as_deref(),
                Some(if size == 255 { host.as_str() } else { "" })
            );
            assert_eq!(url.pathname.as_deref(), Some("/path"));
        }
    }

    #[test]
    fn bracket_formatting_depends_on_the_final_hostname() {
        for (input, host, output) in [
            ("http://[200]", "200", "http://200"),
            ("http://[::1]", "::1", "http://[::1]"),
            ("httper:user@[::1]:80/", "::1", "httper:user@[::1]:80/"),
        ] {
            let url = ParsedUrl::parse(input);
            assert_eq!(url.hostname.as_deref(), Some(host));
            assert_eq!(url.format(), output);
        }
    }
}
