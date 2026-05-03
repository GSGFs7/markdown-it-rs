mod container;
mod css;
mod leaf;
mod text;

use markdown_it::MarkdownIt;
use markdown_it::plugins::extra::directives::{self, DirectiveKind};

fn main() {
    let mut md = MarkdownIt::new();
    markdown_it::plugins::cmark::add(&mut md);
    directives::add(&mut md);

    // :badge
    directives::add_render(&mut md, DirectiveKind::Text, "badge", text::render_badge);

    // ::youtube
    directives::add_render(
        &mut md,
        DirectiveKind::Leaf,
        "youtube",
        leaf::render_youtube,
    );

    // :::note, :::tip...
    for name in ["note", "tip", "important", "warning", "caution"] {
        directives::add_render(
            &mut md,
            DirectiveKind::Container,
            name,
            container::render_alert,
        );
    }

    let input = r#"
# Markdown Directives Demo

## 1. Text Directives (Inline)

- Version: :badge{label="v1.2.3" type="info"}
- Status: :badge{label="Passed" type="success"}

## 2. Leaf Directives (Block)

::youtube{v="dQw4w9WgXcQ"}

## 3. Container Directives (Fenced)

Github style alerts

:::note
Useful information that users should know, even when skimming.
:::

:::tip
Helpful advice for doing something better or more easily.
:::

:::important
Key information users need to know to achieve their goal.
:::

:::warning
Urgent information that needs immediate user attention.
:::

:::caution
Advises about risks or negative outcomes of certain actions.
:::
"#;

    let body = md.parse(input).render();
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <title>Modular Directives Demo</title>
    <style>{}</style>
</head>
<body>
    <main>{}</main>
</body>
</html>"#,
        css::DEMO_CSS,
        body
    );

    let path = "examples/directives/demo.html";
    std::fs::write(path, html).expect("write file failed");
    println!("Successfully generated '{path}' using modular structure.");
}
