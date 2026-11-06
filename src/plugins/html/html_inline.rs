//! HTML inline syntax from CommonMark
//!
//! <https://spec.commonmark.org/0.30/#raw-html>
use super::anchor_delta;
use super::utils::regexps::HTML_OPEN_CLOSE_TAG_RE;
use crate::MarkdownIt;
use crate::document::Node;
use crate::parser::inline::{InlineRule, InlineState};
use crate::render::{NodeRenderer, RenderContext};

pub fn add(md: &mut MarkdownIt) {
    crate::parser::inline::add_filtered_scan_hook(
        md,
        special_html_scan_hook,
        source_has_special_html,
    );
    md.inline.add_rule::<HtmlInlineScanner>();
    md.add_renderer::<HtmlInline, _>("html", HtmlInlineRenderer);
    md.add_renderer::<HtmlInline, _>("text", HtmlInlineRenderer);
}

/// Source-wide state for special HTML scanning.
#[derive(Debug, Default)]
struct HtmlSpecialScan {
    /// Set once a special HTML token lacks a closing delimiter.
    disabled: bool,
    /// Cached `<!/<?` starts, shared across nested inline sessions.
    starts: Option<Vec<usize>>,
}

/// `<!/<?` positions that can disable special HTML for the whole source.
fn special_html_starts(source: &str) -> impl Iterator<Item = usize> + '_ {
    let bytes = source.as_bytes();
    memchr::memchr_iter(b'<', bytes).filter(|&pos| matches!(bytes.get(pos + 1), Some(b'!' | b'?')))
}

fn source_has_special_html(source: &str) -> bool {
    special_html_starts(source).next().is_some()
}

fn special_html_scan_hook(state: &InlineState<'_>, start: usize, end: usize) -> bool {
    let mut shared = state.shared_ext();
    let scan = shared.get_or_insert_default::<HtmlSpecialScan>();
    if scan.disabled {
        return false;
    }
    let starts = scan
        .starts
        .get_or_insert_with(|| special_html_starts(&state.src).collect());
    let next = starts.partition_point(|&pos| pos <= start);
    starts.get(next).is_some_and(|&pos| pos < end)
}

fn special_html_disabled(state: &InlineState<'_>) -> bool {
    state
        .shared_ext()
        .get::<HtmlSpecialScan>()
        .is_some_and(|scan| scan.disabled)
}

fn disable_special_html(state: &InlineState<'_>) {
    state
        .shared_ext()
        .get_or_insert_default::<HtmlSpecialScan>()
        .disabled = true;
}

#[derive(Debug)]
pub struct HtmlInline {
    pub content: String,
}

struct HtmlInlineRenderer;

impl NodeRenderer<HtmlInline> for HtmlInlineRenderer {
    fn render(
        &self,
        _: &Node,
        value: &HtmlInline,
        _: &mut RenderContext<'_>,
        output: &mut crate::render::Writer,
    ) {
        output.write_str(&value.content);
    }
}

#[doc(hidden)]
pub struct HtmlInlineScanner;

impl InlineRule for HtmlInlineScanner {
    const MARKER: char = '<';
    const NAMES: &'static [&'static str] = &["html_inline"];

    fn check(state: &mut InlineState<'_>) -> Option<usize> {
        let matched = scan_html_inline(state)?;
        state.link_level = state
            .link_level
            .checked_add(matched.link_level_delta)
            .expect("check link level overflow");
        Some(matched.consumed)
    }

    fn run(state: &mut InlineState<'_>) -> Option<(Option<crate::document::NodeId>, usize)> {
        let matched = scan_html_inline(state)?;

        state.link_level += matched.link_level_delta;

        let (source, document) = state.remaining_and_document();

        Some((
            Some(document.create_node(HtmlInline {
                content: source[..matched.consumed].to_owned(),
            })),
            matched.consumed,
        ))
    }
}

struct HtmlInlineMatch {
    consumed: usize,
    link_level_delta: i32,
}

fn scan_html_inline(state: &mut InlineState<'_>) -> Option<HtmlInlineMatch> {
    let pos = state.pos;
    let pos_max = state.pos_max;

    // Keep the same minimum length as markdown-it: `<` plus two bytes.
    if pos + 2 >= pos_max {
        return None;
    }

    let second = state.src.as_bytes()[pos + 1];
    let end = if second == b'!' || second == b'?' {
        scan_special(state, pos)?
    } else if second == b'/' || second.is_ascii_alphabetic() {
        let capture = HTML_OPEN_CLOSE_TAG_RE
            .find(&state.src[pos..pos_max])?
            .as_str();
        pos + capture.len()
    } else {
        return None;
    };

    let capture = &state.src[pos..end];
    Some(HtmlInlineMatch {
        consumed: capture.len(),
        link_level_delta: anchor_delta(capture),
    })
}

/// Scan comments, CDATA sections, processing instructions, and declarations.
///
/// This mirrors markdown-it's `scanSpecial`: a missing terminator disables all
/// later special HTML tokens for the rest of this inline source. Short comments
/// (`<!-->` and `<!--->`) bypass that cutoff, as they do in markdown-it.
fn scan_special(state: &mut InlineState<'_>, pos: usize) -> Option<usize> {
    let pos_max = state.pos_max;
    let src = &state.src;
    let rest = &src[pos..pos_max];

    let (start, terminator) = if rest.starts_with("<!--") {
        // `<!-->` and `<!--->` are complete short comments.
        if rest.as_bytes().get(4) == Some(&b'>') {
            return Some(pos + 5);
        }
        if rest.as_bytes().get(4) == Some(&b'-') && rest.as_bytes().get(5) == Some(&b'>') {
            return Some(pos + 6);
        }
        (pos + 4, "-->")
    } else if rest.starts_with("<![CDATA[") {
        (pos + 9, "]]>")
    } else if rest.as_bytes().get(1) == Some(&b'?') {
        (pos + 2, "?>")
    } else if rest.as_bytes().get(2).is_some_and(u8::is_ascii_alphabetic) {
        (pos + 3, ">")
    } else {
        return None;
    };

    if special_html_disabled(state) {
        return None;
    }

    let Some(offset) = src[start..pos_max].find(terminator) else {
        disable_special_html(state);
        return None;
    };

    Some(start + offset + terminator.len())
}

#[cfg(test)]
mod tests {
    fn render(input: &str) -> String {
        let md = &mut crate::MarkdownIt::empty();
        crate::plugins::cmark::add(md);
        crate::plugins::html::add(md);
        md.render(input)
    }

    #[test]
    fn comment_allows_internal_double_hyphens() {
        assert_eq!(
            render("foo <!-- this is a --\ncomment - with hyphens -->"),
            "<p>foo <!-- this is a --\ncomment - with hyphens --></p>\n",
        );
    }

    #[test]
    fn supports_short_comment_forms() {
        assert_eq!(
            render("foo <!--> foo -->\n\nfoo <!---> foo -->"),
            "<p>foo <!--> foo --&gt;</p>\n<p>foo <!---> foo --&gt;</p>\n",
        );
    }

    #[test]
    fn declaration_requires_only_an_initial_ascii_letter() {
        for declaration in [
            r"<!ELEMENc\T br EMPTY>",
            "<!bar baz>",
            "<!A>",
            "<!a>",
            "<!DOCTYPEhtml>",
            "<!a\\* &amp;\nbody>",
        ] {
            assert_eq!(
                render(&format!("foo {declaration}\n")),
                format!("<p>foo {declaration}</p>\n"),
            );
        }
    }

    #[test]
    fn declaration_rejects_non_ascii_initial_letters_and_missing_closers() {
        for (input, expected) in [
            ("foo <!>", "<p>foo &lt;!&gt;</p>\n"),
            ("foo <!1>", "<p>foo &lt;!1&gt;</p>\n"),
            ("foo <! A>", "<p>foo &lt;! A&gt;</p>\n"),
            ("foo <!é>", "<p>foo &lt;!é&gt;</p>\n"),
            ("foo <!a", "<p>foo &lt;!a</p>\n"),
            ("foo <!a> tail >", "<p>foo <!a> tail &gt;</p>\n"),
        ] {
            assert_eq!(render(input), expected);
        }
    }

    #[test]
    fn comment_closer_may_overlap_internal_hyphens() {
        for comment in ["<!----->", "<!------>", "<!------------>"] {
            assert_eq!(
                render(&format!("foo {comment}")),
                format!("<p>foo {comment}</p>\n"),
            );
        }
    }

    #[test]
    fn failed_special_html_disables_later_special_html() {
        assert_eq!(render("~<?<!a>"), "<p>~&lt;?&lt;!a&gt;</p>\n",);
        assert_eq!(render("[<??><?3"), "<p>[&lt;??&gt;&lt;?3</p>\n",);
    }

    #[test]
    fn failed_link_scans_disable_special_html_with_parentheses() {
        for html in ["<!a>", "<??>", "<!--ok-->", "<![CDATA[ok]]>"] {
            for prefix in ["[", "!["] {
                for references in ["", "\n\n[r]: /url"] {
                    let input = format!("{prefix}{html}(<?l{references}");
                    let escaped = crate::common::utils::escape_html(html);
                    assert_eq!(
                        render(&input),
                        format!("<p>{prefix}{escaped}(&lt;?l</p>\n"),
                        "{input:?}",
                    );
                }
            }
        }
    }

    #[test]
    fn short_comments_bypass_disabled_special_html() {
        assert_eq!(render("a <?x <!--> y"), "<p>a &lt;?x <!--> y</p>\n",);
    }
}
