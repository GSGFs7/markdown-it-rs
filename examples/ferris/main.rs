// Runs arena-backed custom block, inline and core plugins.
mod block_rule;
mod core_rule;
mod inline_rule;

fn main() {
    let inline_md = &mut markdown_it::MarkdownIt::empty();

    markdown_it::plugins::cmark::block::paragraph::add(inline_md);

    // add the custom inline rule
    inline_rule::add(inline_md);

    let document = inline_md.parse_document("(\\/) hello world (\\/)");
    let inline_html = inline_md.render_document(&document);

    let block_md = &mut markdown_it::MarkdownIt::empty();
    markdown_it::plugins::cmark::add(block_md);
    block_rule::add(block_md);
    core_rule::add(block_md);
    let block_html = block_md.render("(\\/)-------------(\\/)");

    let html = format!("{inline_html}{block_html}");

    print!("{html}");

    assert_eq!(
        html.trim(),
        r#"
<p><span class="ferris-inline">🦀</span> hello world <span class="ferris-inline">🦀</span></p>
<div class="ferris-block"><img src="https://upload.wikimedia.org/wikipedia/commons/0/0f/Original_Ferris.svg"></div>
<footer class="ferris-counter">There is a crab lurking in this document.</footer>
    "#
        .trim()
    );
}
