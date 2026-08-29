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
// TESTGEN: fixtures/markdown-it/proto.txt
#[rustfmt::skip]
mod fixtures_markdown_it_proto_txt {
use super::run;
// this part of the file is auto-generated
// don't edit it, otherwise your changes might be lost
#[test]
fn unnamed() {
    let input = r#"[__proto__]

[__proto__]: blah"#;
    let output = r#"<p><a href="blah"><strong>proto</strong></a></p>"#;
    run(input, output);
}

#[test]
fn unnamed_1() {
    let input = r#"[hasOwnProperty]

[hasOwnProperty]: blah"#;
    let output = r#"<p><a href="blah">hasOwnProperty</a></p>"#;
    run(input, output);
}
// end of auto-generated module
}
///////////////////////////////////////////////////////////////////////////
