//! Regexps to match html elements
//!
#![allow(non_upper_case_globals)]
use std::sync::LazyLock;

use regex::Regex;

// Match JavaScript's \s: Unicode White_Space plus BOM, excluding NEL.
pub(crate) const HTML_WHITESPACE: &str = r"[\s\u{FEFF}--\u{0085}]";

const attr_name: &str = r#"[a-zA-Z_:][a-zA-Z0-9:._-]*"#;

const unquoted: &str = r#"[^"'=<>`\x00-\x20]+"#;
const single_quoted: &str = r#"'[^']*'"#;
const double_quoted: &str = r#""[^"]*""#;

pub static HTML_OPEN_CLOSE_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    let attr_value = format!("(?:{unquoted}|{single_quoted}|{double_quoted})");
    let attribute = format!(
        "(?:{HTML_WHITESPACE}+{attr_name}(?:{HTML_WHITESPACE}*={HTML_WHITESPACE}*{attr_value})?)"
    );
    let open_tag = format!("<[A-Za-z][A-Za-z0-9\\-]*{attribute}*{HTML_WHITESPACE}*/?>");
    let close_tag = format!("</[A-Za-z][A-Za-z0-9\\-]*{HTML_WHITESPACE}*>");

    Regex::new(&format!("^(?:{open_tag}|{close_tag})")).unwrap()
});

pub static HTML_LINK_OPEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!("(?i)^<a(?:>|{HTML_WHITESPACE})")).unwrap());

pub static HTML_LINK_CLOSE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!("(?i)^</a{HTML_WHITESPACE}*>")).unwrap());
