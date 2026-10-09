//! URL and email detection compatible with markdown-it's `linkify-it` usage.
//!
//! This crate only detects links and returns byte ranges into the original
//! input. Markdown parsing, URL normalization, validation, and rendering stay
//! in the `markdown-it-rs` crate.

// TODO: complete rewrite this

use linkify_upstream::{LinkFinder, LinkKind as UpstreamLinkKind};
use unicode_general_category::{GeneralCategory, get_general_category};

/// The kind of an automatically detected link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    Url,
    Email,
}

/// A link detected in the original input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    start: usize,
    end: usize,
    kind: LinkKind,
}

impl Link {
    pub fn start(self) -> usize {
        self.start
    }

    pub fn end(self) -> usize {
        self.end
    }

    pub fn kind(self) -> LinkKind {
        self.kind
    }

    /// Return the matched text when `input` is the string this link came from.
    pub fn get_str(self, input: &str) -> Option<&str> {
        input.get(self.start..self.end)
    }

    /// Return the matched text.
    ///
    /// # Panics
    ///
    /// Panics when `input` is not the original string passed to [`Linkify`],
    /// or when it otherwise does not contain this link's byte range.
    pub fn as_str(self, input: &str) -> &str {
        self.get_str(input)
            .expect("input must be the original string used to create the link")
    }
}

/// Finds links using markdown-it-compatible matching rules.
#[derive(Debug, Default)]
pub struct Linkify;

impl Linkify {
    pub fn new() -> Self {
        Self
    }

    /// Return all non-overlapping links as byte ranges into `input`.
    pub fn links(&self, input: &str) -> Vec<Link> {
        self.links_with_fuzzy(input, false)
    }

    /// Return links, optionally recognizing URLs without an explicit scheme.
    pub fn links_with_fuzzy(&self, input: &str, fuzzy_links: bool) -> Vec<Link> {
        let mut finder = LinkFinder::new();
        finder.url_must_have_scheme(!fuzzy_links);
        let scan_input = mask_unicode_authorities(input);

        let mut links = finder
            .links(&scan_input)
            .filter_map(|link| {
                let kind = match *link.kind() {
                    UpstreamLinkKind::Url => LinkKind::Url,
                    UpstreamLinkKind::Email => LinkKind::Email,
                    _ => unreachable!("linkify returned an unknown link kind"),
                };
                let raw = &input[link.start()..link.end()];
                if kind == LinkKind::Url
                    && raw.contains("://")
                    && !has_supported_explicit_scheme(raw)
                {
                    return None;
                }

                let start = if kind == LinkKind::Email {
                    email_start(input, link.start())
                } else {
                    link.start()
                };
                Some(Link {
                    start,
                    end: link.end(),
                    kind,
                })
            })
            .collect::<Vec<_>>();

        self.extend_explicit_url_paths(input, &scan_input, &finder, &mut links);
        self.add_protocol_relative_urls(input, &mut links);
        self.add_emails_with_numeric_tlds(input, &mut links);

        links.sort_by_key(|link| (link.start, std::cmp::Reverse(link.end)));
        links.dedup_by(|a, b| a.start == b.start && a.end == b.end && a.kind == b.kind);
        let mut previous_end = 0;
        links.retain(|link| {
            if link.start < previous_end {
                false
            } else {
                previous_end = link.end;
                true
            }
        });
        links
    }

    fn extend_explicit_url_paths(
        &self,
        input: &str,
        scan_input: &str,
        finder: &LinkFinder,
        links: &mut Vec<Link>,
    ) {
        if !input.contains(['`', '|']) {
            return;
        }

        // Scan allowed path characters without changing byte offsets.
        // compatible markdownit.js's `linkify-it`.
        //
        // "https://example.com/foo`bar`baz" -> "https://example.com/foo~bar~baz"
        // scan the replaced URL length & encode origin content.
        let scan_input = scan_input.replace(['`', '|'], "~");
        for link in finder.links(&scan_input) {
            if *link.kind() != UpstreamLinkKind::Url {
                continue;
            }

            let original = &input[link.start()..link.end()];
            if !original.contains(['`', '|']) || !has_supported_explicit_scheme(original) {
                continue;
            }
            let end = link.start() + balanced_path_end(original);

            if let Some(existing) = links
                .iter_mut()
                .find(|existing| existing.start == link.start() && existing.kind == LinkKind::Url)
            {
                existing.end = end;
            } else {
                links.push(Link {
                    start: link.start(),
                    end,
                    kind: LinkKind::Url,
                });
            }
        }
    }

    fn add_protocol_relative_urls(&self, input: &str, links: &mut Vec<Link>) {
        let mut fuzzy_finder = LinkFinder::new();
        fuzzy_finder.url_must_have_scheme(false);
        let scan_input = input.replace(['`', '|'], "~");

        // rust `linkify` deliberately doesn't recognize protocol-relative URLs.
        // but markdwonit.js's `linkify-it` will identify it.
        for (start, _) in input.match_indices("//") {
            // https://example.com
            //      ^--- processed
            // \//example.com
            // ^--- disable auto linkify
            if !is_protocol_relative_boundary(input, start) {
                continue;
            }

            // //example.com/ ciallo
            //   ^^^^^^^^^^^^^^^^^^^--- check if this is a link
            // (it should identify "example.com/")
            let rest = &scan_input[start + 2..];
            let Some(link) = fuzzy_finder.links(rest).next() else {
                continue;
            };
            if link.start() != 0 || *link.kind() != UpstreamLinkKind::Url {
                continue;
            }

            links.push(Link {
                start,
                end: start + balanced_path_end(&input[start..start + 2 + link.end()]),
                kind: LinkKind::Url,
            });
        }
    }

    fn add_emails_with_numeric_tlds(&self, input: &str, links: &mut Vec<Link>) {
        let mut masked = None;
        for (at, _) in input.match_indices('@') {
            let rest = &input[at + 1..];
            let end = rest
                .find(|ch: char| !ch.is_alphanumeric() && !matches!(ch, '.' | '-'))
                .unwrap_or(rest.len());
            let domain = rest[..end].trim_end_matches('.');
            let Some(dot) = domain.rfind('.') else {
                continue;
            };

            let tld = &domain[dot + 1..];
            if tld.len() < 2 || domain.ends_with('-') || !tld.bytes().any(|b| b.is_ascii_digit()) {
                continue;
            }

            // Mask digits for upstream validation, keeping original byte ranges.
            let bytes = masked.get_or_insert_with(|| input.as_bytes().to_vec());
            for (offset, byte) in tld.bytes().enumerate() {
                if byte.is_ascii_digit() {
                    bytes[at + 1 + dot + 1 + offset] = b'a';
                }
            }
        }

        let Some(masked) = masked else {
            return;
        };

        let scan_input = String::from_utf8(masked).unwrap();
        let mut finder = LinkFinder::new();
        finder.kinds(&[UpstreamLinkKind::Email]);
        for link in finder.links(&scan_input) {
            links.push(Link {
                start: email_start(input, link.start()),
                end: link.end(),
                kind: LinkKind::Email,
            });
        }
    }
}

fn email_start(input: &str, start: usize) -> usize {
    if start >= "mailto:".len()
        && input
            .get(start - "mailto:".len()..start)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("mailto:"))
    {
        start - "mailto:".len()
    } else {
        start
    }
}

fn balanced_path_end(url: &str) -> usize {
    let authority = url.find("://").map_or(2, |index| index + 3);
    let Some(path) = url[authority..].find(['/', '?', '#']) else {
        return url.len();
    };
    let path = authority + path;
    let mut open: [Vec<usize>; 3] = Default::default();
    for (offset, byte) in url.as_bytes()[path..].iter().enumerate() {
        match byte {
            b'(' => open[0].push(path + offset),
            b'[' => open[1].push(path + offset),
            b'{' => open[2].push(path + offset),
            b')' => {
                open[0].pop();
            }
            b']' => {
                open[1].pop();
            }
            b'}' => {
                open[2].pop();
            }
            _ => {}
        }
    }
    open.iter().flatten().copied().min().unwrap_or(url.len())
}

fn mask_unicode_authorities(input: &str) -> std::borrow::Cow<'_, str> {
    let mut masked = None;
    for (separator, _) in input.match_indices("://") {
        let start = input[..separator]
            .rfind(|c: char| !c.is_ascii_alphanumeric() && !matches!(c, '+' | '-' | '.'))
            .map_or(0, |index| {
                index + input[index..].chars().next().unwrap().len_utf8()
            });
        if !has_supported_explicit_scheme(&input[start..]) {
            continue;
        }

        let authority = input[separator + 3..]
            .split(|ch: char| {
                ch.is_whitespace() || matches!(ch, '/' | '?' | '#' | '<' | '>' | '"' | '\'' | '`')
            })
            .next()
            .unwrap();
        // Keep credentials under upstream's existing validation. A hostname
        // ending in a hyphen must not turn into a partial Unicode URL match.
        let hostname = authority
            .split(':')
            .next()
            .unwrap()
            .trim_end_matches(['.', ',', ';', ')', ']', '}', '!']);
        if authority.contains('@') || hostname.ends_with('-') {
            continue;
        }

        for (offset, ch) in input[separator + 3..].char_indices() {
            if ch.is_whitespace() || matches!(ch, '/' | '?' | '#' | '<' | '>' | '"' | '\'' | '`') {
                break;
            }
            if !ch.is_ascii()
                && matches!(
                    get_general_category(ch),
                    GeneralCategory::UppercaseLetter
                        | GeneralCategory::LowercaseLetter
                        | GeneralCategory::TitlecaseLetter
                        | GeneralCategory::ModifierLetter
                        | GeneralCategory::OtherLetter
                        | GeneralCategory::NonspacingMark
                        | GeneralCategory::SpacingMark
                        | GeneralCategory::EnclosingMark
                )
            {
                // Upstream requires an ASCII TLD even when IRI parsing is on.
                // Use one ASCII letter per UTF-8 byte so every returned range
                // still indexes the original source. URL normalization happens
                // later, using the original Unicode hostname.
                let bytes = masked.get_or_insert_with(|| input.as_bytes().to_vec());
                let position = separator + 3 + offset;
                bytes[position..position + ch.len_utf8()].fill(b'a');
            }
        }
    }
    masked.map_or_else(
        || std::borrow::Cow::Borrowed(input),
        |bytes| std::borrow::Cow::Owned(String::from_utf8(bytes).unwrap()),
    )
}

fn is_protocol_relative_boundary(input: &str, start: usize) -> bool {
    let Some(previous) = input[..start].chars().next_back() else {
        return true;
    };

    if previous.is_ascii() {
        return previous.is_ascii_control()
            || previous.is_ascii_whitespace()
            || matches!(
                previous,
                '!' | '"'
                    | '#'
                    | '%'
                    | '&'
                    | '\''
                    | '('
                    | ')'
                    | '*'
                    | ','
                    | '-'
                    | '.'
                    | ';'
                    | '<'
                    | '>'
                    | '?'
                    | '@'
                    | '['
                    | ']'
                    | '{'
                    | '}'
            );
    }

    matches!(
        get_general_category(previous),
        GeneralCategory::ClosePunctuation
            | GeneralCategory::ConnectorPunctuation
            | GeneralCategory::Control
            | GeneralCategory::DashPunctuation
            | GeneralCategory::FinalPunctuation
            | GeneralCategory::InitialPunctuation
            | GeneralCategory::LineSeparator
            | GeneralCategory::OpenPunctuation
            | GeneralCategory::OtherPunctuation
            | GeneralCategory::ParagraphSeparator
            | GeneralCategory::SpaceSeparator
    )
}

fn has_supported_explicit_scheme(input: &str) -> bool {
    let Some((scheme, _)) = input.split_once("://") else {
        return false;
    };

    // `linkify-it` only support the 3 explicit schemes
    matches!(
        scheme.to_ascii_lowercase().as_str(),
        "http" | "https" | "ftp"
    )
}

#[cfg(test)]
mod tests {
    use super::{LinkKind, Linkify};

    fn matches(input: &str) -> Vec<(&str, LinkKind)> {
        Linkify::new()
            .links_with_fuzzy(input, true)
            .into_iter()
            .map(|link| (link.as_str(input), link.kind()))
            .collect()
    }

    #[test]
    fn finds_urls_and_emails() {
        assert_eq!(
            matches("example.org test@example.com"),
            vec![
                ("example.org", LinkKind::Url),
                ("test@example.com", LinkKind::Email),
            ]
        );
    }

    #[test]
    fn accepts_digits_in_email_tlds() {
        for input in [
            "foo+special@Bar.b9",
            "a@bar.9b",
            "a@bar.12",
            "mailto:foo@Bar.b9",
        ] {
            assert_eq!(matches(input), vec![(input, LinkKind::Email)], "{input:?}");
        }
        assert_eq!(
            matches("前文 foo@Bar.b9. next@example.com"),
            vec![
                ("foo@Bar.b9", LinkKind::Email),
                ("next@example.com", LinkKind::Email)
            ]
        );
        for input in ["a@bar.b9-", "a@bar.b9_x"] {
            assert!(matches(input).is_empty(), "{input:?}");
        }
        assert_eq!(
            matches("https://example.com/a@Bar.b9"),
            vec![("https://example.com/a@Bar.b9", LinkKind::Url)]
        );
    }

    #[test]
    fn includes_mailto_prefix_in_email_range() {
        assert_eq!(
            matches("mailto:test@example.com"),
            vec![("mailto:test@example.com", LinkKind::Email)]
        );
    }

    #[test]
    fn safely_gets_link_text() {
        let input = "https://example.com";
        let link = Linkify::new().links(input)[0];

        assert_eq!(link.get_str(input), Some(input));
        assert_eq!(link.get_str("短"), None);
    }

    #[test]
    fn handles_unicode_before_email() {
        for input in [
            "组     test@example.com",
            "é      test@example.com",
            "💥    test@example.com",
            "中文    test@example.com",
        ] {
            assert_eq!(
                matches(input),
                vec![("test@example.com", LinkKind::Email)],
                "{input:?}"
            );
        }
    }

    #[test]
    fn accepts_backticks_in_explicit_url() {
        assert_eq!(
            matches("https://example.com/foo`bar`baz"),
            vec![("https://example.com/foo`bar`baz", LinkKind::Url)]
        );
    }

    #[test]
    fn accepts_pipes_in_url_paths() {
        for input in [
            "https://example.com/a|b",
            "https://example.com/a|",
            "https://example.com/a`b|c`d",
            "https://例子.测试/a_(b)?x=1|é~&y=2é",
            "//example.com/a|b",
        ] {
            assert_eq!(matches(input), vec![(input, LinkKind::Url)], "{input:?}");
        }
        assert_eq!(
            matches("https://example.com/a|b[c"),
            vec![("https://example.com/a|b", LinkKind::Url)]
        );
        assert_eq!(
            matches("https://example.com/a|b. next@example.com"),
            vec![
                ("https://example.com/a|b", LinkKind::Url),
                ("next@example.com", LinkKind::Email),
            ]
        );
    }

    #[test]
    fn finds_unicode_tlds_with_original_byte_ranges() {
        let input = "前文 https://例子.测试/a_(b)?x=1&y=2 后文 https://example.com";
        assert_eq!(
            matches(input),
            vec![
                ("https://例子.测试/a_(b)?x=1&y=2", LinkKind::Url),
                ("https://example.com", LinkKind::Url),
            ]
        );
        for input in [
            "http://例子.测试",
            "HTTPS://例子.测试:8080/路径",
            "ftp://例子.测试/file",
            "https://例子.测试/foo`bar`baz",
        ] {
            assert_eq!(matches(input), vec![(input, LinkKind::Url)], "{input:?}");
        }
        assert!(matches("custom://例子.测试").is_empty());
        assert!(matches("https://例子.测试-").is_empty());
        assert!(matches("(https://例子.测试-)").is_empty());
    }

    #[test]
    fn finds_protocol_relative_url() {
        assert_eq!(
            matches("//example.com/path"),
            vec![("//example.com/path", LinkKind::Url)]
        );
    }

    #[test]
    fn protocol_relative_urls_do_not_overlap() {
        assert_eq!(
            matches("//example.com//other.org"),
            vec![("//example.com//other.org", LinkKind::Url)]
        );
    }

    #[test]
    fn protocol_relative_urls_require_a_linkify_it_boundary() {
        for input in [
            "x//example.com",
            "http:////example.com",
            "///example.com",
            "////example.com",
            "组//example.com",
            "💥//example.com",
        ] {
            assert!(matches(input).is_empty(), "{input:?}");
        }

        for input in [" //example.com", "。//example.com", "(//example.com)"] {
            assert_eq!(
                matches(input),
                vec![("//example.com", LinkKind::Url)],
                "{input:?}"
            );
        }
    }

    #[test]
    fn fuzzy_links_are_opt_in() {
        assert!(Linkify::new().links("example.org").is_empty());
        assert_eq!(matches("example.org"), vec![("example.org", LinkKind::Url)]);
    }

    #[test]
    fn ignores_unregistered_schemes() {
        assert!(Linkify::new().links("a://example.org").is_empty());
        assert_eq!(
            Linkify::new()
                .links("http://example.org")
                .into_iter()
                .map(|link| link.as_str("http://example.org"))
                .collect::<Vec<_>>(),
            vec!["http://example.org"]
        );
    }

    #[test]
    fn includes_mailto_after_unicode() {
        for input in [
            "组 mailto:test@example.com",
            "é MAILTO:test@example.com",
            "💥 MaIlTo:test@example.com",
        ] {
            assert_eq!(
                matches(input),
                vec![(input.split_once(' ').unwrap().1, LinkKind::Email)],
                "{input:?}"
            );
        }
    }
}
