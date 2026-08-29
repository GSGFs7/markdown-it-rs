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
// TESTGEN: fixtures/markdown-it/xss.txt
#[rustfmt::skip]
mod fixtures_markdown_it_xss_txt {
use super::run;
// this part of the file is auto-generated
// don't edit it, otherwise your changes might be lost
#[test]
fn unnamed() {
    let input = r#"[normal link](javascript)"#;
    let output = r#"<p><a href="javascript">normal link</a></p>"#;
    run(input, output);
}

#[test]
fn should_not_allow_some_protocols_in_links_and_images() {
    let input = r#"[xss link](javascript:alert(1))

[xss link](JAVASCRIPT:alert(1))

[xss link](vbscript:alert(1))

[xss link](VBSCRIPT:alert(1))

[xss link](file:///123)"#;
    let output = r#"<p>[xss link](javascript:alert(1))</p>
<p>[xss link](JAVASCRIPT:alert(1))</p>
<p>[xss link](vbscript:alert(1))</p>
<p>[xss link](VBSCRIPT:alert(1))</p>
<p>[xss link](file:///123)</p>"#;
    run(input, output);
}

#[test]
fn unnamed_1() {
    let input = r#"[xss link](&#34;&#62;&#60;script&#62;alert&#40;&#34;xss&#34;&#41;&#60;/script&#62;)

[xss link](&#74;avascript:alert(1))

[xss link](&#x26;#74;avascript:alert(1))

[xss link](\&#74;avascript:alert(1))"#;
    let output = r#"<p><a href="%22%3E%3Cscript%3Ealert(%22xss%22)%3C/script%3E">xss link</a></p>
<p>[xss link](Javascript:alert(1))</p>
<p><a href="&amp;#74;avascript:alert(1)">xss link</a></p>
<p><a href="&amp;#74;avascript:alert(1)">xss link</a></p>"#;
    run(input, output);
}

#[test]
fn unnamed_2() {
    let input = r#"[xss link](<javascript:alert(1)>)"#;
    let output = r#"<p>[xss link](&lt;javascript:alert(1)&gt;)</p>"#;
    run(input, output);
}

#[test]
fn unnamed_3() {
    let input = r#"[xss link](javascript&#x3A;alert(1))"#;
    let output = r#"<p>[xss link](javascript:alert(1))</p>"#;
    run(input, output);
}

#[test]
fn should_not_allow_data_uri_except_some_whitelisted_mimes() {
    let input = r#"![](data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7)"#;
    let output = r#"<p><img src="data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7" alt=""></p>"#;
    run(input, output);
}

#[test]
fn unnamed_4() {
    let input = r#"[xss link](data:text/html;base64,PHNjcmlwdD5hbGVydCgnWFNTJyk8L3NjcmlwdD4K)"#;
    let output = r#"<p>[xss link](data:text/html;base64,PHNjcmlwdD5hbGVydCgnWFNTJyk8L3NjcmlwdD4K)</p>"#;
    run(input, output);
}

#[test]
fn unnamed_5() {
    let input = r#"[normal link](/javascript:link)"#;
    let output = r#"<p><a href="/javascript:link">normal link</a></p>"#;
    run(input, output);
}

#[test]
fn image_parser_use_the_same_code_base_as_link() {
    let input = r#"![xss link](javascript:alert(1))"#;
    let output = r#"<p>![xss link](javascript:alert(1))</p>"#;
    run(input, output);
}

#[test]
fn autolinks() {
    let input = r#"<javascript&#x3A;alert(1)>

<javascript:alert(1)>"#;
    let output = r#"<p>&lt;javascript:alert(1)&gt;</p>
<p>&lt;javascript:alert(1)&gt;</p>"#;
    run(input, output);
}

#[test]
fn linkifier() {
    let input = r#"javascript&#x3A;alert(1)

javascript:alert(1)"#;
    let output = r#"<p>javascript:alert(1)</p>
<p>javascript:alert(1)</p>"#;
    run(input, output);
}

#[test]
fn references() {
    let input = r#"[test]: javascript:alert(1)"#;
    let output = r#"<p>[test]: javascript:alert(1)</p>"#;
    run(input, output);
}

#[test]
fn make_sure_we_decode_entities_before_split() {
    let input = r#"```js&#32;custom-class
test1
```

```js&#x0C;custom-class
test2
```"#;
    let output = r#"<pre><code class="js">test1
</code></pre>
<pre><code class="js">test2
</code></pre>"#;
    run(input, output);
}
// end of auto-generated module
}
///////////////////////////////////////////////////////////////////////////
