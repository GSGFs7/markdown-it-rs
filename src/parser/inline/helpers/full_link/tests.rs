use super::*;

fn parse_inline_link_target(
    md: &MarkdownIt,
    source: &str,
    start: usize,
    max: usize,
) -> Option<InlineLinkTarget> {
    scan_inline_link_target(md, source, start, max).0
}

fn parse_target(source: &str) -> Option<InlineLinkTarget> {
    let md = MarkdownIt::empty();
    if !source.starts_with('(') {
        return None;
    }
    let pos = skip_link_spaces(source, 1, source.len());
    parse_inline_link_target(&md, source, pos, source.len())
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
fn missing_target_content_respects_the_parse_window() {
    let md = MarkdownIt::empty();
    let source = "雪( \t\n/url)";
    let start = "雪".len();
    for max in start + 1..=source.find('/').unwrap() {
        let target_start = skip_link_spaces(source, start + 1, max);
        assert!(parse_inline_link_target(&md, source, target_start, max).is_none());
    }
    let target_start = skip_link_spaces(source, start + 1, source.len());
    let target = parse_inline_link_target(&md, source, target_start, source.len()).unwrap();
    assert_eq!(target.href.as_deref(), Some("/url"));
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
fn inline_title_requires_whitespace_after_destination() {
    for title in ["\"title\"", "'title'", "(title)"] {
        assert!(parse_target(&format!("(<url>{title})")).is_none());
        for whitespace in [" ", "\t", "\n"] {
            let source = format!("(<url>{whitespace}{title})");
            let target = parse_target(&source).unwrap();
            assert_eq!(target.href.as_deref(), Some("url"));
            assert_eq!(target.title.as_deref(), Some("title"));
            assert_eq!(target.end, source.len());
        }
    }
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
    let target_start = skip_link_spaces(source, start + 1, max);
    let target = parse_inline_link_target(&md, source, target_start, max).unwrap();
    assert_eq!(target.href.as_deref(), Some("/url"));
    assert_eq!(target.title.as_deref(), Some("title"));
    assert_eq!(target.end, max);
    assert!(parse_inline_link_target(&md, source, target_start, max - 1).is_none());
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
            resolve_reference_link(source, 1, 2, reference_end.map(|end| (4, end)), &references)
                .expect(source);
        assert_eq!((result.label_start, result.label_end), (1, 2));
        assert_eq!(result.href.as_deref(), Some(href), "{source}");
        assert_eq!(result.title.as_deref(), title, "{source}");
        assert_eq!(result.end, end, "{source}");
    }

    // A missing explicit reference must not fall back to the first label.
    assert!(resolve_reference_link("[a][missing]", 1, 2, Some((4, 11)), &references).is_none());
}
