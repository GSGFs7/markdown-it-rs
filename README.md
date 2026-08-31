# markdown-it-rs

> [!NOTE]
> This is a personally maintained fork of
> [markdown-it-rust/markdown-it](https://github.com/markdown-it-rust/markdown-it).  

> [!WARNING]
> **Status: unpublished `1.0.0`** — contains many breaking changes compared to
> the latest published version (`0.7.0`); the API is not stable yet.

A Rust-native, AST-first Markdown parser with [markdown-it.js](https://github.com/markdown-it/markdown-it)-compatible rendering.

You can check a [demo](https://gsgfs7.github.io/markdown-it-rs/) in your browser.

## Features

- 100% CommonMark compatible & 100% markdown-it.js-compatible HTML rendering
- Mutable, typed AST
- Everything is a plugin
- Source maps for parsed nodes
- Extensible core, block, and inline rule chains
- Optional CJK-friendly emphasis handling
- Optional Python and WebAssembly bindings

## Quick start

The `MarkdownItDefault` preset corresponds to markdown-it.js's default syntax:

```rust
use markdown_it::{MarkdownIt, Preset};

fn main() {
    let md = MarkdownIt::with_preset(Preset::MarkdownItDefault);
    let html = md.render("Hello **world**!");

    assert_eq!(html, "<p>Hello <strong>world</strong>!</p>\n");
}
```

Assembling your own Rust Markdown dialect:

```rust
use markdown_it::MarkdownIt;
use markdown_it::plugins::{cmark, extra};

fn main() {
    let mut md = MarkdownIt::empty();
    cmark::add(&mut md);
    extra::tables::add(&mut md);
    extra::tasklist::add(&mut md);
    extra::footnote::add(&mut md);
    // ...
}
```

## Planned plugin API

> [!IMPORTANT]
> This section is a design target for the next major version, not an API that is
> available in the current release yet.

The planned high-level API uses typed, composable builders for common plugins,
while keeping the lower-level rule and AST interfaces available for advanced
parsers. An emoji shortcode plugin with custom HTML rendering should look roughly
like this:

```rust,ignore
use markdown_it::{MarkdownIt, PluginSpec};

const EMOJIS: &[(&str, &str, &str)] = &[
    (":rocket:", "🚀", "rocket"),
    (":warning:", "⚠️", "warning"),
];

#[derive(Debug)]
struct Emoji {
    glyph: &'static str,
    label: &'static str,
}

fn emoji() -> PluginSpec {
    PluginSpec::new("emoji")
        .inline_leaf::<Emoji>("shortcode")
        .marker(':')
        .parse(|cx| {
            let &(shortcode, glyph, label) = EMOJIS
                .iter()
                .find(|(code, _, _)| cx.starts_with(code))?;

            cx.consume(shortcode);
            Some(Emoji { glyph, label })
        })
        .render_html(|emoji, html| {
            html.element("span")
                .class("emoji")
                .attr("role", "img")
                .attr("aria-label", emoji.label)
                .text(emoji.glyph);
        })
        .finish()
}

let md = MarkdownIt::builder().plugin(emoji()).build()?;
let html = md.render("Ready :rocket:");
//assert_eq!(html, r#"<p>Ready <span class="emoji" role="img" aria-label="rocket">🚀</span></p>"#)
```

The goal is to keep simple plugins around 20 lines, with automatic rollback and
HTML escaping, while retaining lower-level APIs for advanced plugins.

Until then, see the `examples/ferris` folder for a detailed guide to the current
low-level plugin API.

## CJK-friendly delimiters

The optional `cjk_friendly` plugin implements the delimiter amendments from
[`markdown-cjk-friendly`](https://github.com/tats-u/markdown-cjk-friendly), so
emphasis next to Chinese, Japanese, or Korean punctuation works without spaces:

```rust
use markdown_it::{MarkdownIt, Preset};

let mut md = MarkdownIt::with_preset(Preset::MarkdownItDefault);
markdown_it::plugins::cjk_friendly::add(&mut md);

assert_eq!(
    md.render("**这是重要内容。**后面可以继续写~~，被删除的内容~~"),
    "<p><strong>这是重要内容。</strong>后面可以继续写<s>，被删除的内容</s></p>\n"
);
```

## Security

This lib does **not** sanitize or filter any HTML output.
You should add a sanitizer before rendering untrusted content.

There are two plugins you should be careful with:

- **`html`** - enable raw inline/block HTML.
  By default `plugins::cmark` does not enable raw HTML.
  Add `markdown_it::plugins::html::add(parser)` to enable it.

- **`directives`** - allows custom directives like `:name{key=value}` that are
  rendered by user provided content. The default renderers simply emit `<span>`/`<div>`
  wrappers. But it might be used like `:name{onclick=...}`.
