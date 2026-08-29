#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) struct Authority<'a> {
    pub(crate) userinfo: Option<&'a str>,
    pub(crate) host: &'a str,
    pub(crate) port: &'a str,
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

    Authority {
        userinfo,
        host,
        port,
    }
}

// this fn only split host:port
pub(crate) fn split_host_port(input: &str) -> (&str, &str) {
    // IPv6
    if input.starts_with('[') {
        let Some(end) = input.find(']') else {
            return (input, "");
        };

        let (host, port) = input.split_at(end + 1);
        if port.is_empty()
            || (port.starts_with(':') && port[1..].bytes().all(|byte| byte.is_ascii_digit()))
        {
            return (host, port);
        }

        return (input, "");
    }

    if let Some((host, port)) = input.rsplit_once(':')
        && port.bytes().all(|byte| byte.is_ascii_digit())
    {
        return (host, &input[host.len()..]);
    }

    (input, "")
}
