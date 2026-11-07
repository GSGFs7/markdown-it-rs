// Share the same plugin sources with the standalone crate and integration tests.
#[path = "src/lib.rs"]
pub mod authoring;

use authoring::{admonition, container, counted, gh_alert, label, notice, wrap};
use markdown_it::MarkdownIt;

fn main() {
    let md = MarkdownIt::builder()
        .plugin(label::label_plugin("ready 雪<&"))
        .plugin(notice::notice_plugin())
        .build();
    println!(
        "Configured leaf plugins:\n{}",
        md.render("@ok\n\n:: A notice")
    );

    let md = MarkdownIt::builder()
        .plugin(admonition::admonition_plugin())
        .build();
    println!(
        "Configured block container:\n{}",
        md.render("::: note\nbody *here*\n:::")
    );

    let mut md = MarkdownIt::new();
    markdown_it::plugins::cmark::add(&mut md);
    counted::install(&mut md);
    container::install(&mut md);
    println!(
        "Shared counter and child session:\n{}",
        md.render("@{ *雪* @ }@")
    );

    let mut md = MarkdownIt::new();
    markdown_it::plugins::cmark::add(&mut md);
    gh_alert::install(&mut md);
    let doc = md.parse("> [!WARNING]\n> Keep *calm*.\n");
    println!(
        "GitHub-style alert (HTML):\n{}",
        md.render_document(&doc, "html")
    );
    println!(
        "GitHub-style alert (text):\n{}",
        md.render_document(&doc, "text")
    );

    let mut md = MarkdownIt::new();
    markdown_it::plugins::cmark::add(&mut md);
    wrap::install(&mut md);
    let doc = md.parse("前 *重点* 尾%");
    let paragraph = doc.children(doc.root())[0];
    let wrapper = doc.children(paragraph)[0];
    println!(
        "AST wrapper: Wrapped={}, children={}\n{}",
        doc.value::<wrap::Wrapped>(wrapper).is_some(),
        doc.children(wrapper).len(),
        md.render_document(&doc, "html"),
    );
}
