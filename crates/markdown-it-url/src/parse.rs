#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) struct AuthorityUrl<'a> {
    pub(crate) prefix: &'a str,    // https://
    pub(crate) authority: &'a str, // example.com
    pub(crate) suffix: &'a str,    // /api/hello?q=hi
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) struct MailtoUrl<'a> {
    pub(crate) body: &'a str,
    pub(crate) address: &'a str,
    pub(crate) query: &'a str,
}

pub(crate) fn split_authority_url(input: &str) -> Option<AuthorityUrl<'_>> {
    let authority_start = if input.starts_with("//") {
        // Protocol-relative URL: //example.com
        // no longer recommended, should use https in entre website
        2
    } else {
        // common usage, protocol + "://" + domain + ...
        // protocol: https, ftp, wss, postgresql, osu e.g. (mailto & sms & tel don't need "//")
        let scheme_end = input.find("://")?;
        let scheme = &input[..scheme_end];
        if !is_url_scheme(scheme) {
            return None;
        }

        scheme_end + 3
    };

    let (authority, suffix) = split_authority_suffix(&input[authority_start..]);

    Some(AuthorityUrl {
        prefix: &input[..authority_start],
        authority,
        suffix,
    })
}

pub(crate) fn split_authority_suffix(input: &str) -> (&str, &str) {
    // example.com
    // example.com/a
    // example.com?a=b
    // example.com#1145
    //            ^--- here
    let authority_end = input.find(['/', '?', '#']).unwrap_or(input.len());
    (&input[..authority_end], &input[authority_end..])
}

pub(crate) fn split_mailto_url(input: &str) -> Option<MailtoUrl<'_>> {
    let body = strip_ascii_prefix(input, "mailto:")?;
    let query_start = body.find('?').unwrap_or(body.len());

    Some(MailtoUrl {
        body,
        address: &body[..query_start],
        query: &body[query_start..],
    })
}

pub(crate) fn strip_ascii_prefix<'a>(input: &'a str, prefix: &str) -> Option<&'a str> {
    if input
        .get(..prefix.len())
        .is_some_and(|actual| actual.eq_ignore_ascii_case(prefix))
    {
        Some(&input[prefix.len()..])
    } else {
        None
    }
}

// this fn only verify scheme
pub(crate) fn is_url_scheme(input: &str) -> bool {
    let mut bytes = input.bytes();
    if let Some(first) = bytes.next() {
        // RFC 3986
        // first must in a-z or A-Z
        // entire scheme must in number, alphabet, '+'， '-', '.'
        // such as: git+ssh://...
        first.is_ascii_alphabetic()
            && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
    } else {
        false
    }
}
