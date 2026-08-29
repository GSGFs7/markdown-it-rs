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
// TESTGEN: fixtures/markdown-it/linkify.txt
#[rustfmt::skip]
mod fixtures_markdown_it_linkify_txt {
use super::run;
// this part of the file is auto-generated
// don't edit it, otherwise your changes might be lost
#[test]
fn linkify() {
    let input = r#"url http://www.youtube.com/watch?v=5Jt5GEr4AYg."#;
    let output = r#"<p>url <a href="http://www.youtube.com/watch?v=5Jt5GEr4AYg">http://www.youtube.com/watch?v=5Jt5GEr4AYg</a>.</p>"#;
    run(input, output);
}

#[test]
fn don_t_touch_text_in_links() {
    let input = r#"[https://example.com](https://example.com)"#;
    let output = r#"<p><a href="https://example.com">https://example.com</a></p>"#;
    run(input, output);
}

#[test]
fn don_t_touch_text_in_autolinks() {
    let input = r#"<https://example.com>"#;
    let output = r#"<p><a href="https://example.com">https://example.com</a></p>"#;
    run(input, output);
}

#[test]
fn don_t_touch_text_in_html_a_tags() {
    let input = r#"<a href="https://example.com">https://example.com</a>"#;
    let output = r#"<p><a href="https://example.com">https://example.com</a></p>"#;
    run(input, output);
}

#[test]
fn entities_inside_raw_links() {
    let input = r#"https://example.com/foo&amp;bar"#;
    let output = r#"<p><a href="https://example.com/foo&amp;amp;bar">https://example.com/foo&amp;amp;bar</a></p>"#;
    run(input, output);
}

#[test]
fn emphasis_inside_raw_links_asterisk_can_happen_in_links_with_params() {
    let input = r#"https://example.com/foo*bar*baz"#;
    let output = r#"<p><a href="https://example.com/foo*bar*baz">https://example.com/foo*bar*baz</a></p>"#;
    run(input, output);
}

#[test]
fn emphasis_inside_raw_links_underscore() {
    let input = r#"http://example.org/foo._bar_-_baz"#;
    let output = r#"<p><a href="http://example.org/foo._bar_-_baz">http://example.org/foo._bar_-_baz</a></p>"#;
    run(input, output);
}

#[test]
fn backticks_inside_raw_links() {
    let input = r#"https://example.com/foo`bar`baz"#;
    let output = r#"<p><a href="https://example.com/foo%60bar%60baz">https://example.com/foo`bar`baz</a></p>"#;
    run(input, output);
}

#[test]
fn links_inside_raw_links() {
    let input = r#"https://example.com/foo[123](456)bar"#;
    let output = r#"<p><a href="https://example.com/foo%5B123%5D(456)bar">https://example.com/foo[123](456)bar</a></p>"#;
    run(input, output);
}

#[test]
fn escapes_not_allowed_at_the_start() {
    let input = r#"\https://example.com"#;
    let output = r#"<p>\https://example.com</p>"#;
    run(input, output);
}

#[test]
fn escapes_not_allowed_at_comma() {
    let input = r#"https\://example.com"#;
    let output = r#"<p>https://example.com</p>"#;
    run(input, output);
}

#[test]
fn escapes_not_allowed_at_slashes() {
    let input = r#"https:\//aa.org https://bb.org"#;
    let output = r#"<p>https://aa.org <a href="https://bb.org">https://bb.org</a></p>"#;
    run(input, output);
}

#[test]
fn bold_links_exclude_markup_of_pairs_from_link_tail() {
    let input = r#"**http://example.com/foobar**"#;
    let output = r#"<p><strong><a href="http://example.com/foobar">http://example.com/foobar</a></strong></p>"#;
    run(input, output);
}

#[test]
fn emails() {
    let input = r#"test@example.com

mailto:test@example.com"#;
    let output = r#"<p><a href="mailto:test@example.com">test@example.com</a></p>
<p><a href="mailto:test@example.com">mailto:test@example.com</a></p>"#;
    run(input, output);
}

#[test]
fn typorgapher_should_not_break_href() {
    let input = r#"http://example.com/(c)"#;
    let output = r#"<p><a href="http://example.com/(c)">http://example.com/(c)</a></p>"#;
    run(input, output);
}

#[test]
fn coverage_prefix_not_valid() {
    let input = r#"http:/example.com/"#;
    let output = r#"<p>http:/example.com/</p>"#;
    run(input, output);
}

#[test]
fn coverage_negative_link_level() {
    let input = r#"</a>[https://example.com](https://example.com)"#;
    let output = r#"<p></a><a href="https://example.com"><a href="https://example.com">https://example.com</a></a></p>"#;
    run(input, output);
}

#[test]
fn emphasis_with_real_link() {
    let input = r#"http://cdecl.ridiculousfish.com/?q=int+%28*f%29+%28float+*%29%3B"#;
    let output = r#"<p><a href="http://cdecl.ridiculousfish.com/?q=int+%28*f%29+%28float+*%29%3B">http://cdecl.ridiculousfish.com/?q=int+(*f)+(float+*)%3B</a></p>"#;
    run(input, output);
}

#[test]
fn emphasis_with_real_link_1() {
    let input = r#"https://www.sell.fi/sites/default/files/elainlaakarilehti/tieteelliset_artikkelit/kahkonen_t._et_al.canine_pancreatitis-_review.pdf"#;
    let output = r#"<p><a href="https://www.sell.fi/sites/default/files/elainlaakarilehti/tieteelliset_artikkelit/kahkonen_t._et_al.canine_pancreatitis-_review.pdf">https://www.sell.fi/sites/default/files/elainlaakarilehti/tieteelliset_artikkelit/kahkonen_t._et_al.canine_pancreatitis-_review.pdf</a></p>"#;
    run(input, output);
}
// end of auto-generated module
}
///////////////////////////////////////////////////////////////////////////
