//! URL and email detection compatible with markdown-it's `linkify-it` usage.
//!
//! This crate only detects links and returns byte ranges into the original
//! input. Markdown parsing, URL normalization, validation, and rendering stay
//! in the `markdown-it-rs` crate.

mod scanner;
mod unicode;
mod unicode_data;

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
        scanner::scan(input, fuzzy_links)
    }
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
    fn length_limit_preserves_complete_scalars_and_later_matches() {
        let input = format!("https://ab.com/{} next@example.com", "😀".repeat(6000));
        let links = Linkify::new().links(&input);
        assert_eq!(links.len(), 2);
        assert_eq!(
            links[0].as_str(&input),
            format!("https://ab.com/{}", "😀".repeat(4995))
        );
        assert_eq!(links[1].as_str(&input), "next@example.com");
        for link in links {
            assert!(input.is_char_boundary(link.start()));
            assert!(input.is_char_boundary(link.end()));
        }
    }

    #[test]
    fn large_non_link_inputs_and_deep_scopes_are_bounded() {
        assert!(
            Linkify::new()
                .links_with_fuzzy(&"a".repeat(100_000), true)
                .is_empty()
        );
        let input = format!(
            "https://a.com/{}x{}",
            "(".repeat(10_000),
            ")".repeat(10_000)
        );
        assert_eq!(
            Linkify::new().links(&input)[0].as_str(&input),
            "https://a.com/"
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
