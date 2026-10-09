//! Emphasis delimiter classification and its configurable policy.

use crate::MarkdownIt;
use crate::common::utils::is_punct_char;

#[derive(Debug, Clone, Copy)]
/// Classification of one emphasis delimiter run.
pub struct DelimiterRun {
    /// Starting marker character.
    pub marker: char,

    /// Boolean flag that determines if this delimiter could open an emphasis.
    pub can_open: bool,

    /// Boolean flag that determines if this delimiter could open an emphasis.
    pub can_close: bool,

    /// Total length of scanned delimiters.
    pub length: usize,
}

impl DelimiterRun {
    pub(crate) fn byte_length(&self) -> usize {
        self.length * self.marker.len_utf8()
    }
}

type DelimiterScanner =
    fn(src: &str, start: usize, pos_max: usize, can_split_word: bool) -> DelimiterRun;

#[derive(Debug)]
struct DelimiterScannerConfig(DelimiterScanner);

fn scan_delims_default(
    src: &str,
    start: usize,
    pos_max: usize,
    can_split_word: bool,
) -> DelimiterRun {
    let mut left_flanking = true;
    let mut right_flanking = true;

    let last_char = if start > 0 {
        src[..start].chars().next_back().unwrap()
    } else {
        // treat beginning of the line as a whitespace
        ' '
    };

    let mut chars = src[start..pos_max].chars();
    let marker = chars.next().unwrap();
    let next_char;
    let mut count = 1;

    loop {
        match chars.next() {
            None => {
                next_char = ' ';
                break;
            }
            Some(x) => {
                if x != marker {
                    // treat end of the line as a whitespace
                    next_char = x;
                    break;
                }
            }
        }
        count += 1;
    }

    // Classify NUL as U+FFFD without changing source offsets.
    let is_last_punct_char =
        last_char == '\0' || last_char.is_ascii_punctuation() || is_punct_char(last_char);
    let is_next_punct_char =
        next_char == '\0' || next_char.is_ascii_punctuation() || is_punct_char(next_char);

    let is_last_whitespace = last_char.is_whitespace();
    let is_next_whitespace = next_char.is_whitespace();

    #[allow(clippy::collapsible_if)]
    if is_next_whitespace {
        left_flanking = false;
    } else if is_next_punct_char {
        if !(is_last_whitespace || is_last_punct_char) {
            left_flanking = false;
        }
    }

    #[allow(clippy::collapsible_if)]
    if is_last_whitespace {
        right_flanking = false;
    } else if is_last_punct_char {
        if !(is_next_whitespace || is_next_punct_char) {
            right_flanking = false;
        }
    }

    let (can_open, can_close) = if !can_split_word {
        (
            left_flanking && (!right_flanking || is_last_punct_char),
            right_flanking && (!left_flanking || is_next_punct_char),
        )
    } else {
        (left_flanking, right_flanking)
    };

    DelimiterRun {
        marker,
        can_open,
        can_close,
        length: count,
    }
}

pub(crate) fn scan_delimiter_run(
    md: &MarkdownIt,
    src: &str,
    start: usize,
    pos_max: usize,
    can_split_word: bool,
) -> DelimiterRun {
    debug_assert!(start < pos_max);
    debug_assert!(src.is_char_boundary(start));
    debug_assert!(src.is_char_boundary(pos_max));

    if let Some(config) = md.ext.get::<DelimiterScannerConfig>() {
        return config.0(src, start, pos_max, can_split_word);
    }

    scan_delims_default(src, start, pos_max, can_split_word)
}

/// custom delimiter scanner
/// provider for `cjk_friendly` plugin. will not be made public for the time being.
pub(crate) fn set_delimiter_scanner(md: &mut MarkdownIt, scanner: DelimiterScanner) {
    md.ext.insert(DelimiterScannerConfig(scanner));
}
