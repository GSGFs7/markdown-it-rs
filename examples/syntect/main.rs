fn main() {
    #[cfg(not(feature = "syntect"))]
    {
        eprintln!("Run this example with the `syntect` feature enabled.");
        eprintln!("such as: cargo run --example syntect --features syntect");
        return;
    }

    let mut md_inline = markdown_it::MarkdownIt::new();
    markdown_it::plugins::cmark::add(&mut md_inline);
    markdown_it::plugins::extra::syntect::add(&mut md_inline);

    let mut md_classed = markdown_it::MarkdownIt::new();
    markdown_it::plugins::cmark::add(&mut md_classed);
    markdown_it::plugins::extra::syntect::add(&mut md_classed);
    markdown_it::plugins::extra::syntect::set_to_classed(&mut md_classed);

    let input_inline = r#"
parse with inline mode:

```rust {2}
fn main() {
    println!("Ciallo world!");
}
```
"#;
    let input_classed = r#"
parse with classed mode:

```rust {2}
fn main() {
    println!("Ciallo world!");
}
```
"#;

    let highlighted_line_css = r#"
.syntect-line-highlighted {
    background-color: #fffbdd;
    border-left: 4px solid #f9c513;
    padding-left: 12px;
}
"#;

    let html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>Code highlight example</title>
  <style>
  body {{
    font-family: sans-serif;
    max-width: 800px;
    margin: 40px auto;
    padding: 20px;
  }}
  h2 {{
    margin-top: 40px;
  }}
  pre {{
    background-color: #f6f8fa;
    padding: 16px 0;
    border-radius: 6px;
    font-family: "SFMono-Regular", Consolas, "Liberation Mono", Menlo, monospace;
    font-size: 14px;
    line-height: 1.5;
    overflow: auto;
  }}
  .syntect-line {{
    display: block;
    padding: 0 16px;
  }}
  {}
  {}
  </style>
</head>
<body>
  <h2>Inline Mode</h2>
  {}
  <h2>Classed Mode</h2>
  {}
</body>
</html>
"#,
        markdown_it::plugins::extra::syntect::theme_css(&md_classed).unwrap_or_default(),
        highlighted_line_css,
        md_inline.parse(input_inline).render(),
        md_classed.parse(input_classed).render(),
    );

    let path = "examples/syntect/demo.html";
    std::fs::write(path, html).expect("write file failed");
    println!("success write to '{}'", path);
}
