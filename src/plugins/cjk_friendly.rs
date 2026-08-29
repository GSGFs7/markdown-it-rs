// Portions of this implementation are derived from markdown-cjk-friendly:
// https://github.com/tats-u/markdown-cjk-friendly
// Copyright (c) 2024 Tatsunori Uchino, and authors and contributors of original packages.
// Licensed under the MIT License; see THIRD_PARTY_LICENSES for the full license text.

//! CJK-friendly emphasis delimiter rules.
//!
//! CommonMark's delimiter rules can fail to recognize emphasis next to CJK
//! punctuation when no spaces are present, for example `**内容。**下一句`.
//! This plugin applies the amendments proposed by
//! <https://github.com/tats-u/markdown-cjk-friendly>.
//!
//! ```rust
//! let md = &mut markdown_it::MarkdownIt::empty();
//! markdown_it::plugins::cmark::add(md);
//! markdown_it::plugins::cjk_friendly::add(md);
//!
//! assert_eq!(
//!     md.parse("**这是重要内容。**后面继续写").render().trim(),
//!     "<p><strong>这是重要内容。</strong>后面继续写</p>"
//! );
//! ```

use unicode_general_category::{GeneralCategory, get_general_category};

use crate::common::utils::is_punct_char;
use crate::parser::inline::{DelimiterRun, InlineState, set_delimiter_scanner};
use crate::parser::main::MarkdownIt;

/// Enable CJK-friendly emphasis delimiter handling.
pub fn add(md: &mut MarkdownIt) {
    set_delimiter_scanner(md, scan_delims);
}

fn scan_delims(state: &InlineState, start: usize, can_split_word: bool) -> DelimiterRun {
    let (last_pos, last_char) = previous_char(&state.src, start).unwrap_or((0, ' '));
    let mut last_main_char = last_char;
    let mut two_previous_char = None;

    if is_non_emoji_general_use_variation_selector(last_char) {
        if let Some((_, ch)) = previous_char(&state.src, last_pos) {
            two_previous_char = Some(ch);
            if get_general_category(ch) != GeneralCategory::SpaceSeparator {
                last_main_char = ch;
            }
        }
    }

    let mut chars = state.src[start..state.pos_max].chars();
    let marker = chars.next().unwrap();
    let mut count = 1;
    let next_char = loop {
        match chars.next() {
            Some(ch) if ch == marker => count += 1,
            Some(ch) => break ch,
            None => break ' ',
        }
    };

    let is_last_whitespace = last_main_char.is_whitespace();
    let is_next_whitespace = next_char.is_whitespace();

    if is_last_whitespace || is_next_whitespace {
        return DelimiterRun {
            marker,
            can_open: !is_next_whitespace,
            can_close: !is_last_whitespace,
            length: count,
        };
    }

    let is_last_punct_char = last_main_char.is_ascii_punctuation() || is_punct_char(last_main_char);
    let is_next_punct_char = next_char.is_ascii_punctuation() || is_punct_char(next_char);

    let mut left_flanking = is_last_punct_char;
    let mut right_flanking = is_next_punct_char;

    if can_split_word {
        let adjacent_to_cjk = is_cjk(next_char)
            || match two_previous_char {
                Some(ch) => is_cjk(ch) || (last_char == '\u{fe01}' && is_quotation_mark(ch)),
                None => is_cjk(last_char) || is_ideographic_variation_selector(last_char),
            };

        left_flanking |= adjacent_to_cjk || !is_next_punct_char;
        right_flanking |= adjacent_to_cjk || !is_last_punct_char;
    }

    DelimiterRun {
        marker,
        can_open: left_flanking,
        can_close: right_flanking,
        length: count,
    }
}

fn previous_char(src: &str, pos: usize) -> Option<(usize, char)> {
    src[..pos].char_indices().next_back()
}

fn is_non_emoji_general_use_variation_selector(ch: char) -> bool {
    ('\u{fe00}'..='\u{fe0e}').contains(&ch)
}

fn is_ideographic_variation_selector(ch: char) -> bool {
    ('\u{e0100}'..='\u{e01ef}').contains(&ch)
}

fn is_quotation_mark(ch: char) -> bool {
    matches!(ch, '\u{2018}' | '\u{2019}' | '\u{201c}' | '\u{201d}')
}

// Generated from the Unicode 17 ranges published by markdown-cjk-friendly.
// Emoji_Presentation characters are intentionally absent, while CJK symbols
// that only have an optional emoji presentation remain included.
fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x1100..=0x11ff
            | 0x20a9
            | 0x2329..=0x232a
            | 0x2630..=0x2637
            | 0x268a..=0x268f
            | 0x2e80..=0x2e99
            | 0x2e9b..=0x2ef3
            | 0x2f00..=0x2fd5
            | 0x2ff0..=0x303e
            | 0x3041..=0x3096
            | 0x3099..=0x30ff
            | 0x3105..=0x312f
            | 0x3131..=0x318e
            | 0x3190..=0x31e5
            | 0x31ef..=0x321e
            | 0x3220..=0x3247
            | 0x3250..=0xa48c
            | 0xa490..=0xa4c6
            | 0xa960..=0xa97c
            | 0xac00..=0xd7a3
            | 0xd7b0..=0xd7c6
            | 0xd7cb..=0xd7fb
            | 0xf900..=0xfaff
            | 0xfe10..=0xfe19
            | 0xfe30..=0xfe52
            | 0xfe54..=0xfe66
            | 0xfe68..=0xfe6b
            | 0xff01..=0xffbe
            | 0xffc2..=0xffc7
            | 0xffca..=0xffcf
            | 0xffd2..=0xffd7
            | 0xffda..=0xffdc
            | 0xffe0..=0xffe6
            | 0xffe8..=0xffee
            | 0x16fe0..=0x16fe4
            | 0x16ff0..=0x16ff6
            | 0x17000..=0x18cd5
            | 0x18cff..=0x18d1e
            | 0x18d80..=0x18df2
            | 0x1aff0..=0x1aff3
            | 0x1aff5..=0x1affb
            | 0x1affd..=0x1affe
            | 0x1b000..=0x1b122
            | 0x1b132
            | 0x1b150..=0x1b152
            | 0x1b155
            | 0x1b164..=0x1b167
            | 0x1b170..=0x1b2fb
            | 0x1d300..=0x1d356
            | 0x1d360..=0x1d376
            | 0x1f200
            | 0x1f202
            | 0x1f210..=0x1f219
            | 0x1f21b..=0x1f22e
            | 0x1f230..=0x1f231
            | 0x1f237
            | 0x1f23b
            | 0x1f240..=0x1f248
            | 0x1f260..=0x1f265
            | 0x20000..=0x3fffd
    )
}

#[cfg(test)]
mod tests {
    use super::add;
    use crate::MarkdownIt;
    use crate::plugins::cmark;

    fn parser(cjk_friendly: bool) -> MarkdownIt {
        let mut md = MarkdownIt::empty();
        cmark::add(&mut md);
        if cjk_friendly {
            add(&mut md);
        }
        md
    }

    #[test]
    fn fixes_cjk_punctuation_next_to_strong_emphasis() {
        let source = "**这是重要内容。**后面继续写";
        assert_eq!(
            parser(false).parse(source).render(),
            "<p>**这是重要内容。**后面继续写</p>\n"
        );
        assert_eq!(
            parser(true).parse(source).render(),
            "<p><strong>这是重要内容。</strong>后面继续写</p>\n"
        );
    }

    #[test]
    fn handles_japanese_korean_and_non_bmp_cjk() {
        let md = parser(true);
        assert_eq!(
            md.parse("太郎は**「こんにちは」**といった").render(),
            "<p>太郎は<strong>「こんにちは」</strong>といった</p>\n"
        );
        assert_eq!(
            md.parse("**안녕(hello)**하세요.").render(),
            "<p><strong>안녕(hello)</strong>하세요.</p>\n"
        );
        assert_eq!(
            md.parse("𰻞𰻞**（ビャンビャン）**麺").render(),
            "<p>𰻞𰻞<strong>（ビャンビャン）</strong>麺</p>\n"
        );
    }

    #[test]
    fn handles_variation_selectors_and_pseudo_emoji() {
        let md = parser(true);
        assert_eq!(
            md.parse("正體字。︁__Hong Kong and Taiwan.__").render(),
            "<p>正體字。︁<strong>Hong Kong and Taiwan.</strong></p>\n"
        );
        assert_eq!(
            md.parse("a**🈂**a").render(),
            "<p>a<strong>🈂</strong>a</p>\n"
        );
    }

    #[test]
    fn does_not_treat_non_cjk_non_bmp_punctuation_as_cjk() {
        let md = parser(true);
        assert_eq!(md.parse("a**𐬻a**a").render(), "<p>a**𐬻a**a</p>\n");
        assert_eq!(md.parse("a**a𝜵**a").render(), "<p>a**a𝜵**a</p>\n");
    }

    #[test]
    fn preserves_commonmark_delimiter_behavior() {
        let plain = parser(false);
        let cjk = parser(true);
        let cases = [
            "foo *bar* baz",
            "foo_bar_baz",
            "foo-_(bar)_",
            "***foo***",
            "a**b**c",
            "**foo **bar baz**",
        ];

        for source in cases {
            assert_eq!(cjk.parse(source).render(), plain.parse(source).render());
        }
    }
}
