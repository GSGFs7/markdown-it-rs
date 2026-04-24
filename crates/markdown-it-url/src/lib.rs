mod authority;
mod display;
mod elide;
mod host;
mod normalize;
mod parse;
mod percent;
pub mod urlencode;

pub use display::format_url_for_humans;
pub use normalize::format_url_for_computers;
pub use urlencode::{AsciiSet, ENCODE_DEFAULT_CHARS, encode};
