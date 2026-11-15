# markdown-it-rs

> [!NOTE]
> This is a personally maintained fork of
> [markdown-it-rust/markdown-it](https://github.com/markdown-it-rust/markdown-it).

A Rust-native, AST-first Markdown parser with [markdown-it.js](https://github.com/markdown-it/markdown-it)-compatible rendering.

You can check a [demo](https://gsgfs7.github.io/markdown-it-rs/) in your browser.

## Features

- 100% CommonMark compatible & 100% markdown-it.js-compatible HTML rendering
- Mutable, typed AST
- Everything is a plugin
- Optional source maps for parsed nodes
- Extensible core, block, and inline rule chains

## Quick start

The `MarkdownItDefault` preset corresponds to markdown-it.js's default syntax:

```rust
use markdown_it::MarkdownIt;
use markdown_it::plugins::presets::Preset;

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

## Plugin API

You can add plugins to it using a declarative high-level API.

```rust
use markdown_it::MarkdownIt;
use markdown_it::parser::inline::helpers::paired_span::{EscapePolicy, PairOptions};
use markdown_it::plugin::PluginSpec;

#[derive(Debug)]
struct Spoiler;

fn spoiler() -> PluginSpec {
    let options = PairOptions::new("||", "||")
        .escape(EscapePolicy::Backslash)
        .allow_empty(false);

    PluginSpec::new("spoiler")
        .inline_container::<Spoiler>("spoiler")
        .marker('|') // tell the parser to pull up this plugin when encounter this
        .parse(move |cx| {
            cx.consume_part(&options)?; // find "||" pair, advance cursor, and record
            Some(Spoiler)
        })
        .render_html(|_, html| {
            let children = html.render_children();
            html.element("span").class("spoiler").raw(children);
        })
        .finish()
}

let md = MarkdownIt::builder().plugin(spoiler()).build();
let html = md.render("The answer is ||**42**||.");

assert_eq!(
    html,
    "<p>The answer is <span class=\"spoiler\"><strong>42</strong></span>.</p>\n"
);
```

See the `examples/ferris` folder for a detailed guide to the low-level rule API.

## Related projects

- [markdown-it.js](https://github.com/markdown-it/markdown-it) — the JavaScript
  reference implementation for this project, with a large ecosystem of
  existing plugins.
- [pulldown-cmark](https://github.com/pulldown-cmark/pulldown-cmark) — a very
  fast Rust CommonMark parser with a pull-style event API.
- [comrak](https://github.com/kivikakk/comrak) — a Rust port of `cmark-gfm`
  with AST access and GitHub-Flavored Markdown extensions.
- [markdown-rs](https://github.com/wooorm/markdown-rs) — a Rust CommonMark
  parser with mdast support and GFM extensions.
- [html5ever](https://github.com/servo/html5ever) — an HTML5 parser that can
  be paired with a DOM implementation for tree-based traversal and rewriting.
- [ammonia](https://github.com/rust-ammonia/ammonia) — an HTML sanitizer for
  Rust. Useful when raw HTML is enabled and output is untrusted.
- [lol-html](https://github.com/cloudflare/lol-html) — a streaming HTML parser
  and rewriter, useful for transforming rendered HTML without a full DOM.
- [syntect](https://github.com/trishume/syntect) — the syntax highlighting
  engine used by `plugins::extra::syntect`.
- [katex-rs](https://github.com/katex-rs/katex-rs) — a Rust KaTeX
  implementation used by `plugins::extra::math` with the `katex` feature.
