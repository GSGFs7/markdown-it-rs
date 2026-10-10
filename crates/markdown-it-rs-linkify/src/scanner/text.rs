use crate::unicode;

/// map UTF-16 -> UTF-8
///
/// ## example
/// 
/// in "a😀中"
///
/// | UTF-16 position | units  |    bytes    |    means    |
/// |:---------------:|:------:|:-----------:|:-----------:|
/// |       0         | 0x0061 |      0      |     a       |
/// |       1         | 0xD38D |      1      |     😀      |
/// |       2         | 0xDE00 | usize::MAX  | inner of 😀 |
/// |       3         | 0x4E2D |      5      |     中      |
/// |       4         |  none  |      8      |     EOF     |
pub(super) struct Text {
    pub(super) units: Vec<u16>,
    pub(super) bytes: Vec<usize>,
}

impl Text {
    pub(super) fn new(source: &str) -> Self {
        let mut units = Vec::with_capacity(source.len());
        let mut bytes = Vec::with_capacity(source.len() + 1);
        for (offset, ch) in source.char_indices() {
            bytes.push(offset);

            let mut buffer = [0; 2];
            let encoded = ch.encode_utf16(&mut buffer);
            units.extend_from_slice(encoded);
            if encoded.len() == 2 {
                // whthin the char
                bytes.push(usize::MAX);
            }
        }
        bytes.push(source.len());

        Self { units, bytes }
    }

    /// reutrns (Unicaode code point, units consumed)
    pub(super) fn cp(&self, pos: usize, end: usize) -> Option<(u32, usize)> {
        if pos >= end {
            return None;
        }

        let a = self.units[pos] as u32;
        if (0xd800..=0xdbff).contains(&a) && pos + 1 < end {
            let b = self.units[pos + 1] as u32;
            if (0xdc00..=0xdfff).contains(&b) {
                return Some((0x10000 + ((a - 0xd800) << 10) + b - 0xdc00, 2));
            }
        }

        Some((a, 1))
    }

    /// read a UTF-16 unit
    pub(super) fn at(&self, pos: usize, end: usize) -> Option<u16> {
        (pos < end).then(|| self.units[pos])
    }

    /// ASCII case-insensitive matching
    pub(super) fn ascii(&self, pos: usize, needle: &[u8], end: usize) -> bool {
        pos + needle.len() <= end
            && needle.iter().enumerate().all(|(i, &b)| {
                self.units[pos + i] <= 127 && (self.units[pos + i] as u8).eq_ignore_ascii_case(&b)
            })
    }

    /// consume a domain char
    pub(super) fn pseudo_end(&self, pos: usize, end: usize) -> Option<usize> {
        let (cp, width) = self.cp(pos, end)?;
        // uc.micro Any consumes an astral scalar as a pair. A scanner search
        // must never begin a domain in the low half of that scalar.
        if (0xdc00..=0xdfff).contains(&cp) {
            return None;
        }

        unicode::pseudo(cp).then_some(pos + width)
    }

    pub(super) fn is_zcc(&self, pos: usize, end: usize) -> bool {
        self.cp(pos, end).is_some_and(|(cp, _)| unicode::zcc(cp))
    }

    pub(super) fn terminator(&self, pos: usize, end: usize) -> bool {
        let Some((cp, _)) = self.cp(pos, end) else {
            return true;
        };

        if !unicode::separator(cp) && !unicode::zpcc(cp) {
            return false;
        }

        if matches!(cp, 0x2d | 0x5f) {
            return false;
        }

        if cp == 0x3a && self.at(pos + 1, end).is_some_and(digit) {
            return false;
        }

        if cp == 0x2e {
            if self.at(pos + 1, end) == Some(b'-' as u16) {
                return false;
            }

            if self
                .cp(pos + 1, end)
                .is_some_and(|(next, _)| !unicode::zpcc(next))
            {
                return false;
            }
        }

        true
    }
}

pub(super) fn digit(ch: u16) -> bool {
    (b'0' as u16..=b'9' as u16).contains(&ch)
}

pub(super) fn alnum(ch: u16) -> bool {
    ch <= 127 && (ch as u8).is_ascii_alphanumeric()
}
