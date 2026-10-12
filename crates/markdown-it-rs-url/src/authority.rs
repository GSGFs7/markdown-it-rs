#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) struct Authority<'a> {
    pub(crate) userinfo: Option<&'a str>,
    pub(crate) host: &'a str,
    pub(crate) port: &'a str,
    pub(crate) suffix: &'a str,
}

pub(crate) fn split_authority(input: &str) -> Authority<'_> {
    // userinfo: "db_user:db_passwd"
    // host_port: "localhost:5432"
    let (userinfo, host_port) = input
        .rsplit_once('@')
        .map_or((None, input), |(userinfo, host_port)| {
            (Some(userinfo), host_port)
        });
    let (host, port) = split_host_port(host_port);
    let (host, suffix) = split_hostname_suffix(host);

    Authority {
        userinfo,
        host,
        port,
        suffix,
    }
}

fn split_hostname_suffix(host: &str) -> (&str, &str) {
    if host.starts_with('[') && host.ends_with(']') {
        return (host, "");
    }
    let is_ascii_host_char =
        |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'_');
    let mut start = 0;
    for label in host.split('.') {
        if label
            .bytes()
            .any(|byte| byte.is_ascii() && !is_ascii_host_char(byte))
        {
            // markdown-it compatibility: move invalid host content after the port.
            let valid = label
                .bytes()
                .take_while(|&byte| is_ascii_host_char(byte))
                .count();
            return host.split_at(start + valid);
        }
        start += label.len() + 1;
    }
    (host, "")
}

// this fn only split host:port
pub(crate) fn split_host_port(input: &str) -> (&str, &str) {
    // IPv6
    if input.starts_with('[')
        && let Some(end) = input.find(']')
    {
        let (host, port) = input.split_at(end + 1);
        if port.is_empty()
            || (port.starts_with(':') && port[1..].bytes().all(|byte| byte.is_ascii_digit()))
        {
            return (host, port);
        }

        return (input, "");
    }

    // markdown-it compatibility: extract numeric ports even from unclosed brackets.
    // A bare trailing colon is not a port.
    if let Some((host, port)) = input.rsplit_once(':')
        && !port.is_empty()
        && port.bytes().all(|byte| byte.is_ascii_digit())
    {
        return (host, &input[host.len()..]);
    }

    (input, "")
}
