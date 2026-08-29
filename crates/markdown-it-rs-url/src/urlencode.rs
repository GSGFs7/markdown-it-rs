use std::borrow::Cow;

const HEX_UPPER: &[u8; 16] = b"0123456789ABCDEF";

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct AsciiSet {
    allowed: u128,
}

impl AsciiSet {
    pub const fn empty() -> Self {
        Self { allowed: 0 }
    }

    pub const fn default() -> Self {
        Self::empty()
            .add(b';')
            .add(b',')
            .add(b'/')
            .add(b'?')
            .add(b':')
            .add(b'@')
            .add(b'&')
            .add(b'=')
            .add(b'+')
            .add(b'$')
            .add(b'-')
            .add(b'_')
            .add(b'.')
            .add(b'!')
            .add(b'~')
            .add(b'*')
            .add(b'\'')
            .add(b'(')
            .add(b')')
            .add(b'#')
    }

    pub const fn add(mut self, byte: u8) -> Self {
        if byte < 128 {
            self.allowed |= 1u128 << byte;
        }
        self
    }

    pub fn contains(self, byte: u8) -> bool {
        byte < 128 && (self.allowed & (1u128 << byte)) != 0
    }
}

pub const ENCODE_DEFAULT_CHARS: AsciiSet = AsciiSet::default();

pub fn encode(input: &str, exclude: AsciiSet, keep_escaped: bool) -> Cow<'_, str> {
    let mut result: Option<String> = None;
    let bytes = input.as_bytes();

    let mut i = 0;
    while i < bytes.len() {
        let cur = bytes[i];

        if keep_escaped
            && cur == b'%'
            && i + 2 < bytes.len()
            && bytes[i + 1].is_ascii_hexdigit()
            && bytes[i + 2].is_ascii_hexdigit()
        {
            if let Some(ref mut r) = result {
                r.push('%');
                r.push(bytes[i + 1] as char);
                r.push(bytes[i + 2] as char);
            }
            i += 3;
            continue;
        }

        if cur.is_ascii_alphanumeric() || exclude.contains(cur) {
            if let Some(ref mut r) = result {
                r.push(cur as char);
            }
        } else {
            // needs escap
            if result.is_none() {
                // if empty
                let mut new_result = String::with_capacity(input.len() * 3);
                new_result.push_str(
                    input
                        .get(..i)
                        .expect("encode only copies prefixes on UTF-8 boundaries"),
                );
                result = Some(new_result);
            }
            if let Some(ref mut r) = result {
                r.push('%');
                r.push(HEX_UPPER[(cur >> 4) as usize] as char);
                r.push(HEX_UPPER[(cur & 0x0F) as usize] as char);
            }
        }

        i += 1;
    }

    match result {
        Some(r) => Cow::Owned(r),
        None => Cow::Borrowed(input),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_alphanumerics_unchanged() {
        let mut alphabet_with_number = String::new();
        for c in 'a'..='z' {
            alphabet_with_number.push(c);
        }
        for n in '0'..='9' {
            alphabet_with_number.push(n);
        }
        assert_eq!(
            encode(&alphabet_with_number, ENCODE_DEFAULT_CHARS, true),
            "abcdefghijklmnopqrstuvwxyz0123456789"
        );
    }

    #[test]
    fn encodes_spaces() {
        let out = encode("a b", ENCODE_DEFAULT_CHARS, true);
        assert_eq!(out, "a%20b");
    }

    #[test]
    fn keeps_default_allowed_ascii() {
        let out = encode("a/b?c=d#e", ENCODE_DEFAULT_CHARS, true);
        assert_eq!(out, "a/b?c=d#e");
    }

    #[test]
    fn encodes_reserved_htmlish_chars() {
        let out = encode("<tag>", ENCODE_DEFAULT_CHARS, true);
        assert_eq!(out, "%3Ctag%3E");
    }

    #[test]
    fn encode_unicode_as_bytes() {
        let out = encode("你好", ENCODE_DEFAULT_CHARS, true);
        assert_eq!(out, "%E4%BD%A0%E5%A5%BD");
    }

    #[test]
    fn preserves_valid_escape_sequences_then_enabled() {
        let out = encode("a%20b", ENCODE_DEFAULT_CHARS, true);
        assert_eq!(out, "a%20b");
    }

    #[test]
    fn reencodes_percent_when_keep_escaped_is_disabled() {
        let out = encode("a%20b", ENCODE_DEFAULT_CHARS, false);
        assert_eq!(out, "a%2520b");
    }

    #[test]
    fn encodes_invalid_escape_sequences() {
        let out = encode("a%2g", ENCODE_DEFAULT_CHARS, true);
        assert_eq!(out, "a%252g");
    }

    #[test]
    fn encodes_alone_percent() {
        let out = encode("100%", ENCODE_DEFAULT_CHARS, true);
        assert_eq!(out, "100%25");
    }

    #[test]
    fn encodes_a_empty_string() {
        let out = encode("", ENCODE_DEFAULT_CHARS, true);
        assert_eq!(out, "");
    }

    #[test]
    fn encodes_a_single_escape_char() {
        let out = encode(" ", ENCODE_DEFAULT_CHARS, true);
        assert_eq!(out, "%20");
    }

    #[test]
    fn encodes_breaking_escape() {
        let out1 = encode("%", ENCODE_DEFAULT_CHARS, true);
        assert_eq!(out1, "%25");

        let out2 = encode("%2", ENCODE_DEFAULT_CHARS, true);
        assert_eq!(out2, "%252");

        let out3 = encode("%25", ENCODE_DEFAULT_CHARS, true);
        assert_eq!(out3, "%25");
    }

    #[test]
    fn encodes_valid_escaped_and_next_needs_escape_char() {
        let out = encode("%20<", ENCODE_DEFAULT_CHARS, true);
        assert_eq!(out, "%20%3C");
    }

    #[test]
    fn encodes_utf8_with_ascii() {
        let out = encode("hi你好a", ENCODE_DEFAULT_CHARS, true);
        assert_eq!(out, "hi%E4%BD%A0%E5%A5%BDa")
    }

    #[test]
    fn encodes_consecutive_space() {
        let out = encode("   ", ENCODE_DEFAULT_CHARS, true);
        assert_eq!(out, "%20%20%20");
    }
}
