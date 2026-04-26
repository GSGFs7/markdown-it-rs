# markdown-it-rs

> [!NOTE]
> This is a personally maintained fork of
> [markdown-it-rust/markdown-it](https://github.com/markdown-it-rust/markdown-it).  
> Due to my limited skills, some features may not be as reliable as the original author's code.

Rust port of popular [markdown-it.js](https://github.com/markdown-it/markdown-it) library.

TL;DR:

- if you want to get result *fast*, use [pulldown-cmark](https://github.com/raphlinus/pulldown-cmark)
- if you want to render GFM exactly like GitHub, use [comrak](https://github.com/kivikakk/comrak)
- if you want to define your own syntax (like `@mentions`, `:emoji:`, custom html classes), use this library

You can check a [demo](https://gsgfs7.github.io/markdown-it-rs/) in your browser *(it's Rust compiled into WASM)*.

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

### Differences with original

1. python binding
    ```bash
    uv add markdown-it-rs-py
    ```
   
2. math
    ```rust
    // enable katex feature first
    // markdown-it-rs = { version = "0.6.2", features = ["katex"] }
    let mut md = markdown_it::MarkdownIt::new();
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::extra::math::add(&mut md);
    ```
   
3. frontmatter
    ```rust
    use markdown_it::parser::core::Root;
    use markdown_it::plugins::extra::front_matter::FrontMatter;

    // extract frontmatter as a string
    let mut md = markdown_it::MarkdownIt::new();
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::extra::front_matter::add(&mut md);

    let input = "---\ntitle: Hello\n---\n# Post";
    let ast = md.parse(input);
    let root = ast.cast::<Root>().unwrap();
    if let Some(fm) = root.ext.get::<FrontMatter>() {
        println!("frontmatter: {}", fm.raw);
    }
    ```
