# markdown-it

> [!NOTE]
> This is a personally maintained fork of
> [markdown-it-rust/markdown-it](https://github.com/markdown-it-rust/markdown-it),

Rust port of popular [markdown-it.js](https://github.com/markdown-it/markdown-it) library.

TL;DR:
 - if you want to get result *fast*, use [pulldown-cmark](https://github.com/raphlinus/pulldown-cmark)
 - if you want to render GFM exactly like github, use [comrak](https://github.com/kivikakk/comrak)
 - if you want to define your own syntax (like `@mentions`, `:emoji:`, custom html classes), use this library

You can check a [demo](https://markdown-it-rust.github.io/markdown-it/) in your browser *(it's Rust compiled into WASM)*.

### Features

 - 100% CommonMark compatibility
 - AST
 - Source maps (full support, not just on block tags like cmark)
 - Ability to write your own syntax of arbitrary complexity
   - to prove this point, CommonMark syntax itself is written as a plugin

### Usage

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

### Notes

*This is an attempt at making a language-agnostic parser. You can probably parse AsciiDoc, reStructuredText or [any other](https://github.com/mundimark/awesome-markdown-alternatives) plain text format with this without too much effort. I&nbsp;might eventually write these as proof-of-concept.*
