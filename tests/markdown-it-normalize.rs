#![cfg(feature = "linkify")]

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
// TESTGEN: fixtures/markdown-it/normalize.txt
#[rustfmt::skip]
mod fixtures_markdown_it_normalize_txt {
use super::run;
// this part of the file is auto-generated
// don't edit it, otherwise your changes might be lost
#[test]
fn encode_link_destination_decode_text_inside_it() {
    let input = r#"<http://example.com/α%CE%B2γ%CE%B4>"#;
    let output = r#"<p><a href="http://example.com/%CE%B1%CE%B2%CE%B3%CE%B4">http://example.com/αβγδ</a></p>"#;
    run(input, output);
}

#[test]
fn unnamed() {
    let input = r#"[foo](http://example.com/α%CE%B2γ%CE%B4)"#;
    let output = r#"<p><a href="http://example.com/%CE%B1%CE%B2%CE%B3%CE%B4">foo</a></p>"#;
    run(input, output);
}

#[test]
fn keep_25_as_is_because_decoding_it_may_break_urls_720() {
    let input = r#"<https://www.google.com/search?q=hello%2E%252Ehello>"#;
    let output = r#"<p><a href="https://www.google.com/search?q=hello%2E%252Ehello">https://www.google.com/search?q=hello.%252Ehello</a></p>"#;
    run(input, output);
}

#[test]
fn should_decode_punycode() {
    let input = r#"<http://xn--n3h.net/>"#;
    let output = r#"<p><a href="http://xn--n3h.net/">http://☃.net/</a></p>"#;
    run(input, output);
}

#[test]
fn unnamed_1() {
    let input = r#"<http://☃.net/>"#;
    let output = r#"<p><a href="http://xn--n3h.net/">http://☃.net/</a></p>"#;
    run(input, output);
}

#[test]
fn invalid_punycode() {
    let input = r#"<http://xn--xn.com/>"#;
    let output = r#"<p><a href="http://xn--xn.com/">http://xn--xn.com/</a></p>"#;
    run(input, output);
}

#[test]
fn invalid_punycode_non_ascii() {
    let input = r#"<http://xn--γ.com/>"#;
    let output = r#"<p><a href="http://xn--xn---emd.com/">http://xn--γ.com/</a></p>"#;
    run(input, output);
}

#[test]
fn two_slashes_should_start_a_domain() {
    let input = r#"[](//☃.net/)"#;
    let output = r#"<p><a href="//xn--n3h.net/"></a></p>"#;
    run(input, output);
}

#[test]
fn ipv6_address_literals_should_preserve_brackets_while_encoding_other_components() {
    let input = r#"[foo](http://[2001:db8::1]:1896/a[b]?x=[y])"#;
    let output = r#"<p><a href="http://[2001:db8::1]:1896/a%5Bb%5D?x=%5By%5D">foo</a></p>"#;
    run(input, output);
}

#[test]
fn unnamed_2() {
    let input = r#"[foo](//[::ffff:192.0.2.1]/)"#;
    let output = r#"<p><a href="//[::ffff:192.0.2.1]/">foo</a></p>"#;
    run(input, output);
}

#[test]
fn unnamed_3() {
    let input = r#"[foo](http://user:password@[2001:db8:0:0:0:0:0:1]:1926/)"#;
    let output = r#"<p><a href="http://user:password@[2001:db8:0:0:0:0:0:1]:1926/">foo</a></p>"#;
    run(input, output);
}

#[test]
fn don_t_encode_domains_in_unknown_schemas() {
    let input = r#"[](skype:γγγ)"#;
    let output = r#"<p><a href="skype:%CE%B3%CE%B3%CE%B3"></a></p>"#;
    run(input, output);
}

#[test]
fn should_support_idn_in_autolinks() {
    let input = r#"test http://xn--n3h.net/ foo"#;
    let output = r#"<p>test <a href="http://xn--n3h.net/">http://☃.net/</a> foo</p>"#;
    run(input, output);
}

#[test]
fn unnamed_4() {
    let input = r#"test http://☃.net/ foo"#;
    let output = r#"<p>test <a href="http://xn--n3h.net/">http://☃.net/</a> foo</p>"#;
    run(input, output);
}

#[test]
fn unnamed_5() {
    let input = r#"test //xn--n3h.net/ foo"#;
    let output = r#"<p>test <a href="//xn--n3h.net/">//☃.net/</a> foo</p>"#;
    run(input, output);
}

#[test]
fn unnamed_6() {
    let input = r#"test xn--n3h@xn--n3h.net foo"#;
    let output = r#"<p>test <a href="mailto:xn--n3h@xn--n3h.net">xn--n3h@☃.net</a> foo</p>"#;
    run(input, output);
}
// end of auto-generated module
}
///////////////////////////////////////////////////////////////////////////
