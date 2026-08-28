# markdown-it-rs

> [!NOTE]
> This is a personally maintained fork of
> [markdown-it-rust/markdown-it](https://github.com/markdown-it-rust/markdown-it).  

A Rust-native, AST-first Markdown parser with [markdown-it.js](https://github.com/markdown-it/markdown-it)-compatible rendering.

You can check a [demo](https://gsgfs7.github.io/markdown-it-rs/) in your browser.

## Features

- 100% CommonMark compatible & highly markdown-it.js compatible
- Mutable, typed AST
- Source maps for parsed nodes
- Extensible core, block, and inline rule chains
- Optional Python and WebAssembly bindings

## Quick start

The `MarkdownItDefault` preset corresponds to markdown-it.js's default syntax:

```rust
use markdown_it::{MarkdownIt, Preset};

fn main() {
    let md = MarkdownIt::with_preset(Preset::MarkdownItDefault);
    let html = md.parse("Hello **world**!").render();

    assert_eq!(html, "<p>Hello <strong>world</strong>!</p>\n");
}
```

Assembling your own Rust Markdown dialect:

```rust
use markdown_it::MarkdownIt;
use markdown_it::plugins::{cmark, extra};

fn main() {
    let mut md = MarkdownIt::new();
    cmark::add(&mut md);
    extra::tables::add(&mut md);
    extra::tasklist::add(&mut md);
    extra::footnote::add(&mut md);
    // ...
}
```

## Write a plugin in a few lines

Plugins are regular Rust functions that install typed parsing or AST
transformation rules. This example replaces an emoji shortcode after
inline parsing:

```rust
use markdown_it::parser::core::CoreRule;
use markdown_it::parser::inline::Text;
use markdown_it::{MarkdownIt, Node, Preset};

struct Emoji;

impl CoreRule for Emoji {
    fn run(root: &mut Node, _: &MarkdownIt) {
        root.walk_mut(|node, _| {
            if let Some(text) = node.cast_mut::<Text>() {
                text.content = text.content.replace(":rocket:", "🚀");
            }
        });
    }
}

fn emoji_plugin(md: &mut MarkdownIt) {
    md.add_rule::<Emoji>().after_named("inline");
}

fn main() {
    let mut md = MarkdownIt::with_preset(Preset::MarkdownItDefault);
    emoji_plugin(&mut md);

    assert_eq!(
        md.parse("Ready to launch :rocket:").render(),
        "<p>Ready to launch 🚀</p>\n"
    );
}
```

See the `examples/ferris` folder for a detailed guide on how to extend it.

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
