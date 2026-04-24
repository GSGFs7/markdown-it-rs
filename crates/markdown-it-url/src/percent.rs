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

            // try utf-8
            if !decoded.is_empty()
                // legal UTF-8
                && let Ok(decoded) = std::str::from_utf8(&decoded)
                // safety display
                && decoded.chars().all(is_safe_display_char)
            {
                result.push_str(decoded);
                continue;
            }

            // fallback, keep it
            result.push_str(&input[start..pos.max(start + 1)]);
            if pos == start {
                // '%' only
                // not process the remaining part
                pos += 1;
            }
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
    if !ch.is_ascii() && !ch.is_control() {
        return true;
    }

    ch.is_ascii_alphanumeric() || matches!(ch, ' ' | '-' | '_' | '.' | '~')
}
