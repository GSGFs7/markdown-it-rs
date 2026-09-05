//! URL and email detection compatible with markdown-it's `linkify-it` usage.
//!
//! This crate only detects links and returns byte ranges into the original
//! input. Markdown parsing, URL normalization, validation, and rendering stay
//! in the `markdown-it-rs` crate.

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

        let mut links = finder
            .links(input)
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

                let mut start = link.start();
                if kind == LinkKind::Email
                    && start >= "mailto:".len()
                    && input
                        .get(start - "mailto:".len()..start)
                        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("mailto:"))
                {
                    start -= "mailto:".len();
                }
                Some(Link {
                    start,
                    end: link.end(),
                    kind,
                })
            })
            .collect::<Vec<_>>();

        self.extend_explicit_urls_with_backticks(input, &finder, &mut links);
        self.add_protocol_relative_urls(input, &mut links);

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

    fn extend_explicit_urls_with_backticks(
        &self,
        input: &str,
        finder: &LinkFinder,
        links: &mut Vec<Link>,
    ) {
        if !input.contains('`') {
            return;
        }

        // compatible markdownit.js's `linkify-it`.
        //
        // "https://example.com/foo`bar`baz" -> "https://example.com/foo~bar~baz"
        // scan the replaced URL length & encode origin content.
        let scan_input = input.replace('`', "~");
        for link in finder.links(&scan_input) {
            if *link.kind() != UpstreamLinkKind::Url {
                continue;
            }

            let original = &input[link.start()..link.end()];
            if !original.contains('`') || !has_supported_explicit_scheme(original) {
                continue;
            }

            if let Some(existing) = links
                .iter_mut()
                .find(|existing| existing.start == link.start() && existing.kind == LinkKind::Url)
            {
                existing.end = existing.end.max(link.end());
            } else {
                links.push(Link {
                    start: link.start(),
                    end: link.end(),
                    kind: LinkKind::Url,
                });
            }
        }
    }

    fn add_protocol_relative_urls(&self, input: &str, links: &mut Vec<Link>) {
        let mut fuzzy_finder = LinkFinder::new();
        fuzzy_finder.url_must_have_scheme(false);

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
            let rest = &input[start + 2..];
            let Some(link) = fuzzy_finder.links(rest).next() else {
                continue;
            };
            if link.start() != 0 || *link.kind() != UpstreamLinkKind::Url {
                continue;
            }

            links.push(Link {
                start,
                end: start + 2 + link.end(),
                kind: LinkKind::Url,
            });
        }
    }
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
