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
//!  - `f` - function that should return your custom [Node] given href and title
//!
use std::collections::HashMap;

use crate::common::utils::unescape_all;
use crate::parser::inline::{InlineState, LegacyInlineRule};
use crate::parser::main::MarkdownIt;
use crate::parser::node::Node;
use crate::plugins::cmark::block::reference::ReferenceMap;

#[derive(Debug)]
struct InlineLinkTarget {
    href: Option<String>,
    title: Option<String>,
    end: usize,
}

#[derive(Debug)]
struct LinkCfg<const PREFIX: char>(fn(Option<String>, Option<String>) -> Node);
/// adds custom rule with no prefix
pub fn add<const ENABLE_NESTED: bool>(
    md: &mut MarkdownIt,
    f: fn(url: Option<String>, title: Option<String>) -> Node,
) {
    md.ext.insert(LinkCfg::<'\0'>(f));
    md.inline.add_legacy_rule::<LinkScanner<ENABLE_NESTED>>();
    if !md.inline.has_legacy_rule::<LinkScannerEnd>() {
        md.inline.add_legacy_rule::<LinkScannerEnd>();
    }
}

/// adds custom rule with given `PREFIX` character
pub fn add_prefix<const PREFIX: char, const ENABLE_NESTED: bool>(
    md: &mut MarkdownIt,
    f: fn(url: Option<String>, title: Option<String>) -> Node,
) {
    md.ext.insert(LinkCfg::<PREFIX>(f));
    let builder = md
        .inline
        .add_legacy_rule::<LinkPrefixScanner<PREFIX, ENABLE_NESTED>>();
    if PREFIX == '!' {
        builder.alias_named("image");
    }
    if !md.inline.has_legacy_rule::<LinkScannerEnd>() {
        md.inline.add_legacy_rule::<LinkScannerEnd>();
    }
}

#[doc(hidden)]
pub struct LinkScanner<const ENABLE_NESTED: bool>;
impl<const ENABLE_NESTED: bool> LegacyInlineRule for LinkScanner<ENABLE_NESTED> {
    const MARKER: char = '[';
    const NAMES: &'static [&'static str] = &["link"];

    fn check(state: &mut InlineState) -> Option<usize> {
        let mut chars = state.src[state.pos..state.pos_max].chars();
        if chars.next().unwrap() != '[' {
            return None;
        }
        rule_check(state, ENABLE_NESTED, 0)
    }

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let mut chars = state.src[state.pos..state.pos_max].chars();
        if chars.next().unwrap() != '[' {
            return None;
        }
        let f = state.md.ext.get::<LinkCfg<'\0'>>().unwrap().0;
        rule_run(state, ENABLE_NESTED, 0, f)
    }
}

#[doc(hidden)]
pub struct LinkPrefixScanner<const PREFIX: char, const ENABLE_NESTED: bool>;
impl<const PREFIX: char, const ENABLE_NESTED: bool> LegacyInlineRule
    for LinkPrefixScanner<PREFIX, ENABLE_NESTED>
{
    const MARKER: char = PREFIX;
    const NAMES: &'static [&'static str] = &["link_prefix"];

    fn check(state: &mut InlineState) -> Option<usize> {
        let mut chars = state.src[state.pos..state.pos_max].chars();
        if chars.next() != Some(PREFIX) {
            return None;
        }
        if chars.next() != Some('[') {
            return None;
        }
        rule_check(state, ENABLE_NESTED, PREFIX.len_utf8())
    }

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let mut chars = state.src[state.pos..state.pos_max].chars();
        if chars.next() != Some(PREFIX) {
            return None;
        }
        if chars.next() != Some('[') {
            return None;
        }
        let f = state.md.ext.get::<LinkCfg<PREFIX>>().unwrap().0;
        rule_run(state, ENABLE_NESTED, PREFIX.len_utf8(), f)
    }
}

#[doc(hidden)]
/// this rule makes sure that parser is stopped on "]" character,
/// but it actually doesn't do anything
pub struct LinkScannerEnd;
impl LegacyInlineRule for LinkScannerEnd {
    const MARKER: char = ']';
    const NAMES: &'static [&'static str] = &["link_end"];

    fn check(_: &mut InlineState) -> Option<usize> {
        None
    }
    fn run(_: &mut InlineState) -> Option<(Node, usize)> {
        None
    }
}

fn rule_check(state: &mut InlineState, enable_nested: bool, offset: usize) -> Option<usize> {
    if let Some(result) = parse_link(state, state.pos + offset, enable_nested) {
        Some(result.end - state.pos)
    } else {
        None
    }
}

fn rule_run(
    state: &mut InlineState,
    enable_nested: bool,
    offset: usize,
    f: fn(Option<String>, Option<String>) -> Node,
) -> Option<(Node, usize)> {
    let start = state.pos;
    let result = parse_link(state, state.pos + offset, enable_nested)?;

    //
    // We found the end of the link, and know for a fact it's a valid link;
    // so all that's left to do is to call tokenizer.
    //
    let old_node = std::mem::replace(&mut state.node, f(result.href, result.title));
    let max = state.pos_max;

    state.link_level += 1;
    state.pos = result.label_start;
    state.pos_max = result.label_end;
    state.md.inline.tokenize(state);
    state.pos = start;
    state.pos_max = max;
    state.link_level -= 1;

    let node = std::mem::replace(&mut state.node, old_node);
    Some((node, result.end - state.pos))
}

#[derive(Debug, Default)]
struct LinkLabelScanCache(HashMap<(usize, bool), Option<usize>>);

// Parse link label
//
// this function assumes that first character ("[") already matches;
// returns the end of the label
fn parse_link_label(state: &mut InlineState, start: usize, enable_nested: bool) -> Option<usize> {
    let cache = state
        .inline_ext
        .get_or_insert_default::<LinkLabelScanCache>();
    if let Some(&cached) = cache.0.get(&(start, enable_nested)) {
        return cached;
    }

    let old_pos = state.pos;
    let mut found = false;
    let mut label_end = None;
    let mut level = 1;

    state.pos = start + 1;

    while let Some(ch) = state.src[state.pos..state.pos_max].chars().next() {
        if ch == ']' {
            level -= 1;
            if level == 0 {
                found = true;
                break;
            }
        }

        let prev_pos = state.pos;
        state.md.inline.skip_token(state);
        if ch == '[' {
            if prev_pos == state.pos - 1 {
                // increase level if we find text `[`, which is not a part of any token
                level += 1;

                let cache = state
                    .inline_ext
                    .get_or_insert_default::<LinkLabelScanCache>();
                if let Some(&cached) = cache.0.get(&(prev_pos, enable_nested)) {
                    // maybe cache appeared as a result of skip_token
                    if let Some(cached_pos) = cached {
                        state.pos = cached_pos;
                    } else {
                        break;
                    }
                }
            } else if !enable_nested {
                break;
            }
        }
    }

    if found {
        label_end = Some(state.pos);
    }

    // restore old state
    state.pos = old_pos;

    let cache = state
        .inline_ext
        .get_or_insert_default::<LinkLabelScanCache>();
    cache.0.insert((start, enable_nested), label_end);

    label_end
}

// private migration helper
#[allow(dead_code)]
fn probe_link_label(
    mut context: crate::parser::inline::InlineProbeContext<'_>,
    enable_nested: bool,
) -> Option<usize> {
    if context.depth() >= context.markdown_it().max_nesting {
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

        context.next_token()?;

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

// Parses [link](<to> "stuff")
//
// this function assumes that first character ("[") already matches
//
fn parse_link(state: &mut InlineState, pos: usize, enable_nested: bool) -> Option<ParseLinkResult> {
    let label_end = parse_link_label(state, pos, enable_nested)?;
    let label_start = pos + 1;

    if let Some(target) =
        parse_inline_link_target(state.md, &state.src, label_end + 1, state.pos_max)
    {
        return Some(ParseLinkResult {
            label_start,
            label_end,
            href: target.href,
            title: target.title,
            end: target.end,
        });
    }

    //
    // Link reference
    //
    // TODO: check if I have any references?
    let mut pos = label_end + 1;
    let mut maybe_label = None;

    match state.src[pos..state.pos_max].chars().next() {
        Some('[') => {
            if let Some(x) = parse_link_label(state, pos, false) {
                maybe_label = Some(&state.src[pos + 1..x]);
                pos = x + 1;
            } else {
                pos = label_end + 1;
            }
        }
        _ => pos = label_end + 1,
    }

    let references = state.root_ext.get::<ReferenceMap>()?;

    // covers label === '' and label === undefined
    // (collapsed reference link and shortcut reference link respectively)
    let label = if matches!(maybe_label, None | Some("")) {
        &state.src[label_start..label_end]
    } else {
        maybe_label.unwrap()
    };

    let (destination, title) = references.get(label)?;

    Some(ParseLinkResult {
        label_start,
        label_end,
        href: Some(destination.to_owned()),
        title: title.map(|s| s.to_owned()),
        end: pos,
    })
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
}

#[cfg(test)]
mod probe_label_tests {
    use super::probe_link_label;
    use crate::parser::inline::{
        InlineProbeContext,
        InlineProbeKind,
        InlineProbeResult,
        InlineRule,
        Text,
    };
    use crate::plugins::cmark::inline::link::Link;
    use crate::{DocumentInlineState, MarkdownIt, Node, NodeDraft};

    // Register both brackets so the built-in text classifier stops at them.
    struct Bracket<const C: char>;
    impl<const C: char> InlineRule for Bracket<C> {
        const MARKER: char = C;
        fn probe(_: &mut InlineProbeContext<'_>) -> InlineProbeResult {
            InlineProbeResult::NoMatch
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
            let parent = state.probe_subrange(1..len).unwrap();
            let result = parent
                .probe_subrange(1..parent.remaining().len())
                .and_then(|child| probe_link_label(child, NESTED));
            assert_eq!(parent.remaining(), &state.remaining()[1..]);
            assert_eq!(parent.trailing_text(), "");
            assert_eq!(parent.link_level(), 0);
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
            .add_migrated_rule::<crate::parser::inline::builtin::TextScanner>()
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
        md.parse_document_direct(source)
            .unwrap()
            .into_legacy()
            .render()
    }

    #[test]
    fn labels_respect_raw_brackets_and_opaque_spans() {
        let md = parser::<false>();
        for source in [
            "@[]",
            "@[abc]",
            "@[雪[a]雨]",
            "@[`]`]",
            r"@[\]]",
            "@[&#93;]",
            "@[<i x=']'>]",
            "@[<a>x]",
        ] {
            let end = source.rfind(']').unwrap() - 2;
            assert_eq!(
                render(&md, source),
                format!("<p>end={end}</p>\n"),
                "{source}"
            );
        }
        assert_eq!(render(&md, "@[a[b]"), "<p>none</p>\n");
        assert_eq!(render(&md, "@[abc"), "<p>none</p>\n");
    }

    struct DefaultProbe;
    impl InlineRule for DefaultProbe {
        const MARKER: char = '?';
        fn run(_: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
            panic!("probe must not call run")
        }
    }

    struct Closing;
    impl InlineRule for Closing {
        const MARKER: char = ']';
        fn probe(_: &mut InlineProbeContext<'_>) -> InlineProbeResult {
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
        md.inline.add_rule::<DefaultProbe>();
        assert_eq!(render(&md, "@[x]?"), "<p>end=1</p>\n");
        assert_eq!(render(&md, "@[?]"), "<p>end=1</p>\n");
    }

    struct BracketToken;
    impl InlineRule for BracketToken {
        const MARKER: char = '[';
        fn probe(context: &mut InlineProbeContext<'_>) -> InlineProbeResult {
            if context.remaining().starts_with("[x]") {
                // Legacy uses consumed length, even if a custom rule calls it Text.
                InlineProbeResult::Match {
                    len: 3,
                    kind: InlineProbeKind::Text,
                }
            } else {
                InlineProbeResult::NoMatch
            }
        }
        fn run(_: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
            panic!("consumer owns input")
        }
    }

    #[test]
    fn nested_flag_preserves_legacy_consumption_rule() {
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
    fn supported_boundaries_match_legacy_helper() {
        use crate::parser::extset::{InlineRootExtSet, RootExtSet};
        use crate::parser::inline::InlineState;
        for source in [
            "@[]",
            "@[abc]",
            "@[雪[a]雨]",
            "@[`]`]",
            r"@[\]]",
            "@[&#93;]",
            "@[<i x=']'>]",
            "@[a[b]",
            "@[abc",
        ] {
            let md = parser::<false>();
            let mut root_ext = RootExtSet::new();
            let mut inline_ext = InlineRootExtSet::new();
            let mut state = InlineState::new(
                source.to_owned(),
                vec![(0, 0)],
                &md,
                &mut root_ext,
                &mut inline_ext,
                crate::Node::default(),
            );
            let expected = match super::parse_link_label(&mut state, 1, false) {
                Some(end) => format!("<p>end={}</p>\n", end - 2),
                None => "<p>none</p>\n".to_owned(),
            };
            assert_eq!(render(&md, source), expected, "{source}");
        }
    }

    fn utf8_parser<const PREFIX: char>() -> MarkdownIt {
        let mut md = MarkdownIt::empty();
        crate::plugins::cmark::add(&mut md);
        super::add_prefix::<PREFIX, true>(&mut md, |href, title| {
            Node::new(Link {
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
            assert_eq!(
                utf8_parser::<'雪'>().parse(source).render(),
                html,
                "{source}"
            );
        }

        assert_eq!(
            utf8_parser::<'😀'>().parse("😀[x](/url)").render(),
            "<p><a href=\"/url\">x</a></p>\n",
        );
        assert_eq!(
            utf8_parser::<'~'>().parse("~[x](/url)").render(),
            "<p><a href=\"/url\">x</a></p>\n",
        );
    }

    #[test]
    fn label_scan_checks_unicode_prefixed_rule() {
        // Image label scanning calls the custom rule's check before its run.
        // The nested custom link contributes its text to the image alt value.
        assert_eq!(
            utf8_parser::<'雪'>()
                .parse("![雪[x](/inner)](/image)")
                .render(),
            "<p><img src=\"/image\" alt=\"x\"></p>\n",
        );
    }
}
