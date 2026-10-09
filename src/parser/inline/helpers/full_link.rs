//! Structure similar to `[link](<to> "stuff")` with configurable prefix.
//!
//! There are two structures in CommonMark that match this syntax:
//!  - links - `[text](<href> "title")`
//!  - images - `![alt](<src> "title")`
//!
//! You can add custom rules like `~[foo](<bar> "baz")`. Let us know if
//! you come up with fun use case to add as an example!
//!
//! Add a custom structure by using [add_prefix] function, which takes following arguments:
//!  - `PREFIX` - marker character before label (`!` in case of images)
//!  - `ENABLE_NESTED` - allow nested links inside
//!  - `md` - parser instance
//!  - `f` - function that should return your custom [NodeDraft] given href and title
//!
use std::collections::HashMap;

use crate::MarkdownIt;
use crate::common::utils::unescape_all;
use crate::document::NodeDraft;
use crate::parser::inline::{DocumentInlineState, InlineRule};
use crate::plugins::cmark::block::reference::ReferenceMap;

#[derive(Debug)]
struct InlineLinkTarget {
    href: Option<String>,
    title: Option<String>,
    end: usize,
}

#[derive(Debug)]
struct DocumentLinkCfg<const PREFIX: char>(fn(Option<String>, Option<String>) -> NodeDraft);

/// Register a link rule with a draft factory.
pub fn add<const ENABLE_NESTED: bool>(
    md: &mut MarkdownIt,
    factory: fn(Option<String>, Option<String>) -> NodeDraft,
) {
    md.ext.insert(DocumentLinkCfg::<'\0'>(factory));
    md.inline.add_rule::<LinkScanner<ENABLE_NESTED>>();
    if !md.inline.has_rule::<LinkScannerEnd>() {
        md.inline.add_rule::<LinkScannerEnd>();
    }
}

/// Register a prefixed link rule with a draft factory.
pub fn add_prefix<const PREFIX: char, const ENABLE_NESTED: bool>(
    md: &mut MarkdownIt,
    factory: fn(Option<String>, Option<String>) -> NodeDraft,
) {
    md.ext.insert(DocumentLinkCfg::<PREFIX>(factory));
    let builder = md
        .inline
        .add_rule::<LinkPrefixScanner<PREFIX, ENABLE_NESTED>>();
    if PREFIX == '!' {
        builder.alias_named("image");
    }
    if !md.inline.has_rule::<LinkScannerEnd>() {
        md.inline.add_rule::<LinkScannerEnd>();
    }
}

#[doc(hidden)]
pub struct LinkScanner<const ENABLE_NESTED: bool>;

impl<const ENABLE_NESTED: bool> InlineRule for LinkScanner<ENABLE_NESTED> {
    const MARKER: char = '[';
    const NAMES: &'static [&'static str] = &["link"];

    fn check(context: &mut crate::DocumentInlineState<'_>) -> Option<usize> {
        document_link_check(context, ENABLE_NESTED, 0)
    }

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
        let factory = state
            .markdown_it()
            .ext
            .get::<DocumentLinkCfg<'\0'>>()
            .expect("direct link rule requires a draft factory")
            .0;
        document_link_run(state, ENABLE_NESTED, 0, factory)
    }
}

#[doc(hidden)]
pub struct LinkPrefixScanner<const PREFIX: char, const ENABLE_NESTED: bool>;

impl<const PREFIX: char, const ENABLE_NESTED: bool> InlineRule
    for LinkPrefixScanner<PREFIX, ENABLE_NESTED>
{
    const MARKER: char = PREFIX;
    const NAMES: &'static [&'static str] = &["link_prefix"];

    fn check(context: &mut crate::DocumentInlineState<'_>) -> Option<usize> {
        if !context.remaining().starts_with(PREFIX) {
            return None;
        }

        document_link_check(context, ENABLE_NESTED, PREFIX.len_utf8())
    }

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
        if !state.remaining().starts_with(PREFIX) {
            return None;
        }

        let factory = state
            .markdown_it()
            .ext
            .get::<DocumentLinkCfg<PREFIX>>()
            .expect("direct prefix rule requires a draft factory")
            .0;
        document_link_run(state, ENABLE_NESTED, PREFIX.len_utf8(), factory)
    }
}

#[doc(hidden)]
/// this rule makes sure that parser is stopped on "]" character,
/// but it actually doesn't do anything
pub struct LinkScannerEnd;

impl InlineRule for LinkScannerEnd {
    const MARKER: char = ']';
    const NAMES: &'static [&'static str] = &["link_end"];

    fn run(_state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
        None
    }
}

// Scan a link label with independent check sessions.
//
// This function assumes that the first character ("[") already matches;
// it returns the byte length of the label including the closing "]".
fn scan_link_label(
    mut context: crate::parser::inline::DocumentInlineState<'_>,
    enable_nested: bool,
) -> Option<usize> {
    if context.depth >= context.markdown_it().max_nesting {
        return None;
    }

    let initial_len = context.remaining().len();
    let mut level = 1usize;
    while let Some(ch) = context.remaining().chars().next() {
        let before = context.remaining().len();
        if ch == ']' {
            level -= 1;
            if level == 0 {
                return Some(initial_len - before);
            }
        }

        context.skip_token()?;

        let consumed = before - context.remaining().len();
        if ch == '[' {
            if consumed == 1 {
                level += 1;
            } else if !enable_nested {
                return None;
            }
        }
    }

    None
}

pub struct ParseLinkFragmentResult {
    /// end position
    pub pos: usize,
    /// number of linebreaks inside
    pub lines: usize,
    /// parsed result
    pub str: String,
}

/// Helper function used to parse `<href>` part of the links with optional brackets.
pub fn parse_link_destination(
    str: &str,
    start: usize,
    max: usize,
) -> Option<ParseLinkFragmentResult> {
    let mut chars = str[start..max].chars().peekable();
    let mut pos = start;

    if let Some('<') = chars.peek() {
        chars.next(); // skip '<'
        pos += 1;
        loop {
            match chars.next() {
                Some('\n' | '<') | None => return None,
                Some('>') => {
                    return Some(ParseLinkFragmentResult {
                        pos: pos + 1,
                        lines: 0,
                        str: unescape_all(&str[start + 1..pos]).into_owned(),
                    });
                }
                Some('\\') => {
                    let x = chars.next()?;
                    pos += 1 + x.len_utf8();
                }
                Some(x) => {
                    pos += x.len_utf8();
                }
            }
        }
    } else {
        let mut level: u32 = 0;
        loop {
            match chars.next() {
                // space + ascii control characters
                Some('\0'..=' ' | '\x7f') | None => break,
                Some('\\') => match chars.next() {
                    Some(' ') | None => {
                        // [a](/url\ "title")
                        //          ^--- this space can't be escape in CommonMark
                        // it should be:
                        //  <a href="/url%5C" title="title">a</a>
                        //               ^^^--- backslash here
                        pos += 1;
                        break;
                    }
                    Some(x) => pos += 1 + x.len_utf8(),
                },
                Some('(') => {
                    level += 1;
                    if level > 32 {
                        return None;
                    }
                    pos += 1;
                }
                Some(')') => {
                    if level == 0 {
                        break;
                    }
                    level -= 1;
                    pos += 1;
                }
                Some(x) => {
                    pos += x.len_utf8();
                }
            }
        }

        if level != 0 {
            return None;
        }

        Some(ParseLinkFragmentResult {
            pos,
            lines: 0,
            str: unescape_all(&str[start..pos]).into_owned(),
        })
    }
}

/// Helper function used to parse `"title"` part of the links (with `'title'` or `(title)` alternative syntax).
pub fn parse_link_title(str: &str, start: usize, max: usize) -> Option<ParseLinkFragmentResult> {
    let mut chars = str[start..max].chars();
    let mut pos = start + 1;
    let mut lines = 0;

    let marker = match chars.next() {
        Some('"') => '"',
        Some('\'') => '\'',
        Some('(') => ')',
        None | Some(_) => return None,
    };

    loop {
        match chars.next() {
            Some(ch) if ch == marker => {
                return Some(ParseLinkFragmentResult {
                    pos: pos + 1,
                    lines,
                    str: unescape_all(&str[start + 1..pos]).into_owned(),
                });
            }
            Some('(') if marker == ')' => {
                return None;
            }
            Some('\n') => {
                pos += 1;
                lines += 1;
            }
            Some('\\') => {
                let x = chars.next()?;
                pos += 1 + x.len_utf8();
                if x == '\n' {
                    // [foo]: /url "
                    // hello
                    // \
                    // world
                    // "
                    //
                    // physical line break, must be recorded
                    // otherwise, some problems may occur during parsing
                    lines += 1;
                }
            }
            Some(x) => {
                pos += x.len_utf8();
            }
            None => {
                return None;
            }
        }
    }
}

struct ParseLinkResult {
    pub label_start: usize,
    pub label_end: usize,
    pub href: Option<String>,
    pub title: Option<String>,
    pub end: usize,
}

// [link](  <href>  "title"  )
//        ^^ skipping these spaces
fn skip_link_spaces(source: &str, mut pos: usize, max: usize) -> usize {
    while pos < max && matches!(source.as_bytes()[pos], b' ' | b'\t' | b'\n') {
        pos += 1;
    }
    pos
}

fn parse_inline_link_target(
    md: &MarkdownIt,
    source: &str,
    start: usize,
    max: usize,
) -> Option<InlineLinkTarget> {
    if !source[start..max].starts_with('(') {
        return None;
    }

    let mut pos = skip_link_spaces(source, start + 1, max);
    let mut href = None;
    let mut title = None;

    // [link](  <href>  "title"  )
    //          ^^^^^^ parsing link destination
    if let Some(result) = parse_link_destination(source, pos, max) {
        let candidate = md.link_formatter.normalize_link(&result.str);
        if md.link_formatter.validate_link(&candidate).is_some() {
            pos = result.pos;
            href = Some(candidate);
        }

        // [link](  <href>  "title"  )
        //                ^^ skipping these spaces
        pos = skip_link_spaces(source, pos, max);
        if let Some(result) = parse_link_title(source, pos, max) {
            title = Some(result.str);
            // [link](  <href>  "title"  )
            //                         ^^ skipping these spaces
            pos = skip_link_spaces(source, result.pos, max);
        }
    }

    if source[pos..max].starts_with(')') {
        Some(InlineLinkTarget {
            href,
            title,
            end: pos + 1,
        })
    } else {
        None
    }
}

fn resolve_reference_link(
    source: &str,
    label_start: usize,
    label_end: usize,
    reference_end: Option<usize>,
    references: &ReferenceMap,
) -> Option<ParseLinkResult> {
    let (label, end) = match reference_end {
        Some(close) => {
            // the second '[' immediately follows the first ']'.
            let explicit_label = &source[label_end + 2..close];
            let label = if explicit_label.is_empty() {
                &source[label_start..label_end]
            } else {
                explicit_label
            };
            (label, close + 1)
        }
        None => (&source[label_start..label_end], label_end + 1),
    };

    let (destination, title) = references.get(label)?;
    Some(ParseLinkResult {
        label_start,
        label_end,
        href: Some(destination.to_owned()),
        title: title.map(str::to_owned),
        end,
    })
}

fn scan_link_candidate(
    context: &crate::parser::inline::DocumentInlineState<'_>,
    pos: usize,
    enable_nested: bool,
    references: Option<&ReferenceMap>,
) -> Option<ParseLinkResult> {
    let source = context.remaining();
    if references.is_none() && !source.contains('(') {
        return None;
    }

    if !source.get(pos..)?.starts_with('[') {
        return None;
    }

    if !source[pos + 1..].contains(']') {
        return None;
    }

    let child_depth = context.depth.checked_add(1)?;
    let label_start = pos + 1;
    let label_end = label_start
        + scan_link_label(
            context.child_state(label_start..source.len(), child_depth, context.link_level)?,
            enable_nested,
        )?;

    if let Some(target) =
        parse_inline_link_target(context.markdown_it(), source, label_end + 1, source.len())
    {
        return Some(ParseLinkResult {
            label_start,
            label_end,
            href: target.href,
            title: target.title,
            end: target.end,
        });
    }

    let suffix_start = label_end + 1;
    let reference_end = if source[suffix_start..].starts_with('[') {
        let reference_start = suffix_start + 1;
        context
            .child_state(
                reference_start..source.len(),
                child_depth,
                context.link_level,
            )
            .and_then(|child| scan_link_label(child, false))
            .map(|close| reference_start + close)
    } else {
        None
    };

    resolve_reference_link(source, label_start, label_end, reference_end, references?)
}

fn document_link_check(
    context: &crate::parser::inline::DocumentInlineState<'_>,
    enable_nested: bool,
    offset: usize,
) -> Option<usize> {
    let references = context.root_ext.and_then(|ext| ext.get::<ReferenceMap>());
    match scan_link_candidate(context, offset, enable_nested, references) {
        Some(candidate) => Some(candidate.end),
        None => None,
    }
}

fn document_link_run(
    state: &mut DocumentInlineState<'_>,
    enable_nested: bool,
    offset: usize,
    factory: fn(Option<String>, Option<String>) -> NodeDraft,
) -> Option<(Option<NodeDraft>, usize)> {
    if !has_possible_link_label_close(state, offset) {
        return None;
    }

    // Parses [link](<to> "stuff") and other link forms (inline target or
    // reference), then parses the label as inline children.
    let references = state.root_ext.and_then(|ext| ext.get::<ReferenceMap>());
    let candidate = scan_link_candidate(state, offset, enable_nested, references)?;
    // We found the end of the link and know for a fact it's a valid link;
    // all that's left to do is to parse the label contents as inline children.
    let mut node = factory(candidate.href, candidate.title);
    let child_link_level = state
        .link_level
        .checked_add(1)
        .expect("inline link nesting level overflow");
    let children = state.parse_subrange_with_link_level(
        candidate.label_start..candidate.label_end,
        child_link_level,
    )?;
    node.children_mut().extend(children);
    Some((Some(node), candidate.end))
}

#[derive(Debug)]
struct LastLinkLabelClose {
    end: usize,
    last: Option<usize>,
    plain_closes: Option<HashMap<usize, usize>>,
    has_target: bool,
}

/// Index label ends directly, bypassing check sessions for plain input.
///
/// Returns `None` when the source contains syntax the index pass cannot
/// handle; label ends must then be scanned normally.
fn plain_label_closes(
    source: &str,
    end: usize,
    rules: &crate::parser::inline::DocumentRuleSet,
) -> Option<HashMap<usize, usize>> {
    let source = &source[..end];
    if !is_plain_label_source(source) || !has_only_supported_checks(source, rules) {
        return None;
    }
    Some(index_bracket_closes(source))
}

/// Whether the source is simple enough to index directly.
///
/// Only plain characters and empty `()` targets are allowed; other syntax
/// could change how a label is scanned.
fn is_plain_label_source(source: &str) -> bool {
    let bytes = source.as_bytes();
    let is_plain = |byte: u8| {
        byte.is_ascii_alphanumeric() || byte.is_ascii_whitespace() || b"[]!()".contains(&byte)
    };
    if !bytes.iter().all(|&byte| is_plain(byte)) {
        return false;
    }

    // Parentheses may only appear as an empty `()` target.
    bytes.iter().enumerate().all(|(index, &byte)| match byte {
        b'(' => bytes.get(index + 1) == Some(&b')'),
        b')' => index.checked_sub(1).and_then(|i| bytes.get(i)) == Some(&b'('),
        _ => true,
    })
}

/// Whether every check rule triggered by this source is handled by the index pass.
fn has_only_supported_checks(source: &str, rules: &crate::parser::inline::DocumentRuleSet) -> bool {
    rules.checks.iter().all(|rule| {
        !source.chars().any(|ch| rule.matches_marker(ch)) || is_index_supported(rule.type_id)
    })
}

fn is_index_supported(type_id: std::any::TypeId) -> bool {
    use std::any::TypeId;
    [
        TypeId::of::<LinkScanner<false>>(),
        TypeId::of::<LinkPrefixScanner<'!', true>>(),
        TypeId::of::<LinkScannerEnd>(),
        TypeId::of::<crate::parser::inline::builtin::TextScanner>(),
        TypeId::of::<crate::plugins::extra::footnote::FootnoteReferenceScanner>(),
    ]
    .contains(&type_id)
}

/// Map each `[` byte index to its matching `]`.
fn index_bracket_closes(source: &str) -> HashMap<usize, usize> {
    let mut open = Vec::new();
    let mut closes = HashMap::new();
    for (index, byte) in source.bytes().enumerate() {
        match byte {
            b'[' => open.push(index),
            b']' => {
                if let Some(open) = open.pop() {
                    closes.insert(open, index);
                }
            }
            _ => {}
        }
    }
    closes
}

fn has_possible_link_label_close(state: &mut DocumentInlineState<'_>, offset: usize) -> bool {
    if !state
        .remaining()
        .get(offset..)
        .is_some_and(|tail| tail.starts_with('['))
    {
        return false;
    }

    let source = state.src.as_ref();
    let end = state.pos_max;
    let cache = state.inline_ext.get_or_insert_with(|| LastLinkLabelClose {
        end,
        last: source[..end].rfind(']'),
        plain_closes: plain_label_closes(source, end, state.ruleset),
        has_target: source[..end].contains('('),
    });
    if cache.end != end {
        cache.end = end;
        cache.last = source[..end].rfind(']');
        cache.plain_closes = plain_label_closes(source, end, state.ruleset);
        cache.has_target = source[..end].contains('(');
    }

    if !cache.has_target
        && state
            .root_ext
            .and_then(|ext| ext.get::<ReferenceMap>())
            .is_none()
    {
        return false;
    }
    if let Some(closes) = &cache.plain_closes {
        return closes.contains_key(&(state.pos + offset));
    }
    cache.last.is_some_and(|last| last > state.pos + offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_target(source: &str) -> Option<InlineLinkTarget> {
        let md = MarkdownIt::empty();
        parse_inline_link_target(&md, source, 0, source.len())
    }

    #[test]
    fn parses_empty_destination() {
        let source = "()";
        let target = parse_target(source).unwrap();
        assert_eq!(target.href.as_deref(), Some(""));
        assert_eq!(target.title, None);
        assert_eq!(target.end, source.len());
    }

    #[test]
    fn parses_angle_destination_and_single_quoted_title() {
        let source = "(<a> 't')";
        let target = parse_target(source).unwrap();
        assert_eq!(target.href.as_deref(), Some("a"));
        assert_eq!(target.title.as_deref(), Some("t"));
        assert_eq!(target.end, source.len());
    }

    #[test]
    fn parses_escaped_parentheses_in_destination() {
        let source = r"(foo\)bar)";
        let target = parse_target(source).unwrap();
        assert_eq!(target.href.as_deref(), Some("foo)bar"));
        assert_eq!(target.end, source.len());

        let source = r"(foo\(bar)";
        let target = parse_target(source).unwrap();
        assert_eq!(target.href.as_deref(), Some("foo(bar"));
        assert_eq!(target.end, source.len());
    }

    #[test]
    fn rejects_unclosed_destination() {
        assert!(parse_target("(/url").is_none());
        assert!(parse_target("(<a").is_none());
    }

    #[test]
    fn rejects_unclosed_title() {
        assert!(parse_target(r#"(/url "title)"#).is_none());
        assert!(parse_target("(/url 'title)").is_none());
    }

    #[test]
    fn rejects_javascript_protocol() {
        assert!(parse_target("(javascript:alert(1))").is_none());
        assert!(parse_target("(JAVASCRIPT:alert(1))").is_none());
    }

    #[test]
    fn inline_target_respects_byte_range() {
        let md = MarkdownIt::empty();
        let source = "雪(/url \"title\")雨";
        let start = "雪".len();
        let max = source.len() - "雨".len();
        let target = parse_inline_link_target(&md, source, start, max).unwrap();
        assert_eq!(target.href.as_deref(), Some("/url"));
        assert_eq!(target.title.as_deref(), Some("title"));
        assert_eq!(target.end, max);
        assert!(parse_inline_link_target(&md, source, start, max - 1).is_none());
    }

    #[test]
    fn reference_candidate_selects_label_and_consumed_end() {
        let mut references = ReferenceMap::default();
        references.insert("a".into(), "/shortcut".into(), None);
        references.insert("b".into(), "/full".into(), Some("title".into()));

        for (source, reference_end, href, title, end) in [
            ("[a]", None, "/shortcut", None, 3),
            ("[a][]", Some(4), "/shortcut", None, 5),
            ("[a][b]", Some(5), "/full", Some("title"), 6),
            ("[a][", None, "/shortcut", None, 3),
            ("[a](/unfinished", None, "/shortcut", None, 3),
        ] {
            let result =
                resolve_reference_link(source, 1, 2, reference_end, &references).expect(source);
            assert_eq!((result.label_start, result.label_end), (1, 2));
            assert_eq!(result.href.as_deref(), Some(href), "{source}");
            assert_eq!(result.title.as_deref(), title, "{source}");
            assert_eq!(result.end, end, "{source}");
        }

        // A missing explicit reference must not fall back to the first label.
        assert!(resolve_reference_link("[a][missing]", 1, 2, Some(11), &references).is_none());
    }
}

#[cfg(test)]
mod check_label_tests {
    use super::*;
    use crate::document::Text;
    use crate::parser::inline::InlineRule;
    use crate::plugins::cmark::inline::link::Link;
    use crate::{DocumentInlineState, MarkdownIt, NodeDraft};

    // Register both brackets so the built-in text classifier stops at them.
    struct Bracket<const C: char>;
    impl<const C: char> InlineRule for Bracket<C> {
        const MARKER: char = C;
        fn check(_: &mut DocumentInlineState<'_>) -> Option<usize> {
            None
        }
        fn run(_: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
            panic!("the consumer must consume the complete test input")
        }
    }

    struct Consumer<const NESTED: bool>;
    impl<const NESTED: bool> InlineRule for Consumer<NESTED> {
        const MARKER: char = '@';
        fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
            if !state.remaining().starts_with("@[") {
                return None;
            }
            let len = state.remaining().len();
            let parent = state
                .child_state(1..len, state.depth.saturating_add(1), state.link_level)
                .unwrap();
            let result = parent
                .child_state(
                    1..parent.remaining().len(),
                    parent.depth.saturating_add(1),
                    parent.link_level,
                )
                .and_then(|child| scan_link_label(child, NESTED));
            assert_eq!(parent.remaining(), &state.remaining()[1..]);
            assert_eq!(parent.trailing_text(), "");
            assert_eq!(parent.link_level, 0);
            let content = match result {
                Some(end) => format!("end={end}"),
                None => "none".to_owned(),
            };
            Some((Some(NodeDraft::new(Text { content })), len))
        }
    }

    fn parser<const NESTED: bool>() -> MarkdownIt {
        let mut md = MarkdownIt::empty();
        crate::plugins::cmark::block::paragraph::add(&mut md);
        md.inline
            .add_rule::<crate::parser::inline::builtin::TextScanner>()
            .before_all();
        md.inline.add_rule::<Bracket<'['>>();
        md.inline.add_rule::<Bracket<']'>>();
        md.inline.add_rule::<Consumer<NESTED>>();
        crate::plugins::cmark::inline::escape::add(&mut md);
        crate::plugins::cmark::inline::backticks::add(&mut md);
        crate::plugins::cmark::inline::entity::add(&mut md);
        crate::plugins::html::html_inline::add(&mut md);
        md
    }

    fn render(md: &MarkdownIt, source: &str) -> String {
        md.render(source)
    }

    /// Label bodies shared by the raw-label and combined-candidate tests.
    ///
    /// A `]` inside a code span, escape, entity, or raw HTML must not close
    /// the label; raw nested `[` bytes still increase the bracket level.
    const LABELS: &[&str] = &[
        "",
        "abc",
        "雪[a]雨",
        "`]`",
        r"\]",
        "&#93;",
        "<i x=']'>",
        "<a>x",
    ];

    #[test]
    fn labels_respect_raw_brackets_and_opaque_spans() {
        let md = parser::<false>();
        for label in LABELS {
            let source = format!("@[{label}]");
            assert_eq!(
                render(&md, &source),
                format!("<p>end={}</p>\n", label.len()),
                "{source}"
            );
        }
        assert_eq!(render(&md, "@[a[b]"), "<p>none</p>\n");
        assert_eq!(render(&md, "@[abc"), "<p>none</p>\n");
    }

    struct DefaultCheck;
    impl InlineRule for DefaultCheck {
        const MARKER: char = '?';
        fn run(_: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
            None
        }
    }

    struct Closing;
    impl InlineRule for Closing {
        const MARKER: char = ']';
        fn check(_: &mut DocumentInlineState<'_>) -> Option<usize> {
            panic!("terminal bracket must be checked before dispatch")
        }
        fn run(_: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
            panic!("consumer owns input")
        }
    }

    #[test]
    fn terminal_close_precedes_dispatch_and_does_not_scan_suffix() {
        let mut md = parser::<false>();
        md.inline.add_rule::<Closing>();
        md.inline.add_rule::<DefaultCheck>();
        assert_eq!(render(&md, "@[x]?"), "<p>end=1</p>\n");
        assert_eq!(render(&md, "@[?]"), "<p>end=1</p>\n");
    }

    struct BracketToken;
    impl InlineRule for BracketToken {
        const MARKER: char = '[';
        fn check(context: &mut DocumentInlineState<'_>) -> Option<usize> {
            if context.remaining().starts_with("[x]") {
                // Legacy uses consumed length, even if a custom rule calls it Text.
                Some(3)
            } else {
                None
            }
        }
        fn run(_: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
            panic!("consumer owns input")
        }
    }

    #[test]
    fn nested_flag_preserves_consumption_rule() {
        let mut no = parser::<false>();
        no.inline.add_rule::<BracketToken>();
        assert_eq!(render(&no, "@[[x]]"), "<p>none</p>\n");
        let mut yes = parser::<true>();
        yes.inline.add_rule::<BracketToken>();
        assert_eq!(render(&yes, "@[[x]]"), "<p>end=3</p>\n");
    }

    #[test]
    fn recursive_entry_at_depth_limit_yields_no_label() {
        let mut md = parser::<false>();
        md.max_nesting = 2;
        assert_eq!(render(&md, "@[]"), "<p>none</p>\n");
    }

    #[test]
    fn registered_link_and_image_check_respect_remaining_depth() {
        let mut md = MarkdownIt::empty();
        crate::plugins::cmark::inline::link::add(&mut md);
        crate::plugins::cmark::inline::image::add(&mut md);
        md.max_nesting = 3;
        let rules = md.inline.document_rules();

        for source in ["[雪](/url)", "[](/url)", "![雪](/img)", "![](/img)"] {
            for depth in [1, 2] {
                let mut context =
                    DocumentInlineState::new(source, 0, source.len(), &md, &rules, depth, 0);
                let result = if source.starts_with('!') {
                    <LinkPrefixScanner<'!', true> as InlineRule>::check(&mut context)
                } else {
                    <LinkScanner<false> as InlineRule>::check(&mut context)
                };
                let expected = if depth == 1 { Some(source.len()) } else { None };
                assert_eq!(result, expected, "depth={depth}, source={source:?}");
                assert_eq!(context.remaining(), source);
                assert_eq!(context.depth, depth);
                assert_eq!(context.link_level, 0);
            }
        }
    }

    fn utf8_parser<const PREFIX: char>() -> MarkdownIt {
        let mut md = MarkdownIt::empty();
        crate::plugins::cmark::add(&mut md);
        super::add_prefix::<PREFIX, true>(&mut md, |href, title| {
            NodeDraft::new(Link {
                url: href.unwrap_or_default(),
                title,
            })
        });
        md
    }

    #[test]
    fn custom_prefix_uses_utf8_byte_length() {
        for (source, html) in [
            ("雪[文字](/url)", "<p><a href=\"/url\">文字</a></p>\n"),
            (
                "雪[文字][ref]\n\n[ref]: /url",
                "<p><a href=\"/url\">文字</a></p>\n",
            ),
            ("雪[文字]", "<p>雪[文字]</p>\n"),
        ] {
            assert_eq!(utf8_parser::<'雪'>().render(source), html, "{source}");
        }

        assert_eq!(
            utf8_parser::<'😀'>().render("😀[x](/url)"),
            "<p><a href=\"/url\">x</a></p>\n",
        );
        assert_eq!(
            utf8_parser::<'~'>().render("~[x](/url)"),
            "<p><a href=\"/url\">x</a></p>\n",
        );
    }

    #[test]
    fn label_scan_checks_unicode_prefixed_rule() {
        // Image label scanning calls the custom rule's check before its run.
        // The nested custom link contributes its text to the image alt value.
        assert_eq!(
            utf8_parser::<'雪'>().render("![雪[x](/inner)](/image)"),
            "<p><img src=\"/image\" alt=\"x\"></p>\n",
        );
    }

    #[test]
    fn candidate_combines_label_target_and_reference() {
        let md = parser::<false>();
        let ruleset = md.inline.document_rules();
        let mut references = ReferenceMap::default();
        references.insert("x".into(), "/shortcut".into(), None);
        references.insert("ref".into(), "/ref".into(), Some("t".into()));

        for (source, href, title, end) in [
            ("[x](/inline)", "/inline", None, 12),
            ("[x][ref]", "/ref", Some("t"), 8),
            ("[x][]", "/shortcut", None, 5),
            ("[x]", "/shortcut", None, 3),
            ("[x][", "/shortcut", None, 3),
            ("[x](/unfinished", "/shortcut", None, 3),
        ] {
            let context = DocumentInlineState::new(source, 0, source.len(), &md, &ruleset, 0, 0);
            let result = scan_link_candidate(&context, 0, false, Some(&references)).expect(source);
            assert_eq!((result.label_start, result.label_end), (1, 2));
            assert_eq!(result.href.as_deref(), Some(href), "{source}");
            assert_eq!(result.title.as_deref(), title, "{source}");
            assert_eq!(result.end, end, "{source}");
            assert_eq!(context.remaining(), source);
            assert_eq!(context.trailing_text(), "");
            assert_eq!(context.link_level, 0);
        }

        for source in ["[x][missing]", "[missing]", "[unclosed"] {
            let context = DocumentInlineState::new(source, 0, source.len(), &md, &ruleset, 0, 0);
            assert!(scan_link_candidate(&context, 0, false, Some(&references)).is_none());
        }
    }

    #[test]
    fn candidate_offsets_are_relative_to_current_window() {
        let md = parser::<false>();
        let ruleset = md.inline.document_rules();
        let source = "前雪[x](/url)";
        let context =
            DocumentInlineState::new(source, "前".len(), source.len(), &md, &ruleset, 0, 0);
        let result = scan_link_candidate(&context, "雪".len(), false, None).unwrap();
        assert_eq!(
            (result.label_start, result.label_end, result.end),
            (4, 5, 12)
        );
        assert_eq!(result.href.as_deref(), Some("/url"));
        assert_eq!(context.remaining(), "雪[x](/url)");
        assert!(scan_link_candidate(&context, 1, false, None).is_none());
    }

    #[test]
    fn candidate_combines_opaque_labels_with_target() {
        let md = parser::<false>();
        let ruleset = md.inline.document_rules();
        for label in LABELS {
            let source = format!("[{label}](/url)");
            let context = DocumentInlineState::new(&source, 0, source.len(), &md, &ruleset, 0, 0);
            let result =
                scan_link_candidate(&context, 0, false, None).unwrap_or_else(|| panic!("{source}"));
            // `]` bytes inside opaque spans must not shorten the label.
            assert_eq!(
                (result.label_start, result.label_end),
                (1, label.len() + 1),
                "{source}"
            );
            assert_eq!(result.href.as_deref(), Some("/url"), "{source}");
            assert_eq!(result.title, None, "{source}");
            assert_eq!(result.end, source.len(), "{source}");
        }
    }

    #[test]
    fn candidate_requires_reference_map_and_label_window() {
        let md = parser::<false>();
        let ruleset = md.inline.document_rules();

        // Inline targets parse without references, shortcut references do not.
        let source = "[x](/url)";
        let context = DocumentInlineState::new(source, 0, source.len(), &md, &ruleset, 0, 0);
        let result = scan_link_candidate(&context, 0, false, None).expect(source);
        assert_eq!(result.href.as_deref(), Some("/url"));
        assert_eq!(result.end, source.len());

        for source in ["[x]", "[x][ref]"] {
            let context = DocumentInlineState::new(source, 0, source.len(), &md, &ruleset, 0, 0);
            assert!(
                scan_link_candidate(&context, 0, false, None).is_none(),
                "{source}"
            );
        }

        // Label scanning consumes one nesting level; when the budget is spent
        // the whole candidate is rejected even though the syntax is valid.
        let mut md = parser::<false>();
        md.max_nesting = 2;
        let ruleset = md.inline.document_rules();
        let source = "[x](/url)";
        let root = DocumentInlineState::new(source, 0, source.len(), &md, &ruleset, 0, 0);
        assert!(scan_link_candidate(&root, 0, false, None).is_some());

        let nested = root
            .child_state(
                0..source.len(),
                root.depth.saturating_add(1),
                root.link_level,
            )
            .unwrap();
        assert_eq!(nested.depth, 1);
        assert_eq!(
            nested
                .child_state(
                    1..source.len(),
                    nested.depth.saturating_add(1),
                    nested.link_level
                )
                .unwrap()
                .depth,
            2
        );
        assert!(scan_link_candidate(&nested, 0, false, None).is_none());
    }

    #[test]
    fn candidate_respects_window_edges_and_parent_cursor() {
        let md = parser::<false>();
        let ruleset = md.inline.document_rules();

        // The full source parses, but the window ends before the closing
        // parenthesis; bytes after `remaining()` must stay invisible.
        let source = "[x](/url)tail";
        let close = source.find(')').unwrap();
        let truncated = DocumentInlineState::new(source, 0, close, &md, &ruleset, 0, 0);
        assert_eq!(truncated.remaining(), "[x](/url");
        assert!(scan_link_candidate(&truncated, 0, false, None).is_none());

        // Including the closing parenthesis makes the same window parse.
        let complete = DocumentInlineState::new(source, 0, close + 1, &md, &ruleset, 0, 0);
        let result = scan_link_candidate(&complete, 0, false, None).unwrap();
        assert_eq!(result.end, close + 1);

        // After the parent cursor advances, results stay relative to
        // `remaining()`, not to the start of the session.
        let source = "前雪[x](/url)";
        let mut context =
            DocumentInlineState::new(source, "前".len(), source.len(), &md, &ruleset, 0, 0);
        context.skip_token().unwrap();
        assert_eq!(context.remaining(), "[x](/url)");
        let result = scan_link_candidate(&context, 0, false, None).unwrap();
        assert_eq!(
            (result.label_start, result.label_end, result.end),
            (1, 2, 9)
        );
        assert_eq!(result.href.as_deref(), Some("/url"));
        assert_eq!(context.remaining(), "[x](/url)");
    }

    #[test]
    fn registered_link_check_does_not_require_running_the_factory() {
        let mut md = MarkdownIt::empty();
        add::<false>(&mut md, |_, _| panic!("check must not call draft factory"));
        let ruleset = md.inline.document_rules();
        let source = "[x](/url)";
        let mut context = DocumentInlineState::new(source, 0, source.len(), &md, &ruleset, 0, 0);
        let token = context.skip_token().unwrap();
        assert_eq!(token, source.len());
        assert_eq!(context.link_level, 0);
        assert!(context.skip_token().is_none());
    }

    #[test]
    fn registered_prefix_check_does_not_require_running_the_factory() {
        let mut md = MarkdownIt::empty();
        add_prefix::<'雪', true>(&mut md, |_, _| panic!("check must not call draft factory"));
        let ruleset = md.inline.document_rules();
        let source = "雪[x](/url)";
        let mut context = DocumentInlineState::new(source, 0, source.len(), &md, &ruleset, 0, 0);
        let token = context.skip_token().unwrap();
        assert_eq!(token, source.len());
        assert_eq!(context.link_level, 0);
        assert!(context.skip_token().is_none());
    }

    #[test]
    fn direct_prefix_uses_utf8_byte_offsets() {
        fn check<const PREFIX: char>() {
            let mut md = MarkdownIt::empty();
            crate::plugins::cmark::block::paragraph::add(&mut md);
            crate::plugins::cmark::inline::link::add(&mut md);
            add_prefix::<PREFIX, true>(&mut md, |href, title| {
                NodeDraft::new(Link {
                    url: href.unwrap_or_default(),
                    title,
                })
            });
            // Expected outputs captured from the pre-migration legacy parser.
            let cases = [
                (
                    format!("{PREFIX}[雪](/url)"),
                    "<p><a href=\"/url\">雪</a></p>\n".to_owned(),
                    1,
                ),
                (
                    format!("[a {PREFIX}[雪](/in)](/out)"),
                    "<p><a href=\"/out\">a <a href=\"/in\">雪</a></a></p>\n".to_owned(),
                    2,
                ),
                (
                    format!("{PREFIX}[雪](/unfinished"),
                    format!("<p>{PREFIX}[雪](/unfinished</p>\n"),
                    0,
                ),
            ];
            for (source, expected, link_count) in cases {
                let document = md.parse_document(&source);
                assert_eq!(md.render_document(&document), expected, "{source}");
                let links = document
                    .events(document.root())
                    .filter(|event| {
                        matches!(event, crate::StructuralEvent::Enter(_))
                            && event.node().is::<Link>()
                    })
                    .count();
                assert_eq!(links, link_count, "{source}");
                if link_count > 0 {
                    let start = source.find("[雪]").unwrap() + 1;
                    let text_ranges: Vec<_> = document
                        .events(document.root())
                        .filter_map(|event| {
                            let node = event.node();
                            node.cast::<crate::document::Text>()
                                .filter(|text| text.content == "雪")
                                .map(|_| node.srcmap().unwrap().get_byte_offsets())
                        })
                        .collect();
                    assert_eq!(text_ranges, [(start, start + "雪".len())], "{source}");
                }
            }
        }
        check::<'~'>();
        check::<'雪'>();
        check::<'🦀'>();
    }
}

#[cfg(test)]
#[test]
fn plain_brackets_have_a_linear_close_index() {
    let mut md = MarkdownIt::new();
    crate::plugins::html::add(&mut md);
    crate::plugins::extra::add(&mut md);
    let rules = md.inline.document_rules();
    let closes = plain_label_closes("![[]()", 6, &rules)
        .expect("plain cmark brackets support a close index");
    assert!(!closes.contains_key(&1));
    assert_eq!(closes.get(&2), Some(&3));
}
