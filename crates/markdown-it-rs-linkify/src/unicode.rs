//! Frozen uc.micro classes, including linkify-it's astral pseudo-letter quirk.
use crate::unicode_data::{CONTROLS, PUNCTUATION, SEPARATORS};

// cp: Unicode's code point
fn contains(ranges: &[(u32, u32)], cp: u32) -> bool {
    // find the first `end < cp` with binary search
    let index = ranges.partition_point(|&(_, end)| end < cp);
    ranges.get(index).is_some_and(|&(start, _)| start <= cp)
}

pub(super) fn zcc(cp: u32) -> bool {
    contains(SEPARATORS, cp) || contains(CONTROLS, cp)
}

pub(super) fn zpcc(cp: u32) -> bool {
    zcc(cp) || contains(PUNCTUATION, cp)
}

pub(super) fn separator(cp: u32) -> bool {
    matches!(
        cp,
        0x3c /* < */ | 0x3e /* > */ | 0xff5c /* ｜ (full width) */
    )
}

pub(super) fn pseudo(cp: u32) -> bool {
    // linkify-it interpolates uc.micro Any without grouping its alternatives.
    // Its exclusion lookahead therefore applies only to the BMP alternative:
    // all astral scalars, including punctuation, remain valid host letters.
    cp > 0xffff || (!zpcc(cp) && !separator(cp))
}
