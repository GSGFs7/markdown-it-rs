//! Parser presets: ready-made plugin selections, and the open trait for
//! defining your own.
//!
//! ```
//! use markdown_it::{MarkdownIt, Preset};
//!
//! let md = MarkdownIt::with_preset(Preset::CommonMark);          // built-in
//! let md = MarkdownIt::with_preset(|md: &mut MarkdownIt| {       // custom
//!     markdown_it::plugins::cmark::add(md);
//!     markdown_it::plugins::extra::tables::add(md);
//! });
//! ```

use crate::MarkdownIt;
use crate::plugins::{cmark, extra, html};

/// Impl this to build your own presets.
pub trait PresetConfig {
    /// Apply this preset to a freshly created `md`.
    fn configure(self, md: &mut MarkdownIt);
}

/// Built-in presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    /// markdown-it.js `default`: CommonMark plus tables and strikethrough.
    /// Raw HTML, linkify, typographer off; `max_nesting` 100.
    MarkdownItDefault,

    /// Strict CommonMark, including raw HTML; `max_nesting` 20.
    CommonMark,

    /// Only paragraphs and plain inline text — a base for manual rule
    /// selection; `max_nesting` 20.
    Zero,
}

impl PresetConfig for Preset {
    fn configure(self, md: &mut MarkdownIt) {
        match self {
            Preset::MarkdownItDefault => {
                cmark::add(md);
                extra::tables::add(md);
                extra::strikethrough::add(md);
                md.max_nesting = 100;
            }
            Preset::CommonMark => {
                cmark::add(md);
                html::add(md);
                md.max_nesting = 20;
                md.render_options.xhtml_out = true;
            }
            Preset::Zero => {
                // same with markdownit.js
                cmark::block::paragraph::add(md);
                md.max_nesting = 20;
            }
        }
    }
}

/// Allow one-off presets to be expressed as closures or functions.
impl<F> PresetConfig for F
where
    F: FnOnce(&mut MarkdownIt),
{
    fn configure(self, md: &mut MarkdownIt) {
        self(md);
    }
}

#[cfg(test)]
mod tests {
    use super::{Preset, PresetConfig};
    use crate::plugins::{cmark, extra};
    use crate::{MarkdownIt, RenderOptions};

    #[test]
    fn markdown_it_default_enables_only_bundled_extensions() {
        let md = MarkdownIt::with_preset(Preset::MarkdownItDefault);

        assert_eq!(
            md.parse("~~deleted~~\n\n| a |\n| - |").render(),
            "<p><s>deleted</s></p>\n<table>\n<thead>\n<tr>\n<th>a</th>\n</tr>\n</thead>\n</table>\n"
        );
        assert_eq!(
            md.parse("<em>escaped</em>").render(),
            "<p>&lt;em&gt;escaped&lt;/em&gt;</p>\n"
        );
        assert_eq!(md.max_nesting, 100);
    }

    #[test]
    fn commonmark_enables_html_but_not_markdown_it_extensions() {
        let md = MarkdownIt::with_preset(Preset::CommonMark);

        assert_eq!(
            md.parse("<em>raw</em> ~~plain~~").render(),
            "<p><em>raw</em> ~~plain~~</p>\n"
        );
        assert_eq!(md.max_nesting, 20);
        assert_eq!(md.render("---"), "<hr />\n");
    }

    #[test]
    fn zero_keeps_only_paragraphs_and_plain_text() {
        let md = MarkdownIt::with_preset(Preset::Zero);

        assert_eq!(
            md.parse("# **plain** <em>text</em>").render(),
            "<p># **plain** &lt;em&gt;text&lt;/em&gt;</p>\n"
        );
        assert_eq!(md.max_nesting, 20);
    }

    #[test]
    fn downstream_types_can_define_presets() {
        struct GfmLike;

        impl PresetConfig for GfmLike {
            fn configure(self, md: &mut MarkdownIt) {
                cmark::add(md);
                extra::tables::add(md);
                extra::strikethrough::add(md);
                extra::tasklist::add(md);
            }
        }

        let md = MarkdownIt::with_preset(GfmLike);
        let html = md.parse("- [x] done").render();
        assert!(html.contains("task-list-item-checkbox"));
    }

    #[test]
    fn closures_can_define_one_off_presets() {
        let md = MarkdownIt::with_preset(|md: &mut MarkdownIt| {
            cmark::add(md);
            extra::mark::add(md);
        });

        assert_eq!(
            md.parse("==marked==").render(),
            "<p><mark>marked</mark></p>\n"
        );
    }

    #[test]
    fn closures_can_consume_captured_preset_data() {
        let options = RenderOptions {
            breaks: true,
            lang_prefix: Some("language-".into()),
            ..RenderOptions::default()
        };

        let md = MarkdownIt::with_preset(move |md: &mut MarkdownIt| {
            cmark::add(md);
            md.render_options = options;
        });

        assert!(md.render_options.breaks);
        assert_eq!(md.render_options.lang_prefix.as_deref(), Some("language-"));
    }
}
