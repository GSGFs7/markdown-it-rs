use markdown_it::{DocumentRenderError, MarkdownIt, Preset};

fn render(preset: Preset, source: &str) -> Result<String, DocumentRenderError> {
    let md = MarkdownIt::with_preset(preset);
    let document = md.parse_document(source);
    md.render_document_as(&document, "text")
}

#[test]
fn commonmark_projects_blocks_and_inline_labels() {
    let source = "# Heading *em*\n\nParagraph [link](url) and ![alt **strong**](img).\n\n> quote\n\n- one\n- two\n";

    assert_eq!(
        render(Preset::CommonMark, source).unwrap(),
        "Heading em\nParagraph link and alt strong.\nquote\none\ntwo\n"
    );
}

#[test]
fn commonmark_normalizes_breaks_and_preserves_code_content() {
    let source = "soft\nbreak  \nhard\n\n`inline <code>`\n\n    indented <&>\n\n```rust\nfenced <&>\n```\n\n---\n";

    assert_eq!(
        render(Preset::CommonMark, source).unwrap(),
        "soft\nbreak\nhard\ninline <code>\nindented <&>\nfenced <&>\n"
    );
}

#[test]
fn commonmark_preserves_raw_html_source() {
    let source = "before <em>raw</em> after\n\n<div>\nblock\n</div>\n\nend\n";

    assert_eq!(
        render(Preset::CommonMark, source).unwrap(),
        "before <em>raw</em> after\n<div>\nblock\n</div>\nend\n"
    );
}

#[test]
fn default_preset_projects_strikethrough_and_tables() {
    let source = "~~deleted文本~~\n\n| a | b |\n| - | - |\n| one | two |\n";

    assert_eq!(
        render(Preset::MarkdownItDefault, source).unwrap(),
        "deleted文本\na\tb\none\ttwo\n"
    );
}

#[test]
fn empty_link_and_image_are_valid_empty_containers() {
    assert_eq!(render(Preset::CommonMark, "[]() ![]()\n").unwrap(), " \n");
}
