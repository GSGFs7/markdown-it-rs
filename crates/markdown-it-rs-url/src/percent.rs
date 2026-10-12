use std::borrow::Cow;

use crate::urlencode::{ENCODE_DEFAULT_CHARS, encode};

pub(crate) fn encode_url(input: &str) -> Cow<'_, str> {
    encode(input, ENCODE_DEFAULT_CHARS, true)
}

pub(crate) fn decode_for_display(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut result = String::with_capacity(input.len());
    let mut pos = 0;

    while pos < bytes.len() {
        if bytes[pos] == b'%' {
            let start = pos;
            let mut decoded = Vec::new();

            // find all needs decode, it may encoded by many precents.
            // such as: "%E3%82%AF%E3%83%AD" -> "クロ"
            while pos + 2 < bytes.len() && bytes[pos] == b'%' {
                let Some(byte) = decode_hex_byte(bytes[pos + 1], bytes[pos + 2]) else {
                    break;
                };

                decoded.push(byte);
                pos += 3;
            }

            // if read some %XX
            if !decoded.is_empty() {
                decode_percent_bytes(&decoded, &mut result);
                continue;
            }

            result.push('%');
            pos = start + 1;
            continue;
        }

        // normal char
        let ch = input[pos..]
            .chars()
            .next()
            // because `pos` < `bytes.len()` always true (while loop)
            // this never panic in general
            .expect("pos is always at a UTF-8 boundary");
        result.push(ch);
        pos += ch.len_utf8(); // utf-8, 1-4 bytes
    }

    result
}

fn decode_percent_bytes(bytes: &[u8], result: &mut String) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut pos = 0;
    while pos < bytes.len() {
        let byte = bytes[pos];
        // append ascii
        if byte.is_ascii() {
            if is_safe_display_char(byte as char) {
                result.push(byte as char);
            } else {
                result.push('%');
                result.push(HEX[(byte >> 4) as usize] as char);
                result.push(HEX[(byte & 15) as usize] as char);
            }
            pos += 1;
            continue;
        }

        // not a ascii, calculate expected length
        // https://datatracker.ietf.org/doc/html/rfc3629#section-3
        let width = match byte {
            0xc0..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf7 => 4,
            _ => 1,
        };
        if width > 1
            && let Some(sequence) = bytes.get(pos..pos + width)
            && sequence[1..].iter().all(|byte| byte & 0xc0 == 0x80)
        {
            // try decode to UTF-8
            match std::str::from_utf8(sequence) {
                Ok(text) => result.push_str(text),
                // markdown-it compatibility: replace each byte of an invalid scalar value.
                Err(_) => result.extend(std::iter::repeat_n('\u{fffd}', width)),
            }
            pos += width;
        } else {
            result.push('\u{fffd}');
            pos += 1;
        }
    }
}

// two hex -> a byte
fn decode_hex_byte(high: u8, low: u8) -> Option<u8> {
    Some(hex_value(high)? * 16 + hex_value(low)?)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn is_safe_display_char(ch: char) -> bool {
    !matches!(
        ch,
        ';' | '/' | '?' | ':' | '@' | '&' | '=' | '+' | '$' | ',' | '#' | '%'
    )
}
