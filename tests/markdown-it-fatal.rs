mod common;

fn run(input: &str, output: &str) {
    let expected = if output.is_empty() {
        String::new()
    } else {
        output.to_owned() + "\n"
    };

    let md = common::markdown_it_fixture_parser();
    let actual = md.parse(&(input.to_owned() + "\n")).render();
    assert_eq!(actual, expected);
}

///////////////////////////////////////////////////////////////////////////
// TESTGEN: fixtures/markdown-it/fatal.txt
#[rustfmt::skip]
mod fixtures_markdown_it_fatal_txt {
use super::run;
// this part of the file is auto-generated
// don't edit it, otherwise your changes might be lost
#[test]
fn should_not_throw_exception_on_invalid_chars_in_url_not_allowed_in_path_mailformed_uri() {
    let input = r#"[foo](<&#x25;test>)"#;
    let output = r#"<p><a href="%25test">foo</a></p>"#;
    run(input, output);
}

#[test]
fn should_not_throw_exception_on_broken_utf_8_sequence_in_url_mailformed_uri() {
    let input = r#"[foo](%C3)"#;
    let output = r#"<p><a href="%C3">foo</a></p>"#;
    run(input, output);
}

#[test]
fn should_not_throw_exception_on_broken_utf_16_surrogates_sequence_in_url_mailformed_uri() {
    let input = r#"[foo](&#xD800;)"#;
    let output = r#"<p><a href="&amp;#xD800;">foo</a></p>"#;
    run(input, output);
}

#[test]
fn should_not_hang_comments_regexp() {
    let input = r#"foo <!--- xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx ->

foo <!------------------------------------------------------------------->"#;
    let output = r#"<p>foo &lt;!— xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx -&gt;</p>
<p>foo <!-------------------------------------------------------------------></p>"#;
    run(input, output);
}

#[test]
fn should_not_hang_cdata_regexp() {
    let input = r#"foo <![CDATA[ xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx ]>"#;
    let output = r#"<p>foo &lt;![CDATA[ xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx ]&gt;</p>"#;
    run(input, output);
}
// end of auto-generated module
}
///////////////////////////////////////////////////////////////////////////
