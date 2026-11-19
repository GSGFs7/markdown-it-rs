//! Scan boundaries of custom paired syntax such as `{{value}}` or `[[text]]`.
//!
//! [`scan`] returns UTF-8 byte offsets into its input and keeps the body verbatim.
//! It does not decode escapes or interpret Markdown — a closer inside a code span
//! still closes the span. Whether the body is kept literal or parsed as inline
//! children is up to the caller. Inline containers use the contextual scanner
//! instead, which skips matched inline constructs and never probes containers.
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
//!         cx.advance_bytes(matched.consumed).then_some(value)
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
//!         cx.consume_part(&options)?;
//!         Some(Boxed)
//!     })
//!     .html_tag("span")
//!     .finish();
//! let md = MarkdownIt::builder().plugin(plugin).build();
//! assert_eq!(md.render("[[*something*]]"), "<p><span><em>something</em></span></p>\n");
//! ```

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use crate::parser::inline::InlineState;
use crate::parser::inline::state::ScanLimitCache;

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

/// Whether `source` starts with the closer, or with the opener when nesting.
fn starts_pair_marker(source: &str, options: &PairOptions<'_>, nesting: bool) -> bool {
    source.starts_with(options.close) || (nesting && source.starts_with(options.open))
}

/// Raw closer positions. Escaped and opaque occurrences are included, so the
/// index can only overestimate the available closers.
#[derive(Debug, Default)]
pub(crate) struct PairScanCache {
    closers: HashMap<String, Rc<[usize]>>,
    line_breaks: Option<Vec<usize>>,
}

impl PairScanCache {
    fn cached_closers(&self, close: &str) -> Option<Rc<[usize]>> {
        self.closers.get(close).cloned()
    }

    fn closers(&mut self, source: &str, close: &str) -> Rc<[usize]> {
        if let Some(positions) = self.closers.get(close) {
            return Rc::clone(positions);
        }
        let mut positions = Vec::new();
        let mut start = 0;
        while let Some(offset) = source[start..].find(close) {
            let pos = start + offset;
            positions.push(pos);
            // Include overlapping occurrences, also for UTF-8 markers.
            start = pos + close.chars().next().unwrap().len_utf8();
        }
        let positions: Rc<[usize]> = positions.into();
        self.line_breaks.get_or_insert_with(|| {
            source
                .bytes()
                .enumerate()
                .filter_map(|(pos, byte)| matches!(byte, b'\r' | b'\n').then_some(pos))
                .collect()
        });
        self.closers.insert(close.to_owned(), positions.clone());
        positions
    }

    fn line_end(&self, start: usize, end: usize) -> usize {
        let Some(breaks) = &self.line_breaks else {
            return end;
        };
        let first = breaks.partition_point(|&pos| pos < start);
        breaks.get(first).map_or(end, |&pos| pos.min(end))
    }
}

/// Scan a container pair against the active inline rules without moving the
/// parent cursor. `offset` and the result are relative to the remaining source.
/// Nested container rules are suppressed; nesting follows `PairOptions`. Results
/// depend on the rule set, `max_nesting` and shared caches.
pub(crate) fn scan_inline(
    parent: &mut InlineState<'_>,
    offset: usize,
    options: &PairOptions<'_>,
) -> Option<PairMatch> {
    stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
        PairScan::new(parent, offset, options)?.run()
    })
}

struct PairScan<'a, 'o> {
    session: InlineState<'a>,
    /// Body up to the end of the parent window.
    body_source: &'a str,
    options: &'o PairOptions<'o>,
    /// Byte offset of the opening marker.
    origin: usize,
    level: usize,
    odd_backslashes: bool,
    newline_forbidden: bool,
    escaping: bool,
    nesting: bool,
    closers: Option<Rc<[usize]>>,
    closers_end: usize,
    cache: Rc<RefCell<ScanLimitCache>>,
}

impl<'a, 'o> PairScan<'a, 'o> {
    fn new(
        parent: &'a mut InlineState<'_>,
        offset: usize,
        options: &'o PairOptions<'o>,
    ) -> Option<Self> {
        let newline_forbidden = !options.allow_line_breaks;
        let source = parent.remaining().get(offset..)?;
        if !source.starts_with(options.open)
            || (newline_forbidden
                && (options.open.contains(['\r', '\n']) || options.close.contains(['\r', '\n'])))
        {
            return None;
        }

        let origin = parent.pos + offset;
        let cache = Rc::clone(&parent.scan_limit_cache);
        let closers = cache.borrow().pair_scan.cached_closers(options.close);
        let mut closers_end = 0;
        if let Some(closers) = &closers {
            let end = if newline_forbidden {
                cache
                    .borrow()
                    .pair_scan
                    .line_end(origin + options.open.len(), parent.pos_max)
            } else {
                parent.pos_max
            };
            closers_end = closers.partition_point(|&pos| pos + options.close.len() <= end);
            if closers_end == 0 || closers[closers_end - 1] < origin + options.open.len() {
                return None;
            }
        }
        let end = parent.remaining().len();
        let child = parent.child_session_at(
            offset + options.open.len()..end,
            parent.depth.saturating_add(1),
            parent.link_level,
        )?;
        let body_source = child.source;
        let mut session = child.state;
        // Re-entering container rules would recurse for every nested opener.
        session.suppress_containers_for_probe();
        Some(Self {
            session,
            body_source,
            options,
            origin,
            level: 1,
            odd_backslashes: false,
            newline_forbidden,
            escaping: options.escape == EscapePolicy::Backslash,
            nesting: options.nesting == NestingPolicy::SamePair,
            closers,
            closers_end,
            cache,
        })
    }

    fn run(mut self) -> Option<PairMatch> {
        while self.session.pos < self.session.pos_max {
            let remaining = self.session.remaining();
            let ch = remaining.chars().next()?;
            if self.newline_forbidden && matches!(ch, '\r' | '\n') {
                self.index_closers();
                return None;
            }

            let escaped = self.escaping && self.odd_backslashes;
            if !escaped {
                if remaining.starts_with(self.options.close) {
                    if self.newline_forbidden && self.options.close.contains(['\r', '\n']) {
                        return None;
                    }

                    self.level -= 1;
                    if self.level == 0 {
                        return self.matched();
                    }
                    self.session.advance_probe(self.options.close.len(), true);
                    continue;
                }

                if self.nesting && remaining.starts_with(self.options.open) {
                    self.level += 1;
                    // Each opener needs a distinct raw closer; reject runs with
                    // too few without scanning the rest of the suffix.
                    if let Some(closers) = &self.closers {
                        let first = closers[..self.closers_end]
                            .partition_point(|&pos| pos < self.session.pos);
                        if self.level > self.closers_end - first {
                            return None;
                        }
                    }
                    self.session.advance_probe(self.options.open.len(), true);
                    continue;
                }
            }

            // Keep pair markers literal after a backslash unless the escape
            // policy owns them; otherwise skip with the escape rule.
            let literal_marker_after_backslash = ch == '\\'
                && !self.escaping
                && starts_pair_marker(&remaining[1..], self.options, self.nesting);
            if escaped
                || literal_marker_after_backslash
                || self.session.depth >= self.session.markdown_it().max_nesting
            {
                self.odd_backslashes = self.escaping && ch == '\\' && !self.odd_backslashes;
                self.session.advance_probe(ch.len_utf8(), true);
                continue;
            }

            let before = self.session.pos;
            let body_offset = before - self.origin - self.options.open.len();
            self.session.probe_token_until(|text| {
                text.char_indices()
                    .skip(1)
                    .find(|&(pos, ch)| {
                        ch == '\\'
                            || (self.newline_forbidden && matches!(ch, '\r' | '\n'))
                            || starts_pair_marker(
                                &self.body_source[body_offset + pos..],
                                self.options,
                                self.nesting,
                            )
                    })
                    .map_or(text.len(), |(pos, _)| pos)
            })?;
            // Without an escape rule, PairOptions still handles marker escaping.
            self.odd_backslashes = self.escaping && ch == '\\' && self.session.pos == before + 1;
            // Opaque constructs may span lines, but the pair still forbids them.
            if self.newline_forbidden
                && self.session.src[before..self.session.pos].contains(['\r', '\n'])
            {
                self.index_closers();
                return None;
            }
        }
        self.index_closers();
        None
    }

    // Index only after a failed scan long enough to pay for the allocation; later
    // openers reuse the positions within their own window.
    fn index_closers(&self) {
        if self.closers.is_none() && self.session.pos - self.origin >= 64 {
            self.cache
                .borrow_mut()
                .pair_scan
                .closers(&self.session.src, self.options.close);
        }
    }

    fn matched(&self) -> Option<PairMatch> {
        let body = self.options.open.len()..self.session.pos - self.origin;
        (self.options.allow_empty || body.start != body.end).then_some(PairMatch {
            body,
            consumed: self.session.pos - self.origin + self.options.close.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closer_index_includes_overlaps_and_is_scoped_to_each_marker() {
        let mut cache = PairScanCache::default();
        let source = "abababa 雪雪雪";
        let first = cache.closers(source, "aba");
        assert_eq!(&*first, &[0, 2, 4]);
        assert_eq!(&*cache.closers(source, "雪雪"), &[8, 11]);
        assert!(cache.closers(source, "missing").is_empty());
        assert!(Rc::ptr_eq(&first, &cache.closers(source, "aba")));
    }

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
