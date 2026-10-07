# markdown-it-rs playground

This preview is rendered by
[markdown-it-rs](https://github.com/GSGFs7/markdown-it-rs), a Rust-native
Markdown parser compiled to WebAssembly. Edit the text on the left and the
preview updates as you type.

The sections below showcase the CommonMark and extra syntax it supports.

---

## Inline styles

*emphasis*, **strong**, ~~strikethrough~~, ==highlight==,
`inline code`, and a [link](https://commonmark.org/).

"Smart quotes", "nested 'quotes'", ellipsis..., and (c) are handled by the
typographer. Bare URLs are linkified: <https://example.com>.

## Blockquotes

> A plain blockquote.
>
> > Nested quotes work too.

## Lists

- unordered item
- item with a nested list
  1. ordered
  2. ordered

### Tasks

- [x] completed task
- [ ] pending task

## Code

```rust {1,4}
fn main() {
    let greeting = "Hello, world!";
    println!("{greeting}");
}
```

    indented code block
    stays plain

## Tables

| Left | Center | Right |
| :--- | :----: | ----: |
| a    |   b    |     c |
| d    |   e    |     f |

## Math

Inline math like $E = mc^2$ and a block:

$$
\int_0^\infty e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}
$$

## Footnotes

Here is a footnote reference[^1] and an inline one^[Inline **markdown** is supported.].

[^1]: The footnote text, rendered at the bottom of the document.
