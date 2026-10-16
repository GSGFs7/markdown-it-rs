pub mod blocks;
pub mod regexps;

pub(crate) fn anchor_delta(content: &str) -> i32 {
    use regexps::{HTML_LINK_CLOSE, HTML_LINK_OPEN};

    if HTML_LINK_OPEN.is_match(content) {
        1
    } else if HTML_LINK_CLOSE.is_match(content) {
        -1
    } else {
        0
    }
}
