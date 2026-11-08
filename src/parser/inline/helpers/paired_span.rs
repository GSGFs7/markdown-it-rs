//! Scan boundaries of custom paired syntax such as `{{value}}` or `[[text]]`.
//!
//! [`scan`] returns UTF-8 byte offsets into its input and keeps the body verbatim.
//! It does not decode escapes or interpret Markdown — a closer inside a code span
//! still closes the span. Whether the body is kept literal or parsed as inline
//! children is up to the caller.
//!
//! Use this helper when:
//!
//! - the opening and closing markers are arbitrary strings and may differ;
//! - the body may stay literal or be parsed as inline children;
//! - you need escape, nesting, empty-body, or line-break policies.
//!
//! Prefer [`code_pair`], [`emph_pair`], or [`full_link`] when the syntax matches
//! code spans, emphasis-like delimiters, or links. `paired_span` only scans, so
//! call it from a plugin's low-level parse callback.
//!
//! [`code_pair`]: crate::parser::inline::helpers::code_pair
//! [`emph_pair`]: crate::parser::inline::helpers::emph_pair
//! [`full_link`]: crate::parser::inline::helpers::full_link
//!
//! A literal leaf:
//!
//! ```rust
//! use markdown_it::parser::inline::helpers::paired_span::{
//!     EscapePolicy, PairOptions,
//! };
//! use markdown_it::plugin::PluginSpec;
//! use markdown_it::MarkdownIt;
//!
//! #[derive(Debug)]
//! struct Variable(String);
//!
//! let options = PairOptions::new("{{", "}}").escape(EscapePolicy::Backslash);
//! let plugin = PluginSpec::new("variable")
//!     .inline_leaf::<Variable>("variable")
//!     .marker('{')
//!     .parse(move |cx| {
//!         let matched = cx.scan_pair(&options)?;
//!         let value = Variable(cx.remaining_slice(matched.body)?.to_owned());
//!         cx.advance(matched.consumed).then_some(value)
//!     })
//!     .render_html(|value, html| { html.element("code").text(&value.0); })
//!     .finish();
//! let md = MarkdownIt::builder().plugin(plugin).build();
//! assert_eq!(md.render("{{*something*}}"), "<p><code>*something*</code></p>\n");
//! ```
//!
//! A container whose body is parsed as Markdown:
//!
//! ```rust
//! use markdown_it::parser::inline::helpers::paired_span::PairOptions;
//! use markdown_it::plugin::PluginSpec;
//! use markdown_it::MarkdownIt;
//!
//! #[derive(Debug)]
//! struct Boxed;
//!
//! let options = PairOptions::new("[[", "]]");
//! let plugin = PluginSpec::new("boxed")
//!     .inline_container::<Boxed>("boxed")
//!     .marker('[')
//!     .parse(move |cx| {
//!         let base = cx.consumed();
//!         let matched = cx.consume_pair(&options)?;
//!         let body = base + matched.body.start..base + matched.body.end;
//!         Some((Boxed, body))
//!     })
//!     .html_tag("span")
//!     .finish();
//! let md = MarkdownIt::builder().plugin(plugin).build();
//! assert_eq!(md.render("[[*something*]]"), "<p><span><em>something</em></span></p>\n");
//! ```

use std::ops::Range;

/// Backslash handling when recognizing markers in the body.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EscapePolicy {
    /// Backslashes are literal.
    #[default]
    None,
    /// A marker is escaped by an odd run of preceding backslashes.
    /// Backslashes are preserved in the body. Markers consume atomically, so
    /// their own trailing backslashes do not escape the next body character.
    Backslash,
}

/// Whether nested openers must be balanced.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NestingPolicy {
    /// The first unescaped closer ends the span.
    #[default]
    None,
    /// Balance nested occurrences of the same marker pair.
    SamePair,
}

/// Options for scanning a paired span.
///
/// Defaults: no escaping or nesting, empty body allowed, line breaks forbidden.
/// Fields are private, so the options are always valid.
#[derive(Debug, Clone, Copy)]
pub struct PairOptions<'a> {
    open: &'a str,
    close: &'a str,
    escape: EscapePolicy,
    nesting: NestingPolicy,
    allow_empty: bool,
    allow_line_breaks: bool,
}

impl<'a> PairOptions<'a> {
    /// Use `open` and `close` as markers.
    /// Equal markers are allowed unless [`NestingPolicy::SamePair`] is used.
    ///
    /// # Panics
    ///
    /// Panics if either marker is empty.
    pub fn new(open: &'a str, close: &'a str) -> Self {
        assert!(!open.is_empty(), "paired_span opener must not be empty");
        assert!(!close.is_empty(), "paired_span closer must not be empty");
        Self {
            open,
            close,
            escape: EscapePolicy::None,
            nesting: NestingPolicy::None,
            allow_empty: true,
            allow_line_breaks: false,
        }
    }

    /// Set backslash handling.
    ///
    /// Use this when a marker may be escaped, such as `\}}` not closing the span.
    pub fn escape(mut self, escape: EscapePolicy) -> Self {
        self.escape = escape;
        self
    }

    /// Balance nested occurrences of this pair.
    ///
    /// Use this when the same pair may nest, such as `{{a{{b}}c}}`.
    ///
    /// # Panics
    ///
    /// With [`NestingPolicy::SamePair`], panics if one marker is a prefix of the other
    /// (including equal markers), because recognition would be ambiguous.
    pub fn nesting(mut self, nesting: NestingPolicy) -> Self {
        if nesting == NestingPolicy::SamePair {
            assert!(
                !self.open.starts_with(self.close) && !self.close.starts_with(self.open),
                "nested paired_span markers must not be prefixes of each other"
            );
        }

        self.nesting = nesting;
        self
    }

    /// Allow an empty body. Whitespace-only bodies count as non-empty.
    ///
    /// Use `false` when `{{}}` should not form a span.
    pub fn allow_empty(mut self, allow: bool) -> Self {
        self.allow_empty = allow;
        self
    }

    /// Allow `\r` and `\n` anywhere in the span, including its markers.
    ///
    /// Use `true` when the body or markers may span lines. The default keeps the
    /// whole span on one line.
    pub fn allow_line_breaks(mut self, allow: bool) -> Self {
        self.allow_line_breaks = allow;
        self
    }
}

/// A matched span. Offsets are byte offsets into the input of [`scan`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairMatch {
    /// Body between the outer markers, verbatim (whitespace and escapes included).
    pub body: Range<usize>,
    /// Total bytes consumed, including the outer markers. Always nonzero.
    pub consumed: usize,
}

/// Scan a pair starting at the first byte of `src`.
///
/// Returns `None` if the opener is missing, the pair is unclosed, a line break
/// is forbidden, or an empty body is forbidden. The opening marker is matched
/// literally; if escapes should apply to it, the caller must check beforehand.
pub fn scan(src: &str, options: &PairOptions<'_>) -> Option<PairMatch> {
    let newline_forbidden = !options.allow_line_breaks;
    if !src.starts_with(options.open) || (newline_forbidden && options.open.contains(['\r', '\n']))
    {
        return None;
    }

    let escaping = options.escape == EscapePolicy::Backslash;
    let nesting = options.nesting == NestingPolicy::SamePair;
    let body_start = options.open.len();
    let mut pos = body_start;
    let mut depth = 1usize;
    let mut odd_backslashes = false;
    while let Some(ch) = src[pos..].chars().next() {
        if newline_forbidden && matches!(ch, '\r' | '\n') {
            return None;
        }

        // A character is escaped when preceded by an odd run of backslashes.
        let is_escaped = escaping && odd_backslashes;
        if !is_escaped {
            let remaining = &src[pos..];
            if remaining.starts_with(options.close) {
                if newline_forbidden && options.close.contains(['\r', '\n']) {
                    return None;
                }

                depth -= 1;
                if depth == 0 {
                    return (options.allow_empty || pos > body_start).then_some(PairMatch {
                        body: body_start..pos,
                        consumed: pos + options.close.len(),
                    });
                }
                pos += options.close.len();
                continue;
            }

            if nesting && remaining.starts_with(options.open) {
                depth += 1;
                pos += options.open.len();
                continue;
            }
        }

        odd_backslashes = escaping && ch == '\\' && !odd_backslashes;
        pos += ch.len_utf8();
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_body_and_reports_byte_offsets() {
        let source = "«🦀 雪 *x* 🦀»tail";
        let matched = scan(source, &PairOptions::new("«🦀", "🦀»")).unwrap();
        assert_eq!(&source[matched.body.clone()], " 雪 *x* ");
        assert_eq!(matched.body.start, "«🦀".len());
        assert_eq!(matched.consumed, source.len() - "tail".len());
        assert!(source.is_char_boundary(matched.body.end));
    }

    #[test]
    fn requires_an_opener_and_a_complete_closer() {
        let options = PairOptions::new("{{", "}}");
        for source in ["", "x{{a}}", "{a}}", "{{", "{{a}", "{{a"] {
            assert_eq!(scan(source, &options), None, "{source:?}");
        }
        let matched = scan("{{a}}}}", &options).unwrap();
        assert_eq!(
            matched,
            PairMatch {
                body: 2..3,
                consumed: 5
            }
        );
    }

    #[test]
    fn empty_and_whitespace_policies() {
        let options = PairOptions::new("{{", "}}");
        assert_eq!(scan("{{}}", &options).unwrap().body, 2..2);
        let options = options.allow_empty(false);
        assert_eq!(scan("{{}} later }}", &options), None);
        assert_eq!(scan("{{ }}", &options).unwrap().body, 2..3);
    }

    #[test]
    fn line_breaks_include_crlf_and_markers() {
        for newline in ["\n", "\r", "\r\n"] {
            let source = format!("{{{{a{newline}b}}}}tail");
            let options = PairOptions::new("{{", "}}");
            assert_eq!(scan(&source, &options), None);
            assert_eq!(
                scan(&source, &options.allow_line_breaks(true))
                    .unwrap()
                    .consumed,
                source.len() - 4
            );
        }
        let options = PairOptions::new("{\n", "\n}");
        assert_eq!(scan("{\nx\n}", &options), None);
        assert!(scan("{\nx\n}", &options.allow_line_breaks(true)).is_some());
        assert_eq!(
            scan("{{a}}\n", &PairOptions::new("{{", "}}"))
                .unwrap()
                .consumed,
            5
        );
    }

    #[test]
    fn backslash_parity_and_literal_preservation() {
        for count in 0..8 {
            let source = format!("{{{{雪{}}}}}tail}}}}", "\\".repeat(count));
            let options = PairOptions::new("{{", "}}").escape(EscapePolicy::Backslash);
            let matched = scan(&source, &options).unwrap();
            let end = if count % 2 == 0 {
                "{{雪".len() + count
            } else {
                source.len() - 2
            };
            assert_eq!(matched.body, 2..end, "{source:?}");
            assert_eq!(matched.consumed, end + 2);
            assert_eq!(
                scan(&source, &options.escape(EscapePolicy::None))
                    .unwrap()
                    .body
                    .end,
                "{{雪".len() + count
            );
        }
        let options = PairOptions::new("{{", "}}").escape(EscapePolicy::Backslash);
        assert_eq!(scan(r"{{x\}}", &options), None);
    }

    #[test]
    fn balances_nested_markers_and_ignores_escaped_openers() {
        let source = "{{a{{b}}c}}tail";
        let options = PairOptions::new("{{", "}}");
        assert_eq!(&source[scan(source, &options).unwrap().body], "a{{b");
        let options = options.nesting(NestingPolicy::SamePair);
        assert_eq!(&source[scan(source, &options).unwrap().body], "a{{b}}c");
        assert_eq!(scan("{{a{{b}}", &options), None);
        let source = r"{{a\{{b}}tail";
        assert_eq!(scan(source, &options), None);
        let matched = scan(source, &options.escape(EscapePolicy::Backslash)).unwrap();
        assert_eq!(&source[matched.body], r"a\{{b");
    }

    #[test]
    fn equal_markers_and_markdown_are_literal() {
        let source = "$$*x*$$tail";
        assert_eq!(
            &source[scan(source, &PairOptions::new("$$", "$$")).unwrap().body],
            "*x*"
        );
        let source = "[[`]]`]]";
        assert_eq!(
            &source[scan(source, &PairOptions::new("[[", "]]")).unwrap().body],
            "`"
        );
    }

    #[test]
    fn deeply_nested_unclosed_input_uses_no_recursion() {
        let source = "{{".repeat(100_000);
        assert_eq!(
            scan(
                &source,
                &PairOptions::new("{{", "}}").nesting(NestingPolicy::SamePair)
            ),
            None
        );
    }

    #[test]
    #[should_panic(expected = "opener must not be empty")]
    fn rejects_empty_opener() {
        PairOptions::new("", "}}");
    }

    #[test]
    #[should_panic(expected = "closer must not be empty")]
    fn rejects_empty_closer() {
        PairOptions::new("{{", "");
    }

    #[test]
    #[should_panic(expected = "must not be prefixes")]
    fn rejects_equal_nested_markers() {
        PairOptions::new("$$", "$$").nesting(NestingPolicy::SamePair);
    }

    #[test]
    #[should_panic(expected = "must not be prefixes")]
    fn rejects_nested_opener_prefix() {
        PairOptions::new("[", "[[").nesting(NestingPolicy::SamePair);
    }

    #[test]
    #[should_panic(expected = "must not be prefixes")]
    fn rejects_nested_closer_prefix() {
        PairOptions::new("[[", "[").nesting(NestingPolicy::SamePair);
    }
}
