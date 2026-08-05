# markdown-it-rs

> [!NOTE]
> This is a personally maintained fork of
> [markdown-it-rust/markdown-it](https://github.com/markdown-it-rust/markdown-it).  
> Due to my limited skills, some features may not be as reliable as the original author's code.

Rust port of popular [markdown-it.js](https://github.com/markdown-it/markdown-it) library.

You can check a [demo](https://gsgfs7.github.io/markdown-it-rs/) in your browser *(it's Rust compiled into WASM)*.

## Features

- CommonMark test-suite coverage
- AST
- Source maps
- Easy to expand
- Python, and WebAssembly support

## Usage

```rust
let parser = &mut markdown_it::MarkdownIt::new();
markdown_it::plugins::cmark::add(parser);
markdown_it::plugins::extra::add(parser);

let ast  = parser.parse("Hello **world**!");
let html = ast.render();

print!("{html}");
// prints "<p>Hello <strong>world</strong>!</p>"
```

For a guide on how to extend it, see `examples` folder.

## Security

This lib does **not** sanitize or filter any HTML output.
You should add a sanitizer before rendering untrusted content.

There are two plugins should be careful:

- **`html`** - enable raw inline/block HTML.
  By default `plugins::cmark` does not enable raw HTML.
  Add `markdown_it::plugins::html::add(parser)` to enable it.

- **`directives`** - allows custom directives like `:name{key=value}` that are
  rendered by user provided content. The default renderers simply emit `<span>`/`<div>`
  wrappers. But it might be used like `:name{onclink=...}`.
