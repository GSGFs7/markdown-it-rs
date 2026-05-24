use markdown_it::MarkdownIt;

fn run(input: &str, output: &str) {
    let md = &mut MarkdownIt::new();
    markdown_it::plugins::cmark::add(md);
    markdown_it::plugins::extra::mark::add(md);
    markdown_it::plugins::extra::strikethrough::add(md);
    let html = md.parse(input).render();
    assert_eq!(html.trim(), output);
}

#[test]
fn mark_simple() {
    run("==highlighted==", "<p><mark>highlighted</mark></p>");
}

#[test]
fn mark_nested() {
    run(
        "==**bold** highlight==",
        "<p><mark><strong>bold</strong> highlight</mark></p>",
    );
}

#[test]
fn mark_multiple() {
    run(
        "==one== and ==two==",
        "<p><mark>one</mark> and <mark>two</mark></p>",
    );
}

#[test]
fn mark_mixed() {
    run(
        "==mark ~~strike~~==",
        "<p><mark>mark <s>strike</s></mark></p>",
    );
}
