use std::borrow::Cow;

pub const LARGE_CORPUS_BYTES: usize = 200 * 1024;
pub const PATHOLOGICAL_SMOKE_BYTES: usize = 32 * 1024;

const SMALL_REAL_WORLD: &str = include_str!("../corpus/small-real-world.md");
const COMMONMARK_SPEC: &str = include_str!("../../tests/fixtures/commonmark/spec.txt");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorpusKind {
    RealWorld,
    Synthetic,
    PathologicalSmoke,
}

#[derive(Debug)]
pub struct Corpus {
    pub name: &'static str,
    pub kind: CorpusKind,
    source: Cow<'static, str>,
}

impl Corpus {
    fn borrowed(name: &'static str, kind: CorpusKind, source: &'static str) -> Self {
        Self {
            name,
            kind,
            source: Cow::Borrowed(source),
        }
    }

    fn generated(name: &'static str, kind: CorpusKind, source: String) -> Self {
        Self {
            name,
            kind,
            source: Cow::Owned(source),
        }
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn len(&self) -> usize {
        self.source.len()
    }

    pub fn is_empty(&self) -> bool {
        self.source.is_empty()
    }
}

/// Return the stable corpus used by the regular throughput benchmarks.
///
/// Generated inputs are constructed here, before Criterion starts timing a
/// parser. Keeping them deterministic makes benchmark runs comparable without
/// checking large synthetic files into the repository.
pub fn standard() -> Vec<Corpus> {
    vec![
        Corpus::borrowed("small-real-world", CorpusKind::RealWorld, SMALL_REAL_WORLD),
        Corpus::borrowed("commonmark-spec", CorpusKind::RealWorld, COMMONMARK_SPEC),
        Corpus::generated(
            "plain-text",
            CorpusKind::Synthetic,
            repeat_to_size(
                "This paragraph contains ordinary words and sentences without markup. \
                 It represents prose that mostly exercises text scanning.\n\n",
                LARGE_CORPUS_BYTES,
            ),
        ),
        Corpus::generated(
            "marker-heavy",
            CorpusKind::Synthetic,
            repeat_to_size(
                "Text with *emphasis*, **strong**, [links](https://example.test), \
                 `code`, ![images](image.png), escapes\\* and <span>HTML</span>.\n\n",
                LARGE_CORPUS_BYTES,
            ),
        ),
        Corpus::generated(
            "unicode-heavy",
            CorpusKind::Synthetic,
            repeat_to_size(
                "中文段落包含标点、“引号”和强调 *内容*。日本語、한글、café、e\u{301}、🦀🚀。\n\n",
                LARGE_CORPUS_BYTES,
            ),
        ),
        Corpus::generated(
            "pathological-unmatched-emphasis",
            CorpusKind::PathologicalSmoke,
            repeated_fragment_to_size("a_ ", PATHOLOGICAL_SMOKE_BYTES),
        ),
        Corpus::generated(
            "pathological-unmatched-brackets",
            CorpusKind::PathologicalSmoke,
            repeated_fragment_to_size("[a", PATHOLOGICAL_SMOKE_BYTES),
        ),
        Corpus::generated(
            "pathological-delimiter-run",
            CorpusKind::PathologicalSmoke,
            repeated_fragment_to_size("*", PATHOLOGICAL_SMOKE_BYTES),
        ),
    ]
}

/// Build one input for a scaling benchmark without introducing randomness.
pub fn pathological_scaling(fragment: &str, target_bytes: usize) -> String {
    repeated_fragment_to_size(fragment, target_bytes)
}

fn repeat_to_size(pattern: &str, target_bytes: usize) -> String {
    assert!(!pattern.is_empty(), "corpus pattern must not be empty");

    let mut output = String::with_capacity(target_bytes + pattern.len());
    while output.len() < target_bytes {
        output.push_str(pattern);
    }

    let mut end = target_bytes.min(output.len());
    while !output.is_char_boundary(end) {
        end -= 1;
    }
    output.truncate(end);
    output
}

fn repeated_fragment_to_size(fragment: &str, target_bytes: usize) -> String {
    assert!(
        !fragment.is_empty(),
        "pathological fragment must not be empty"
    );

    let repeat_count = target_bytes.div_ceil(fragment.len());
    repeat_to_size(&fragment.repeat(repeat_count), target_bytes)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn corpus_names_are_unique_and_sources_are_not_empty() {
        let corpora = standard();
        let mut names = HashSet::new();

        for corpus in corpora {
            assert!(
                names.insert(corpus.name),
                "duplicate corpus: {}",
                corpus.name
            );
            assert!(!corpus.is_empty(), "empty corpus: {}", corpus.name);
        }
    }

    #[test]
    fn generated_corpora_have_the_requested_size() {
        for corpus in standard() {
            let expected = match corpus.kind {
                CorpusKind::Synthetic => Some(LARGE_CORPUS_BYTES),
                CorpusKind::PathologicalSmoke => Some(PATHOLOGICAL_SMOKE_BYTES),
                CorpusKind::RealWorld => None,
            };

            if let Some(expected) = expected {
                assert!(corpus.len() <= expected);
                assert!(expected - corpus.len() < 4, "wrong size: {}", corpus.name);
            }
        }
    }

    #[test]
    fn checked_in_real_world_corpora_remain_in_expected_size_classes() {
        let corpora = standard();
        let small = corpora
            .iter()
            .find(|corpus| corpus.name == "small-real-world")
            .unwrap();
        let spec = corpora
            .iter()
            .find(|corpus| corpus.name == "commonmark-spec")
            .unwrap();

        assert!((8 * 1024..=16 * 1024).contains(&small.len()));
        assert!((180 * 1024..=220 * 1024).contains(&spec.len()));
    }

    #[test]
    fn scaling_input_is_deterministic() {
        let first = pathological_scaling("a_ ", 70 * 1024);
        let second = pathological_scaling("a_ ", 70 * 1024);

        assert_eq!(first, second);
        assert_eq!(first.len(), 70 * 1024);
    }
}
