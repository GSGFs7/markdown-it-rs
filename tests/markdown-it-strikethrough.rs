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
// TESTGEN: fixtures/markdown-it/strikethrough.txt
#[rustfmt::skip]
mod fixtures_markdown_it_strikethrough_txt {
use super::run;
// this part of the file is auto-generated
// don't edit it, otherwise your changes might be lost
#[test]
fn unnamed() {
    let input = r#"~~Strikeout~~"#;
    let output = r#"<p><s>Strikeout</s></p>"#;
    run(input, output);
}

#[test]
fn unnamed_1() {
    let input = r#"x ~~~~foo~~ bar~~"#;
    let output = r#"<p>x <s><s>foo</s> bar</s></p>"#;
    run(input, output);
}

#[test]
fn unnamed_2() {
    let input = r#"x ~~foo ~~bar~~~~"#;
    let output = r#"<p>x <s>foo <s>bar</s></s></p>"#;
    run(input, output);
}

#[test]
fn unnamed_3() {
    let input = r#"x ~~~~foo~~~~"#;
    let output = r#"<p>x <s><s>foo</s></s></p>"#;
    run(input, output);
}

#[test]
fn unnamed_4() {
    let input = r#"x ~~a ~~foo~~~~~~~~~~~bar~~ b~~

x ~~a ~~foo~~~~~~~~~~~~bar~~ b~~"#;
    let output = r#"<p>x <s>a <s>foo</s></s>~~~<s><s>bar</s> b</s></p>
<p>x <s>a <s>foo</s></s>~~~~<s><s>bar</s> b</s></p>"#;
    run(input, output);
}

#[test]
fn strikeouts_have_the_same_priority_as_emphases() {
    let input = r#"**~~test**~~

~~**test~~**"#;
    let output = r#"<p><strong>~~test</strong>~~</p>
<p><s>**test</s>**</p>"#;
    run(input, output);
}

#[test]
fn strikeouts_have_the_same_priority_as_emphases_with_respect_to_links() {
    let input = r#"[~~link]()~~

~~[link~~]()"#;
    let output = r#"<p><a href="">~~link</a>~~</p>
<p>~~<a href="">link~~</a></p>"#;
    run(input, output);
}

#[test]
fn strikeouts_have_the_same_priority_as_emphases_with_respect_to_backticks() {
    let input = r#"~~`code~~`

`~~code`~~"#;
    let output = r#"<p>~~<code>code~~</code></p>
<p><code>~~code</code>~~</p>"#;
    run(input, output);
}

#[test]
fn nested_strikeouts() {
    let input = r#"~~foo ~~bar~~ baz~~

~~f **o ~~o b~~ a** r~~"#;
    let output = r#"<p><s>foo <s>bar</s> baz</s></p>
<p><s>f <strong>o <s>o b</s> a</strong> r</s></p>"#;
    run(input, output);
}

#[test]
fn should_not_have_a_whitespace_between_text_and() {
    let input = r#"foo ~~ bar ~~ baz"#;
    let output = r#"<p>foo ~~ bar ~~ baz</p>"#;
    run(input, output);
}

#[test]
fn should_parse_strikethrough_within_link_tags() {
    let input = r#"[~~foo~~]()"#;
    let output = r#"<p><a href=""><s>foo</s></a></p>"#;
    run(input, output);
}

#[test]
fn newline_should_be_considered_a_whitespace() {
    let input = r#"~~test
~~

~~
test~~

~~
test
~~"#;
    let output = r#"<p>~~test
~~</p>
<p>~~
test~~</p>
<p>~~
test
~~</p>"#;
    run(input, output);
}

#[test]
fn from_commonmark_test_suite_replacing_with_our_marker() {
    let input = r#"a~~"foo"~~"#;
    let output = r#"<p>a~~“foo”~~</p>"#;
    run(input, output);
}

#[test]
fn coverage_single_tilde() {
    let input = r#"~a~"#;
    let output = r#"<p>~a~</p>"#;
    run(input, output);
}

#[test]
fn regression_test_for_742() {
    let input = r#"-~~~~;~~~~~~"#;
    let output = r#"<p>-<s><s>;</s></s>~~</p>"#;
    run(input, output);
}
// end of auto-generated module
}
///////////////////////////////////////////////////////////////////////////
