fn run(input: &str, output: &str) {
    let output = if output.is_empty() {
        "".to_owned()
    } else {
        output.to_owned() + "\n"
    };
    let md = &mut markdown_it::MarkdownIt::with_preset(markdown_it::Preset::CommonMark);
    let node = md.parse(&(input.to_owned() + "\n"));

    // make sure we have sourcemaps for everything
    node.walk(|node, _| assert!(node.srcmap.is_some()));

    let result = node.render();
    assert_eq!(result, output);

    // The CJK-friendly amendment must not change any original CommonMark case.
    markdown_it::plugins::cjk_friendly::add(md);
    assert_eq!(md.render(&(input.to_owned() + "\n")), result);

    // make sure it doesn't crash without trailing \n
    let _ = md.parse(input.trim_end());
}

///////////////////////////////////////////////////////////////////////////
// TESTGEN: fixtures/commonmark/good.txt
#[rustfmt::skip]
mod fixtures_commonmark_good_txt {
use super::run;
// this part of the file is auto-generated
// don't edit it, otherwise your changes might be lost
#[test]
fn src_line_354() {
    let input = "\tfoo\tbaz\t\tbim";
    let output = "<pre><code>foo\tbaz\t\tbim
</code></pre>";
    run(input, output);
}

#[test]
fn src_line_361() {
    let input = "  \tfoo\tbaz\t\tbim";
    let output = "<pre><code>foo\tbaz\t\tbim
</code></pre>";
    run(input, output);
}

#[test]
fn src_line_368() {
    let input = "    a\ta
    ὐ\ta";
    let output = "<pre><code>a\ta
ὐ\ta
</code></pre>";
    run(input, output);
}

#[test]
fn src_line_381() {
    let input = "  - foo

\tbar";
    let output = r#"<ul>
<li>
<p>foo</p>
<p>bar</p>
</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_394() {
    let input = "- foo

\t\tbar";
    let output = r#"<ul>
<li>
<p>foo</p>
<pre><code>  bar
</code></pre>
</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_417() {
    let input = ">\t\tfoo";
    let output = r#"<blockquote>
<pre><code>  foo
</code></pre>
</blockquote>"#;
    run(input, output);
}

#[test]
fn src_line_426() {
    let input = "-\t\tfoo";
    let output = r#"<ul>
<li>
<pre><code>  foo
</code></pre>
</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_438() {
    let input = "    foo
\tbar";
    let output = r#"<pre><code>foo
bar
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_447() {
    let input = " - foo
   - bar
\t - baz";
    let output = r#"<ul>
<li>foo
<ul>
<li>bar
<ul>
<li>baz</li>
</ul>
</li>
</ul>
</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_465() {
    let input = "#\tFoo";
    let output = r#"<h1>Foo</h1>"#;
    run(input, output);
}

#[test]
fn src_line_471() {
    let input = "*\t*\t*\t";
    let output = r#"<hr />"#;
    run(input, output);
}

#[test]
fn src_line_488() {
    let input = r#"\!\"\#\$\%\&\'\(\)\*\+\,\-\.\/\:\;\<\=\>\?\@\[\\\]\^\_\`\{\|\}\~"#;
    let output = r#"<p>!&quot;#$%&amp;'()*+,-./:;&lt;=&gt;?@[\]^_`{|}~</p>"#;
    run(input, output);
}

#[test]
fn src_line_498() {
    let input = "\\\t\\A\\a\\ \\3\\φ\\«";
    let output = "<p>\\\t\\A\\a\\ \\3\\φ\\«</p>";
    run(input, output);
}

#[test]
fn src_line_508() {
    let input = r#"\*not emphasized*
\<br/> not a tag
\[not a link](/foo)
\`not code`
1\. not a list
\* not a list
\# not a heading
\[foo]: /url "not a reference"
\&ouml; not a character entity"#;
    let output = r#"<p>*not emphasized*
&lt;br/&gt; not a tag
[not a link](/foo)
`not code`
1. not a list
* not a list
# not a heading
[foo]: /url &quot;not a reference&quot;
&amp;ouml; not a character entity</p>"#;
    run(input, output);
}

#[test]
fn src_line_533() {
    let input = r#"\\*emphasis*"#;
    let output = r#"<p>\<em>emphasis</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_542() {
    let input = r#"foo\
bar"#;
    let output = r#"<p>foo<br />
bar</p>"#;
    run(input, output);
}

#[test]
fn src_line_554() {
    let input = r#"`` \[\` ``"#;
    let output = r#"<p><code>\[\`</code></p>"#;
    run(input, output);
}

#[test]
fn src_line_561() {
    let input = r#"    \[\]"#;
    let output = r#"<pre><code>\[\]
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_569() {
    let input = r#"~~~
\[\]
~~~"#;
    let output = r#"<pre><code>\[\]
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_579() {
    let input = r#"<https://example.com?find=\*>"#;
    let output = r#"<p><a href="https://example.com?find=%5C*">https://example.com?find=\*</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_586() {
    let input = r#"<a href="/bar\/)">"#;
    let output = r#"<a href="/bar\/)">"#;
    run(input, output);
}

#[test]
fn src_line_596() {
    let input = r#"[foo](/bar\* "ti\*tle")"#;
    let output = r#"<p><a href="/bar*" title="ti*tle">foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_603() {
    let input = r#"[foo]

[foo]: /bar\* "ti\*tle""#;
    let output = r#"<p><a href="/bar*" title="ti*tle">foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_612() {
    let input = r#"``` foo\+bar
foo
```"#;
    let output = r#"<pre><code class="language-foo+bar">foo
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_648() {
    let input = r#"&nbsp; &amp; &copy; &AElig; &Dcaron;
&frac34; &HilbertSpace; &DifferentialD;
&ClockwiseContourIntegral; &ngE;"#;
    let output = r#"<p>  &amp; © Æ Ď
¾ ℋ ⅆ
∲ ≧̸</p>"#;
    run(input, output);
}

#[test]
fn src_line_667() {
    let input = r#"&#35; &#1234; &#992; &#0;"#;
    let output = r#"<p># Ӓ Ϡ �</p>"#;
    run(input, output);
}

#[test]
fn src_line_680() {
    let input = r#"&#X22; &#XD06; &#xcab;"#;
    let output = r#"<p>&quot; ആ ಫ</p>"#;
    run(input, output);
}

#[test]
fn src_line_689() {
    let input = r#"&nbsp &x; &#; &#x;
&#87654321;
&#abcdef0;
&ThisIsNotDefined; &hi?;"#;
    let output = r#"<p>&amp;nbsp &amp;x; &amp;#; &amp;#x;
&amp;#87654321;
&amp;#abcdef0;
&amp;ThisIsNotDefined; &amp;hi?;</p>"#;
    run(input, output);
}

#[test]
fn src_line_706() {
    let input = r#"&copy"#;
    let output = r#"<p>&amp;copy</p>"#;
    run(input, output);
}

#[test]
fn src_line_716() {
    let input = r#"&MadeUpEntity;"#;
    let output = r#"<p>&amp;MadeUpEntity;</p>"#;
    run(input, output);
}

#[test]
fn src_line_727() {
    let input = r#"<a href="&ouml;&ouml;.html">"#;
    let output = r#"<a href="&ouml;&ouml;.html">"#;
    run(input, output);
}

#[test]
fn src_line_734() {
    let input = r#"[foo](/f&ouml;&ouml; "f&ouml;&ouml;")"#;
    let output = r#"<p><a href="/f%C3%B6%C3%B6" title="föö">foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_741() {
    let input = r#"[foo]

[foo]: /f&ouml;&ouml; "f&ouml;&ouml;""#;
    let output = r#"<p><a href="/f%C3%B6%C3%B6" title="föö">foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_750() {
    let input = r#"``` f&ouml;&ouml;
foo
```"#;
    let output = r#"<pre><code class="language-föö">foo
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_763() {
    let input = r#"`f&ouml;&ouml;`"#;
    let output = r#"<p><code>f&amp;ouml;&amp;ouml;</code></p>"#;
    run(input, output);
}

#[test]
fn src_line_770() {
    let input = r#"    f&ouml;f&ouml;"#;
    let output = r#"<pre><code>f&amp;ouml;f&amp;ouml;
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_782() {
    let input = r#"&#42;foo&#42;
*foo*"#;
    let output = r#"<p>*foo*
<em>foo</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_790() {
    let input = r#"&#42; foo

* foo"#;
    let output = r#"<p>* foo</p>
<ul>
<li>foo</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_801() {
    let input = r#"foo&#10;&#10;bar"#;
    let output = r#"<p>foo

bar</p>"#;
    run(input, output);
}

#[test]
fn src_line_809() {
    let input = r#"&#9;foo"#;
    let output = "<p>\tfoo</p>";
    run(input, output);
}

#[test]
fn src_line_816() {
    let input = r#"[a](url &quot;tit&quot;)"#;
    let output = r#"<p>[a](url &quot;tit&quot;)</p>"#;
    run(input, output);
}

#[test]
fn src_line_839() {
    let input = r#"- `one
- two`"#;
    let output = r#"<ul>
<li>`one</li>
<li>two`</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_878() {
    let input = r#"***
---
___"#;
    let output = r#"<hr />
<hr />
<hr />"#;
    run(input, output);
}

#[test]
fn src_line_891() {
    let input = r#"+++"#;
    let output = r#"<p>+++</p>"#;
    run(input, output);
}

#[test]
fn src_line_898() {
    let input = r#"==="#;
    let output = r#"<p>===</p>"#;
    run(input, output);
}

#[test]
fn src_line_907() {
    let input = r#"--
**
__"#;
    let output = r#"<p>--
**
__</p>"#;
    run(input, output);
}

#[test]
fn src_line_920() {
    let input = r#" ***
  ***
   ***"#;
    let output = r#"<hr />
<hr />
<hr />"#;
    run(input, output);
}

#[test]
fn src_line_933() {
    let input = r#"    ***"#;
    let output = r#"<pre><code>***
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_941() {
    let input = r#"Foo
    ***"#;
    let output = r#"<p>Foo
***</p>"#;
    run(input, output);
}

#[test]
fn src_line_952() {
    let input = r#"_____________________________________"#;
    let output = r#"<hr />"#;
    run(input, output);
}

#[test]
fn src_line_961() {
    let input = r#" - - -"#;
    let output = r#"<hr />"#;
    run(input, output);
}

#[test]
fn src_line_968() {
    let input = r#" **  * ** * ** * **"#;
    let output = r#"<hr />"#;
    run(input, output);
}

#[test]
fn src_line_975() {
    let input = r#"-     -      -      -"#;
    let output = r#"<hr />"#;
    run(input, output);
}

#[test]
fn src_line_984() {
    let input = "- - - -   \x20";
    let output = r#"<hr />"#;
    run(input, output);
}

#[test]
fn src_line_993() {
    let input = r#"_ _ _ _ a

a------

---a---"#;
    let output = r#"<p>_ _ _ _ a</p>
<p>a------</p>
<p>---a---</p>"#;
    run(input, output);
}

#[test]
fn src_line_1009() {
    let input = r#" *-*"#;
    let output = r#"<p><em>-</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_1018() {
    let input = r#"- foo
***
- bar"#;
    let output = r#"<ul>
<li>foo</li>
</ul>
<hr />
<ul>
<li>bar</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_1035() {
    let input = r#"Foo
***
bar"#;
    let output = r#"<p>Foo</p>
<hr />
<p>bar</p>"#;
    run(input, output);
}

#[test]
fn src_line_1052() {
    let input = r#"Foo
---
bar"#;
    let output = r#"<h2>Foo</h2>
<p>bar</p>"#;
    run(input, output);
}

#[test]
fn src_line_1065() {
    let input = r#"* Foo
* * *
* Bar"#;
    let output = r#"<ul>
<li>Foo</li>
</ul>
<hr />
<ul>
<li>Bar</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_1082() {
    let input = r#"- Foo
- * * *"#;
    let output = r#"<ul>
<li>Foo</li>
<li>
<hr />
</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_1111() {
    let input = r#"# foo
## foo
### foo
#### foo
##### foo
###### foo"#;
    let output = r#"<h1>foo</h1>
<h2>foo</h2>
<h3>foo</h3>
<h4>foo</h4>
<h5>foo</h5>
<h6>foo</h6>"#;
    run(input, output);
}

#[test]
fn src_line_1130() {
    let input = r#"####### foo"#;
    let output = r#"<p>####### foo</p>"#;
    run(input, output);
}

#[test]
fn src_line_1145() {
    let input = r#"#5 bolt

#hashtag"#;
    let output = r#"<p>#5 bolt</p>
<p>#hashtag</p>"#;
    run(input, output);
}

#[test]
fn src_line_1157() {
    let input = r#"\## foo"#;
    let output = r#"<p>## foo</p>"#;
    run(input, output);
}

#[test]
fn src_line_1166() {
    let input = r#"# foo *bar* \*baz\*"#;
    let output = r#"<h1>foo <em>bar</em> *baz*</h1>"#;
    run(input, output);
}

#[test]
fn src_line_1175() {
    let input = "#                  foo                    \x20";
    let output = r#"<h1>foo</h1>"#;
    run(input, output);
}

#[test]
fn src_line_1184() {
    let input = r#" ### foo
  ## foo
   # foo"#;
    let output = r#"<h3>foo</h3>
<h2>foo</h2>
<h1>foo</h1>"#;
    run(input, output);
}

#[test]
fn src_line_1197() {
    let input = r#"    # foo"#;
    let output = r#"<pre><code># foo
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_1205() {
    let input = r#"foo
    # bar"#;
    let output = r#"<p>foo
# bar</p>"#;
    run(input, output);
}

#[test]
fn src_line_1216() {
    let input = r#"## foo ##
  ###   bar    ###"#;
    let output = r#"<h2>foo</h2>
<h3>bar</h3>"#;
    run(input, output);
}

#[test]
fn src_line_1227() {
    let input = r#"# foo ##################################
##### foo ##"#;
    let output = r#"<h1>foo</h1>
<h5>foo</h5>"#;
    run(input, output);
}

#[test]
fn src_line_1238() {
    let input = "### foo ###    \x20";
    let output = r#"<h3>foo</h3>"#;
    run(input, output);
}

#[test]
fn src_line_1249() {
    let input = r#"### foo ### b"#;
    let output = r#"<h3>foo ### b</h3>"#;
    run(input, output);
}

#[test]
fn src_line_1258() {
    let input = r#"# foo#"#;
    let output = r#"<h1>foo#</h1>"#;
    run(input, output);
}

#[test]
fn src_line_1268() {
    let input = r#"### foo \###
## foo #\##
# foo \#"#;
    let output = r#"<h3>foo ###</h3>
<h2>foo ###</h2>
<h1>foo #</h1>"#;
    run(input, output);
}

#[test]
fn src_line_1282() {
    let input = r#"****
## foo
****"#;
    let output = r#"<hr />
<h2>foo</h2>
<hr />"#;
    run(input, output);
}

#[test]
fn src_line_1293() {
    let input = r#"Foo bar
# baz
Bar foo"#;
    let output = r#"<p>Foo bar</p>
<h1>baz</h1>
<p>Bar foo</p>"#;
    run(input, output);
}

#[test]
fn src_line_1306() {
    let input = "##\x20
#
### ###";
    let output = r#"<h2></h2>
<h1></h1>
<h3></h3>"#;
    run(input, output);
}

#[test]
fn src_line_1346() {
    let input = r#"Foo *bar*
=========

Foo *bar*
---------"#;
    let output = r#"<h1>Foo <em>bar</em></h1>
<h2>Foo <em>bar</em></h2>"#;
    run(input, output);
}

#[test]
fn src_line_1360() {
    let input = r#"Foo *bar
baz*
===="#;
    let output = r#"<h1>Foo <em>bar
baz</em></h1>"#;
    run(input, output);
}

#[test]
fn src_line_1374() {
    let input = "  Foo *bar
baz*\t
====";
    let output = r#"<h1>Foo <em>bar
baz</em></h1>"#;
    run(input, output);
}

#[test]
fn src_line_1386() {
    let input = r#"Foo
-------------------------

Foo
="#;
    let output = r#"<h2>Foo</h2>
<h1>Foo</h1>"#;
    run(input, output);
}

#[test]
fn src_line_1401() {
    let input = r#"   Foo
---

  Foo
-----

  Foo
  ==="#;
    let output = r#"<h2>Foo</h2>
<h2>Foo</h2>
<h1>Foo</h1>"#;
    run(input, output);
}

#[test]
fn src_line_1419() {
    let input = r#"    Foo
    ---

    Foo
---"#;
    let output = r#"<pre><code>Foo
---

Foo
</code></pre>
<hr />"#;
    run(input, output);
}

#[test]
fn src_line_1438() {
    let input = "Foo
   ----     \x20";
    let output = r#"<h2>Foo</h2>"#;
    run(input, output);
}

#[test]
fn src_line_1448() {
    let input = r#"Foo
    ---"#;
    let output = r#"<p>Foo
---</p>"#;
    run(input, output);
}

#[test]
fn src_line_1459() {
    let input = r#"Foo
= =

Foo
--- -"#;
    let output = r#"<p>Foo
= =</p>
<p>Foo</p>
<hr />"#;
    run(input, output);
}

#[test]
fn src_line_1475() {
    let input = "Foo \x20
-----";
    let output = r#"<h2>Foo</h2>"#;
    run(input, output);
}

#[test]
fn src_line_1485() {
    let input = r#"Foo\
----"#;
    let output = r#"<h2>Foo\</h2>"#;
    run(input, output);
}

#[test]
fn src_line_1496() {
    let input = r#"`Foo
----
`

<a title="a lot
---
of dashes"/>"#;
    let output = r#"<h2>`Foo</h2>
<p>`</p>
<h2>&lt;a title=&quot;a lot</h2>
<p>of dashes&quot;/&gt;</p>"#;
    run(input, output);
}

#[test]
fn src_line_1515() {
    let input = r#"> Foo
---"#;
    let output = r#"<blockquote>
<p>Foo</p>
</blockquote>
<hr />"#;
    run(input, output);
}

#[test]
fn src_line_1526() {
    let input = r#"> foo
bar
==="#;
    let output = r#"<blockquote>
<p>foo
bar
===</p>
</blockquote>"#;
    run(input, output);
}

#[test]
fn src_line_1539() {
    let input = r#"- Foo
---"#;
    let output = r#"<ul>
<li>Foo</li>
</ul>
<hr />"#;
    run(input, output);
}

#[test]
fn src_line_1554() {
    let input = r#"Foo
Bar
---"#;
    let output = r#"<h2>Foo
Bar</h2>"#;
    run(input, output);
}

#[test]
fn src_line_1567() {
    let input = r#"---
Foo
---
Bar
---
Baz"#;
    let output = r#"<hr />
<h2>Foo</h2>
<h2>Bar</h2>
<p>Baz</p>"#;
    run(input, output);
}

#[test]
fn src_line_1584() {
    let input = r#"
===="#;
    let output = r#"<p>====</p>"#;
    run(input, output);
}

#[test]
fn src_line_1596() {
    let input = r#"---
---"#;
    let output = r#"<hr />
<hr />"#;
    run(input, output);
}

#[test]
fn src_line_1605() {
    let input = r#"- foo
-----"#;
    let output = r#"<ul>
<li>foo</li>
</ul>
<hr />"#;
    run(input, output);
}

#[test]
fn src_line_1616() {
    let input = r#"    foo
---"#;
    let output = r#"<pre><code>foo
</code></pre>
<hr />"#;
    run(input, output);
}

#[test]
fn src_line_1626() {
    let input = r#"> foo
-----"#;
    let output = r#"<blockquote>
<p>foo</p>
</blockquote>
<hr />"#;
    run(input, output);
}

#[test]
fn src_line_1640() {
    let input = r#"\> foo
------"#;
    let output = r#"<h2>&gt; foo</h2>"#;
    run(input, output);
}

#[test]
fn src_line_1671() {
    let input = r#"Foo

bar
---
baz"#;
    let output = r#"<p>Foo</p>
<h2>bar</h2>
<p>baz</p>"#;
    run(input, output);
}

#[test]
fn src_line_1687() {
    let input = r#"Foo
bar

---

baz"#;
    let output = r#"<p>Foo
bar</p>
<hr />
<p>baz</p>"#;
    run(input, output);
}

#[test]
fn src_line_1705() {
    let input = r#"Foo
bar
* * *
baz"#;
    let output = r#"<p>Foo
bar</p>
<hr />
<p>baz</p>"#;
    run(input, output);
}

#[test]
fn src_line_1720() {
    let input = r#"Foo
bar
\---
baz"#;
    let output = r#"<p>Foo
bar
---
baz</p>"#;
    run(input, output);
}

#[test]
fn src_line_1748() {
    let input = r#"    a simple
      indented code block"#;
    let output = r#"<pre><code>a simple
  indented code block
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_1762() {
    let input = r#"  - foo

    bar"#;
    let output = r#"<ul>
<li>
<p>foo</p>
<p>bar</p>
</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_1776() {
    let input = r#"1.  foo

    - bar"#;
    let output = r#"<ol>
<li>
<p>foo</p>
<ul>
<li>bar</li>
</ul>
</li>
</ol>"#;
    run(input, output);
}

#[test]
fn src_line_1796() {
    let input = r#"    <a/>
    *hi*

    - one"#;
    let output = r#"<pre><code>&lt;a/&gt;
*hi*

- one
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_1812() {
    let input = "    chunk1

    chunk2
 \x20
\x20
\x20
    chunk3";
    let output = r#"<pre><code>chunk1

chunk2



chunk3
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_1835() {
    let input = "    chunk1
     \x20
      chunk2";
    let output = "<pre><code>chunk1
 \x20
  chunk2
</code></pre>";
    run(input, output);
}

#[test]
fn src_line_1850() {
    let input = r#"Foo
    bar
"#;
    let output = r#"<p>Foo
bar</p>"#;
    run(input, output);
}

#[test]
fn src_line_1864() {
    let input = r#"    foo
bar"#;
    let output = r#"<pre><code>foo
</code></pre>
<p>bar</p>"#;
    run(input, output);
}

#[test]
fn src_line_1877() {
    let input = r#"# Heading
    foo
Heading
------
    foo
----"#;
    let output = r#"<h1>Heading</h1>
<pre><code>foo
</code></pre>
<h2>Heading</h2>
<pre><code>foo
</code></pre>
<hr />"#;
    run(input, output);
}

#[test]
fn src_line_1897() {
    let input = r#"        foo
    bar"#;
    let output = r#"<pre><code>    foo
bar
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_1910() {
    let input = "
   \x20
    foo
   \x20
";
    let output = r#"<pre><code>foo
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_1924() {
    let input = "    foo \x20";
    let output = "<pre><code>foo \x20
</code></pre>";
    run(input, output);
}

#[test]
fn src_line_1979() {
    let input = r#"```
<
 >
```"#;
    let output = r#"<pre><code>&lt;
 &gt;
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_1993() {
    let input = r#"~~~
<
 >
~~~"#;
    let output = r#"<pre><code>&lt;
 &gt;
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_2006() {
    let input = r#"``
foo
``"#;
    let output = r#"<p><code>foo</code></p>"#;
    run(input, output);
}

#[test]
fn src_line_2017() {
    let input = r#"```
aaa
~~~
```"#;
    let output = r#"<pre><code>aaa
~~~
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_2029() {
    let input = r#"~~~
aaa
```
~~~"#;
    let output = r#"<pre><code>aaa
```
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_2043() {
    let input = r#"````
aaa
```
``````"#;
    let output = r#"<pre><code>aaa
```
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_2055() {
    let input = r#"~~~~
aaa
~~~
~~~~"#;
    let output = r#"<pre><code>aaa
~~~
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_2070() {
    let input = r#"```"#;
    let output = r#"<pre><code></code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_2077() {
    let input = r#"`````

```
aaa"#;
    let output = r#"<pre><code>
```
aaa
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_2090() {
    let input = r#"> ```
> aaa

bbb"#;
    let output = r#"<blockquote>
<pre><code>aaa
</code></pre>
</blockquote>
<p>bbb</p>"#;
    run(input, output);
}

#[test]
fn src_line_2106() {
    let input = "```

 \x20
```";
    let output = "<pre><code>
 \x20
</code></pre>";
    run(input, output);
}

#[test]
fn src_line_2120() {
    let input = r#"```
```"#;
    let output = r#"<pre><code></code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_2132() {
    let input = r#" ```
 aaa
aaa
```"#;
    let output = r#"<pre><code>aaa
aaa
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_2144() {
    let input = r#"  ```
aaa
  aaa
aaa
  ```"#;
    let output = r#"<pre><code>aaa
aaa
aaa
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_2158() {
    let input = r#"   ```
   aaa
    aaa
  aaa
   ```"#;
    let output = r#"<pre><code>aaa
 aaa
aaa
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_2174() {
    let input = r#"    ```
    aaa
    ```"#;
    let output = r#"<pre><code>```
aaa
```
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_2189() {
    let input = r#"```
aaa
  ```"#;
    let output = r#"<pre><code>aaa
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_2199() {
    let input = r#"   ```
aaa
  ```"#;
    let output = r#"<pre><code>aaa
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_2211() {
    let input = r#"```
aaa
    ```"#;
    let output = r#"<pre><code>aaa
    ```
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_2225() {
    let input = r#"``` ```
aaa"#;
    let output = r#"<p><code> </code>
aaa</p>"#;
    run(input, output);
}

#[test]
fn src_line_2234() {
    let input = r#"~~~~~~
aaa
~~~ ~~"#;
    let output = r#"<pre><code>aaa
~~~ ~~
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_2248() {
    let input = r#"foo
```
bar
```
baz"#;
    let output = r#"<p>foo</p>
<pre><code>bar
</code></pre>
<p>baz</p>"#;
    run(input, output);
}

#[test]
fn src_line_2265() {
    let input = r#"foo
---
~~~
bar
~~~
# baz"#;
    let output = r#"<h2>foo</h2>
<pre><code>bar
</code></pre>
<h1>baz</h1>"#;
    run(input, output);
}

#[test]
fn src_line_2287() {
    let input = r#"```ruby
def foo(x)
  return 3
end
```"#;
    let output = r#"<pre><code class="language-ruby">def foo(x)
  return 3
end
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_2301() {
    let input = r#"~~~~    ruby startline=3 $%@#$
def foo(x)
  return 3
end
~~~~~~~"#;
    let output = r#"<pre><code class="language-ruby">def foo(x)
  return 3
end
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_2315() {
    let input = r#"````;
````"#;
    let output = r#"<pre><code class="language-;"></code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_2325() {
    let input = r#"``` aa ```
foo"#;
    let output = r#"<p><code>aa</code>
foo</p>"#;
    run(input, output);
}

#[test]
fn src_line_2336() {
    let input = r#"~~~ aa ``` ~~~
foo
~~~"#;
    let output = r#"<pre><code class="language-aa">foo
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_2348() {
    let input = r#"```
``` aaa
```"#;
    let output = r#"<pre><code>``` aaa
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_2427() {
    let input = r#"<table><tr><td>
<pre>
**Hello**,

_world_.
</pre>
</td></tr></table>"#;
    let output = r#"<table><tr><td>
<pre>
**Hello**,
<p><em>world</em>.
</pre></p>
</td></tr></table>"#;
    run(input, output);
}

#[test]
fn src_line_2456() {
    let input = r#"<table>
  <tr>
    <td>
           hi
    </td>
  </tr>
</table>

okay."#;
    let output = r#"<table>
  <tr>
    <td>
           hi
    </td>
  </tr>
</table>
<p>okay.</p>"#;
    run(input, output);
}

#[test]
fn src_line_2478() {
    let input = r#" <div>
  *hello*
         <foo><a>"#;
    let output = r#" <div>
  *hello*
         <foo><a>"#;
    run(input, output);
}

#[test]
fn src_line_2491() {
    let input = r#"</div>
*foo*"#;
    let output = r#"</div>
*foo*"#;
    run(input, output);
}

#[test]
fn src_line_2502() {
    let input = r#"<DIV CLASS="foo">

*Markdown*

</DIV>"#;
    let output = r#"<DIV CLASS="foo">
<p><em>Markdown</em></p>
</DIV>"#;
    run(input, output);
}

#[test]
fn src_line_2518() {
    let input = r#"<div id="foo"
  class="bar">
</div>"#;
    let output = r#"<div id="foo"
  class="bar">
</div>"#;
    run(input, output);
}

#[test]
fn src_line_2529() {
    let input = r#"<div id="foo" class="bar
  baz">
</div>"#;
    let output = r#"<div id="foo" class="bar
  baz">
</div>"#;
    run(input, output);
}

#[test]
fn src_line_2541() {
    let input = r#"<div>
*foo*

*bar*"#;
    let output = r#"<div>
*foo*
<p><em>bar</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_2557() {
    let input = r#"<div id="foo"
*hi*"#;
    let output = r#"<div id="foo"
*hi*"#;
    run(input, output);
}

#[test]
fn src_line_2566() {
    let input = r#"<div class
foo"#;
    let output = r#"<div class
foo"#;
    run(input, output);
}

#[test]
fn src_line_2578() {
    let input = r#"<div *???-&&&-<---
*foo*"#;
    let output = r#"<div *???-&&&-<---
*foo*"#;
    run(input, output);
}

#[test]
fn src_line_2590() {
    let input = r#"<div><a href="bar">*foo*</a></div>"#;
    let output = r#"<div><a href="bar">*foo*</a></div>"#;
    run(input, output);
}

#[test]
fn src_line_2597() {
    let input = r#"<table><tr><td>
foo
</td></tr></table>"#;
    let output = r#"<table><tr><td>
foo
</td></tr></table>"#;
    run(input, output);
}

#[test]
fn src_line_2614() {
    let input = r#"<div></div>
``` c
int x = 33;
```"#;
    let output = r#"<div></div>
``` c
int x = 33;
```"#;
    run(input, output);
}

#[test]
fn src_line_2631() {
    let input = r#"<a href="foo">
*bar*
</a>"#;
    let output = r#"<a href="foo">
*bar*
</a>"#;
    run(input, output);
}

#[test]
fn src_line_2644() {
    let input = r#"<Warning>
*bar*
</Warning>"#;
    let output = r#"<Warning>
*bar*
</Warning>"#;
    run(input, output);
}

#[test]
fn src_line_2655() {
    let input = r#"<i class="foo">
*bar*
</i>"#;
    let output = r#"<i class="foo">
*bar*
</i>"#;
    run(input, output);
}

#[test]
fn src_line_2666() {
    let input = r#"</ins>
*bar*"#;
    let output = r#"</ins>
*bar*"#;
    run(input, output);
}

#[test]
fn src_line_2681() {
    let input = r#"<del>
*foo*
</del>"#;
    let output = r#"<del>
*foo*
</del>"#;
    run(input, output);
}

#[test]
fn src_line_2696() {
    let input = r#"<del>

*foo*

</del>"#;
    let output = r#"<del>
<p><em>foo</em></p>
</del>"#;
    run(input, output);
}

#[test]
fn src_line_2714() {
    let input = r#"<del>*foo*</del>"#;
    let output = r#"<p><del><em>foo</em></del></p>"#;
    run(input, output);
}

#[test]
fn src_line_2730() {
    let input = r#"<pre language="haskell"><code>
import Text.HTML.TagSoup

main :: IO ()
main = print $ parseTags tags
</code></pre>
okay"#;
    let output = r#"<pre language="haskell"><code>
import Text.HTML.TagSoup

main :: IO ()
main = print $ parseTags tags
</code></pre>
<p>okay</p>"#;
    run(input, output);
}

#[test]
fn src_line_2751() {
    let input = r#"<script type="text/javascript">
// JavaScript example

document.getElementById("demo").innerHTML = "Hello JavaScript!";
</script>
okay"#;
    let output = r#"<script type="text/javascript">
// JavaScript example

document.getElementById("demo").innerHTML = "Hello JavaScript!";
</script>
<p>okay</p>"#;
    run(input, output);
}

#[test]
fn src_line_2770() {
    let input = r#"<textarea>

*foo*

_bar_

</textarea>"#;
    let output = r#"<textarea>

*foo*

_bar_

</textarea>"#;
    run(input, output);
}

#[test]
fn src_line_2790() {
    let input = r#"<style
  type="text/css">
h1 {color:red;}

p {color:blue;}
</style>
okay"#;
    let output = r#"<style
  type="text/css">
h1 {color:red;}

p {color:blue;}
</style>
<p>okay</p>"#;
    run(input, output);
}

#[test]
fn src_line_2813() {
    let input = r#"<style
  type="text/css">

foo"#;
    let output = r#"<style
  type="text/css">

foo"#;
    run(input, output);
}

#[test]
fn src_line_2826() {
    let input = r#"> <div>
> foo

bar"#;
    let output = r#"<blockquote>
<div>
foo
</blockquote>
<p>bar</p>"#;
    run(input, output);
}

#[test]
fn src_line_2840() {
    let input = r#"- <div>
- foo"#;
    let output = r#"<ul>
<li>
<div>
</li>
<li>foo</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_2855() {
    let input = r#"<style>p{color:red;}</style>
*foo*"#;
    let output = r#"<style>p{color:red;}</style>
<p><em>foo</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_2864() {
    let input = r#"<!-- foo -->*bar*
*baz*"#;
    let output = r#"<!-- foo -->*bar*
<p><em>baz</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_2876() {
    let input = r#"<script>
foo
</script>1. *bar*"#;
    let output = r#"<script>
foo
</script>1. *bar*"#;
    run(input, output);
}

#[test]
fn src_line_2889() {
    let input = r#"<!-- Foo

bar
   baz -->
okay"#;
    let output = r#"<!-- Foo

bar
   baz -->
<p>okay</p>"#;
    run(input, output);
}

#[test]
fn src_line_2907() {
    let input = r#"<?php

  echo '>';

?>
okay"#;
    let output = r#"<?php

  echo '>';

?>
<p>okay</p>"#;
    run(input, output);
}

#[test]
fn src_line_2926() {
    let input = r#"<!DOCTYPE html>"#;
    let output = r#"<!DOCTYPE html>"#;
    run(input, output);
}

#[test]
fn src_line_2935() {
    let input = r#"<![CDATA[
function matchwo(a,b)
{
  if (a < b && a < 0) then {
    return 1;

  } else {

    return 0;
  }
}
]]>
okay"#;
    let output = r#"<![CDATA[
function matchwo(a,b)
{
  if (a < b && a < 0) then {
    return 1;

  } else {

    return 0;
  }
}
]]>
<p>okay</p>"#;
    run(input, output);
}

#[test]
fn src_line_2969() {
    let input = r#"  <!-- foo -->

    <!-- foo -->"#;
    let output = r#"  <!-- foo -->
<pre><code>&lt;!-- foo --&gt;
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_2980() {
    let input = r#"  <div>

    <div>"#;
    let output = r#"  <div>
<pre><code>&lt;div&gt;
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_2994() {
    let input = r#"Foo
<div>
bar
</div>"#;
    let output = r#"<p>Foo</p>
<div>
bar
</div>"#;
    run(input, output);
}

#[test]
fn src_line_3011() {
    let input = r#"<div>
bar
</div>
*foo*"#;
    let output = r#"<div>
bar
</div>
*foo*"#;
    run(input, output);
}

#[test]
fn src_line_3026() {
    let input = r#"Foo
<a href="bar">
baz"#;
    let output = r#"<p>Foo
<a href="bar">
baz</p>"#;
    run(input, output);
}

#[test]
fn src_line_3067() {
    let input = r#"<div>

*Emphasized* text.

</div>"#;
    let output = r#"<div>
<p><em>Emphasized</em> text.</p>
</div>"#;
    run(input, output);
}

#[test]
fn src_line_3080() {
    let input = r#"<div>
*Emphasized* text.
</div>"#;
    let output = r#"<div>
*Emphasized* text.
</div>"#;
    run(input, output);
}

#[test]
fn src_line_3102() {
    let input = r#"<table>

<tr>

<td>
Hi
</td>

</tr>

</table>"#;
    let output = r#"<table>
<tr>
<td>
Hi
</td>
</tr>
</table>"#;
    run(input, output);
}

#[test]
fn src_line_3129() {
    let input = r#"<table>

  <tr>

    <td>
      Hi
    </td>

  </tr>

</table>"#;
    let output = r#"<table>
  <tr>
<pre><code>&lt;td&gt;
  Hi
&lt;/td&gt;
</code></pre>
  </tr>
</table>"#;
    run(input, output);
}

#[test]
fn src_line_3178() {
    let input = r#"[foo]: /url "title"

[foo]"#;
    let output = r#"<p><a href="/url" title="title">foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_3187() {
    let input = "   [foo]:\x20
      /url \x20
           'the title' \x20

[foo]";
    let output = r#"<p><a href="/url" title="the title">foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_3198() {
    let input = r#"[Foo*bar\]]:my_(url) 'title (with parens)'

[Foo*bar\]]"#;
    let output = r#"<p><a href="my_(url)" title="title (with parens)">Foo*bar]</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_3207() {
    let input = r#"[Foo bar]:
<my url>
'title'

[Foo bar]"#;
    let output = r#"<p><a href="my%20url" title="title">Foo bar</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_3220() {
    let input = r#"[foo]: /url '
title
line1
line2
'

[foo]"#;
    let output = r#"<p><a href="/url" title="
title
line1
line2
">foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_3239() {
    let input = r#"[foo]: /url 'title

with blank line'

[foo]"#;
    let output = r#"<p>[foo]: /url 'title</p>
<p>with blank line'</p>
<p>[foo]</p>"#;
    run(input, output);
}

#[test]
fn src_line_3254() {
    let input = r#"[foo]:
/url

[foo]"#;
    let output = r#"<p><a href="/url">foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_3266() {
    let input = r#"[foo]:

[foo]"#;
    let output = r#"<p>[foo]:</p>
<p>[foo]</p>"#;
    run(input, output);
}

#[test]
fn src_line_3278() {
    let input = r#"[foo]: <>

[foo]"#;
    let output = r#"<p><a href="">foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_3289() {
    let input = r#"[foo]: <bar>(baz)

[foo]"#;
    let output = r#"<p>[foo]: <bar>(baz)</p>
<p>[foo]</p>"#;
    run(input, output);
}

#[test]
fn src_line_3302() {
    let input = r#"[foo]: /url\bar\*baz "foo\"bar\baz"

[foo]"#;
    let output = r#"<p><a href="/url%5Cbar*baz" title="foo&quot;bar\baz">foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_3313() {
    let input = r#"[foo]

[foo]: url"#;
    let output = r#"<p><a href="url">foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_3325() {
    let input = r#"[foo]

[foo]: first
[foo]: second"#;
    let output = r#"<p><a href="first">foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_3338() {
    let input = r#"[FOO]: /url

[Foo]"#;
    let output = r#"<p><a href="/url">Foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_3347() {
    let input = r#"[ΑΓΩ]: /φου

[αγω]"#;
    let output = r#"<p><a href="/%CF%86%CE%BF%CF%85">αγω</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_3362() {
    let input = r#"[foo]: /url"#;
    let output = r#""#;
    run(input, output);
}

#[test]
fn src_line_3370() {
    let input = r#"[
foo
]: /url
bar"#;
    let output = r#"<p>bar</p>"#;
    run(input, output);
}

#[test]
fn src_line_3383() {
    let input = r#"[foo]: /url "title" ok"#;
    let output = r#"<p>[foo]: /url &quot;title&quot; ok</p>"#;
    run(input, output);
}

#[test]
fn src_line_3392() {
    let input = r#"[foo]: /url
"title" ok"#;
    let output = r#"<p>&quot;title&quot; ok</p>"#;
    run(input, output);
}

#[test]
fn src_line_3403() {
    let input = r#"    [foo]: /url "title"

[foo]"#;
    let output = r#"<pre><code>[foo]: /url &quot;title&quot;
</code></pre>
<p>[foo]</p>"#;
    run(input, output);
}

#[test]
fn src_line_3417() {
    let input = r#"```
[foo]: /url
```

[foo]"#;
    let output = r#"<pre><code>[foo]: /url
</code></pre>
<p>[foo]</p>"#;
    run(input, output);
}

#[test]
fn src_line_3432() {
    let input = r#"Foo
[bar]: /baz

[bar]"#;
    let output = r#"<p>Foo
[bar]: /baz</p>
<p>[bar]</p>"#;
    run(input, output);
}

#[test]
fn src_line_3447() {
    let input = r#"# [Foo]
[foo]: /url
> bar"#;
    let output = r#"<h1><a href="/url">Foo</a></h1>
<blockquote>
<p>bar</p>
</blockquote>"#;
    run(input, output);
}

#[test]
fn src_line_3458() {
    let input = r#"[foo]: /url
bar
===
[foo]"#;
    let output = r#"<h1>bar</h1>
<p><a href="/url">foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_3468() {
    let input = r#"[foo]: /url
===
[foo]"#;
    let output = r#"<p>===
<a href="/url">foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_3481() {
    let input = r#"[foo]: /foo-url "foo"
[bar]: /bar-url
  "bar"
[baz]: /baz-url

[foo],
[bar],
[baz]"#;
    let output = r#"<p><a href="/foo-url" title="foo">foo</a>,
<a href="/bar-url" title="bar">bar</a>,
<a href="/baz-url">baz</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_3502() {
    let input = r#"[foo]

> [foo]: /url"#;
    let output = r#"<p><a href="/url">foo</a></p>
<blockquote>
</blockquote>"#;
    run(input, output);
}

#[test]
fn src_line_3524() {
    let input = r#"aaa

bbb"#;
    let output = r#"<p>aaa</p>
<p>bbb</p>"#;
    run(input, output);
}

#[test]
fn src_line_3536() {
    let input = r#"aaa
bbb

ccc
ddd"#;
    let output = r#"<p>aaa
bbb</p>
<p>ccc
ddd</p>"#;
    run(input, output);
}

#[test]
fn src_line_3552() {
    let input = r#"aaa


bbb"#;
    let output = r#"<p>aaa</p>
<p>bbb</p>"#;
    run(input, output);
}

#[test]
fn src_line_3565() {
    let input = r#"  aaa
 bbb"#;
    let output = r#"<p>aaa
bbb</p>"#;
    run(input, output);
}

#[test]
fn src_line_3577() {
    let input = r#"aaa
             bbb
                                       ccc"#;
    let output = r#"<p>aaa
bbb
ccc</p>"#;
    run(input, output);
}

#[test]
fn src_line_3591() {
    let input = r#"   aaa
bbb"#;
    let output = r#"<p>aaa
bbb</p>"#;
    run(input, output);
}

#[test]
fn src_line_3600() {
    let input = r#"    aaa
bbb"#;
    let output = r#"<pre><code>aaa
</code></pre>
<p>bbb</p>"#;
    run(input, output);
}

#[test]
fn src_line_3614() {
    let input = "aaa    \x20
bbb    \x20";
    let output = r#"<p>aaa<br />
bbb</p>"#;
    run(input, output);
}

#[test]
fn src_line_3631() {
    let input = " \x20

aaa
 \x20

# aaa

 \x20";
    let output = r#"<p>aaa</p>
<h1>aaa</h1>"#;
    run(input, output);
}

#[test]
fn src_line_3699() {
    let input = r#"> # Foo
> bar
> baz"#;
    let output = r#"<blockquote>
<h1>Foo</h1>
<p>bar
baz</p>
</blockquote>"#;
    run(input, output);
}

#[test]
fn src_line_3714() {
    let input = r#"># Foo
>bar
> baz"#;
    let output = r#"<blockquote>
<h1>Foo</h1>
<p>bar
baz</p>
</blockquote>"#;
    run(input, output);
}

#[test]
fn src_line_3729() {
    let input = r#"   > # Foo
   > bar
 > baz"#;
    let output = r#"<blockquote>
<h1>Foo</h1>
<p>bar
baz</p>
</blockquote>"#;
    run(input, output);
}

#[test]
fn src_line_3744() {
    let input = r#"    > # Foo
    > bar
    > baz"#;
    let output = r#"<pre><code>&gt; # Foo
&gt; bar
&gt; baz
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_3759() {
    let input = r#"> # Foo
> bar
baz"#;
    let output = r#"<blockquote>
<h1>Foo</h1>
<p>bar
baz</p>
</blockquote>"#;
    run(input, output);
}

#[test]
fn src_line_3775() {
    let input = r#"> bar
baz
> foo"#;
    let output = r#"<blockquote>
<p>bar
baz
foo</p>
</blockquote>"#;
    run(input, output);
}

#[test]
fn src_line_3799() {
    let input = r#"> foo
---"#;
    let output = r#"<blockquote>
<p>foo</p>
</blockquote>
<hr />"#;
    run(input, output);
}

#[test]
fn src_line_3819() {
    let input = r#"> - foo
- bar"#;
    let output = r#"<blockquote>
<ul>
<li>foo</li>
</ul>
</blockquote>
<ul>
<li>bar</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_3837() {
    let input = r#">     foo
    bar"#;
    let output = r#"<blockquote>
<pre><code>foo
</code></pre>
</blockquote>
<pre><code>bar
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_3850() {
    let input = r#"> ```
foo
```"#;
    let output = r#"<blockquote>
<pre><code></code></pre>
</blockquote>
<p>foo</p>
<pre><code></code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_3866() {
    let input = r#"> foo
    - bar"#;
    let output = r#"<blockquote>
<p>foo
- bar</p>
</blockquote>"#;
    run(input, output);
}

#[test]
fn src_line_3890() {
    let input = r#">"#;
    let output = r#"<blockquote>
</blockquote>"#;
    run(input, output);
}

#[test]
fn src_line_3898() {
    let input = ">
> \x20
>\x20";
    let output = r#"<blockquote>
</blockquote>"#;
    run(input, output);
}

#[test]
fn src_line_3910() {
    let input = ">
> foo
> \x20";
    let output = r#"<blockquote>
<p>foo</p>
</blockquote>"#;
    run(input, output);
}

#[test]
fn src_line_3923() {
    let input = r#"> foo

> bar"#;
    let output = r#"<blockquote>
<p>foo</p>
</blockquote>
<blockquote>
<p>bar</p>
</blockquote>"#;
    run(input, output);
}

#[test]
fn src_line_3945() {
    let input = r#"> foo
> bar"#;
    let output = r#"<blockquote>
<p>foo
bar</p>
</blockquote>"#;
    run(input, output);
}

#[test]
fn src_line_3958() {
    let input = r#"> foo
>
> bar"#;
    let output = r#"<blockquote>
<p>foo</p>
<p>bar</p>
</blockquote>"#;
    run(input, output);
}

#[test]
fn src_line_3972() {
    let input = r#"foo
> bar"#;
    let output = r#"<p>foo</p>
<blockquote>
<p>bar</p>
</blockquote>"#;
    run(input, output);
}

#[test]
fn src_line_3986() {
    let input = r#"> aaa
***
> bbb"#;
    let output = r#"<blockquote>
<p>aaa</p>
</blockquote>
<hr />
<blockquote>
<p>bbb</p>
</blockquote>"#;
    run(input, output);
}

#[test]
fn src_line_4004() {
    let input = r#"> bar
baz"#;
    let output = r#"<blockquote>
<p>bar
baz</p>
</blockquote>"#;
    run(input, output);
}

#[test]
fn src_line_4015() {
    let input = r#"> bar

baz"#;
    let output = r#"<blockquote>
<p>bar</p>
</blockquote>
<p>baz</p>"#;
    run(input, output);
}

#[test]
fn src_line_4027() {
    let input = r#"> bar
>
baz"#;
    let output = r#"<blockquote>
<p>bar</p>
</blockquote>
<p>baz</p>"#;
    run(input, output);
}

#[test]
fn src_line_4043() {
    let input = r#"> > > foo
bar"#;
    let output = r#"<blockquote>
<blockquote>
<blockquote>
<p>foo
bar</p>
</blockquote>
</blockquote>
</blockquote>"#;
    run(input, output);
}

#[test]
fn src_line_4058() {
    let input = r#">>> foo
> bar
>>baz"#;
    let output = r#"<blockquote>
<blockquote>
<blockquote>
<p>foo
bar
baz</p>
</blockquote>
</blockquote>
</blockquote>"#;
    run(input, output);
}

#[test]
fn src_line_4080() {
    let input = r#">     code

>    not code"#;
    let output = r#"<blockquote>
<pre><code>code
</code></pre>
</blockquote>
<blockquote>
<p>not code</p>
</blockquote>"#;
    run(input, output);
}

#[test]
fn src_line_4134() {
    let input = r#"A paragraph
with two lines.

    indented code

> A block quote."#;
    let output = r#"<p>A paragraph
with two lines.</p>
<pre><code>indented code
</code></pre>
<blockquote>
<p>A block quote.</p>
</blockquote>"#;
    run(input, output);
}

#[test]
fn src_line_4156() {
    let input = r#"1.  A paragraph
    with two lines.

        indented code

    > A block quote."#;
    let output = r#"<ol>
<li>
<p>A paragraph
with two lines.</p>
<pre><code>indented code
</code></pre>
<blockquote>
<p>A block quote.</p>
</blockquote>
</li>
</ol>"#;
    run(input, output);
}

#[test]
fn src_line_4189() {
    let input = r#"- one

 two"#;
    let output = r#"<ul>
<li>one</li>
</ul>
<p>two</p>"#;
    run(input, output);
}

#[test]
fn src_line_4201() {
    let input = r#"- one

  two"#;
    let output = r#"<ul>
<li>
<p>one</p>
<p>two</p>
</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_4215() {
    let input = r#" -    one

     two"#;
    let output = r#"<ul>
<li>one</li>
</ul>
<pre><code> two
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_4228() {
    let input = r#" -    one

      two"#;
    let output = r#"<ul>
<li>
<p>one</p>
<p>two</p>
</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_4250() {
    let input = r#"   > > 1.  one
>>
>>     two"#;
    let output = r#"<blockquote>
<blockquote>
<ol>
<li>
<p>one</p>
<p>two</p>
</li>
</ol>
</blockquote>
</blockquote>"#;
    run(input, output);
}

#[test]
fn src_line_4277() {
    let input = r#">>- one
>>
  >  > two"#;
    let output = r#"<blockquote>
<blockquote>
<ul>
<li>one</li>
</ul>
<p>two</p>
</blockquote>
</blockquote>"#;
    run(input, output);
}

#[test]
fn src_line_4296() {
    let input = r#"-one

2.two"#;
    let output = r#"<p>-one</p>
<p>2.two</p>"#;
    run(input, output);
}

#[test]
fn src_line_4309() {
    let input = r#"- foo


  bar"#;
    let output = r#"<ul>
<li>
<p>foo</p>
<p>bar</p>
</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_4326() {
    let input = r#"1.  foo

    ```
    bar
    ```

    baz

    > bam"#;
    let output = r#"<ol>
<li>
<p>foo</p>
<pre><code>bar
</code></pre>
<p>baz</p>
<blockquote>
<p>bam</p>
</blockquote>
</li>
</ol>"#;
    run(input, output);
}

#[test]
fn src_line_4354() {
    let input = r#"- Foo

      bar


      baz"#;
    let output = r#"<ul>
<li>
<p>Foo</p>
<pre><code>bar


baz
</code></pre>
</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_4376() {
    let input = r#"123456789. ok"#;
    let output = r#"<ol start="123456789">
<li>ok</li>
</ol>"#;
    run(input, output);
}

#[test]
fn src_line_4385() {
    let input = r#"1234567890. not ok"#;
    let output = r#"<p>1234567890. not ok</p>"#;
    run(input, output);
}

#[test]
fn src_line_4394() {
    let input = r#"0. ok"#;
    let output = r#"<ol start="0">
<li>ok</li>
</ol>"#;
    run(input, output);
}

#[test]
fn src_line_4403() {
    let input = r#"003. ok"#;
    let output = r#"<ol start="3">
<li>ok</li>
</ol>"#;
    run(input, output);
}

#[test]
fn src_line_4414() {
    let input = r#"-1. not ok"#;
    let output = r#"<p>-1. not ok</p>"#;
    run(input, output);
}

#[test]
fn src_line_4437() {
    let input = r#"- foo

      bar"#;
    let output = r#"<ul>
<li>
<p>foo</p>
<pre><code>bar
</code></pre>
</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_4454() {
    let input = r#"  10.  foo

           bar"#;
    let output = r#"<ol start="10">
<li>
<p>foo</p>
<pre><code>bar
</code></pre>
</li>
</ol>"#;
    run(input, output);
}

#[test]
fn src_line_4473() {
    let input = r#"    indented code

paragraph

    more code"#;
    let output = r#"<pre><code>indented code
</code></pre>
<p>paragraph</p>
<pre><code>more code
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_4488() {
    let input = r#"1.     indented code

   paragraph

       more code"#;
    let output = r#"<ol>
<li>
<pre><code>indented code
</code></pre>
<p>paragraph</p>
<pre><code>more code
</code></pre>
</li>
</ol>"#;
    run(input, output);
}

#[test]
fn src_line_4510() {
    let input = r#"1.      indented code

   paragraph

       more code"#;
    let output = r#"<ol>
<li>
<pre><code> indented code
</code></pre>
<p>paragraph</p>
<pre><code>more code
</code></pre>
</li>
</ol>"#;
    run(input, output);
}

#[test]
fn src_line_4537() {
    let input = r#"   foo

bar"#;
    let output = r#"<p>foo</p>
<p>bar</p>"#;
    run(input, output);
}

#[test]
fn src_line_4547() {
    let input = r#"-    foo

  bar"#;
    let output = r#"<ul>
<li>foo</li>
</ul>
<p>bar</p>"#;
    run(input, output);
}

#[test]
fn src_line_4564() {
    let input = r#"-  foo

   bar"#;
    let output = r#"<ul>
<li>
<p>foo</p>
<p>bar</p>
</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_4591() {
    let input = r#"-
  foo
-
  ```
  bar
  ```
-
      baz"#;
    let output = r#"<ul>
<li>foo</li>
<li>
<pre><code>bar
</code></pre>
</li>
<li>
<pre><code>baz
</code></pre>
</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_4617() {
    let input = "-  \x20
  foo";
    let output = r#"<ul>
<li>foo</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_4631() {
    let input = r#"-

  foo"#;
    let output = r#"<ul>
<li></li>
</ul>
<p>foo</p>"#;
    run(input, output);
}

#[test]
fn src_line_4645() {
    let input = r#"- foo
-
- bar"#;
    let output = r#"<ul>
<li>foo</li>
<li></li>
<li>bar</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_4660() {
    let input = "- foo
-  \x20
- bar";
    let output = r#"<ul>
<li>foo</li>
<li></li>
<li>bar</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_4675() {
    let input = r#"1. foo
2.
3. bar"#;
    let output = r#"<ol>
<li>foo</li>
<li></li>
<li>bar</li>
</ol>"#;
    run(input, output);
}

#[test]
fn src_line_4690() {
    let input = r#"*"#;
    let output = r#"<ul>
<li></li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_4700() {
    let input = r#"foo
*

foo
1."#;
    let output = r#"<p>foo
*</p>
<p>foo
1.</p>"#;
    run(input, output);
}

#[test]
fn src_line_4722() {
    let input = r#" 1.  A paragraph
     with two lines.

         indented code

     > A block quote."#;
    let output = r#"<ol>
<li>
<p>A paragraph
with two lines.</p>
<pre><code>indented code
</code></pre>
<blockquote>
<p>A block quote.</p>
</blockquote>
</li>
</ol>"#;
    run(input, output);
}

#[test]
fn src_line_4746() {
    let input = r#"  1.  A paragraph
      with two lines.

          indented code

      > A block quote."#;
    let output = r#"<ol>
<li>
<p>A paragraph
with two lines.</p>
<pre><code>indented code
</code></pre>
<blockquote>
<p>A block quote.</p>
</blockquote>
</li>
</ol>"#;
    run(input, output);
}

#[test]
fn src_line_4770() {
    let input = r#"   1.  A paragraph
       with two lines.

           indented code

       > A block quote."#;
    let output = r#"<ol>
<li>
<p>A paragraph
with two lines.</p>
<pre><code>indented code
</code></pre>
<blockquote>
<p>A block quote.</p>
</blockquote>
</li>
</ol>"#;
    run(input, output);
}

#[test]
fn src_line_4794() {
    let input = r#"    1.  A paragraph
        with two lines.

            indented code

        > A block quote."#;
    let output = r#"<pre><code>1.  A paragraph
    with two lines.

        indented code

    &gt; A block quote.
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_4824() {
    let input = r#"  1.  A paragraph
with two lines.

          indented code

      > A block quote."#;
    let output = r#"<ol>
<li>
<p>A paragraph
with two lines.</p>
<pre><code>indented code
</code></pre>
<blockquote>
<p>A block quote.</p>
</blockquote>
</li>
</ol>"#;
    run(input, output);
}

#[test]
fn src_line_4848() {
    let input = r#"  1.  A paragraph
    with two lines."#;
    let output = r#"<ol>
<li>A paragraph
with two lines.</li>
</ol>"#;
    run(input, output);
}

#[test]
fn src_line_4861() {
    let input = r#"> 1. > Blockquote
continued here."#;
    let output = r#"<blockquote>
<ol>
<li>
<blockquote>
<p>Blockquote
continued here.</p>
</blockquote>
</li>
</ol>
</blockquote>"#;
    run(input, output);
}

#[test]
fn src_line_4878() {
    let input = r#"> 1. > Blockquote
> continued here."#;
    let output = r#"<blockquote>
<ol>
<li>
<blockquote>
<p>Blockquote
continued here.</p>
</blockquote>
</li>
</ol>
</blockquote>"#;
    run(input, output);
}

#[test]
fn src_line_4906() {
    let input = r#"- foo
  - bar
    - baz
      - boo"#;
    let output = r#"<ul>
<li>foo
<ul>
<li>bar
<ul>
<li>baz
<ul>
<li>boo</li>
</ul>
</li>
</ul>
</li>
</ul>
</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_4932() {
    let input = r#"- foo
 - bar
  - baz
   - boo"#;
    let output = r#"<ul>
<li>foo</li>
<li>bar</li>
<li>baz</li>
<li>boo</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_4949() {
    let input = r#"10) foo
    - bar"#;
    let output = r#"<ol start="10">
<li>foo
<ul>
<li>bar</li>
</ul>
</li>
</ol>"#;
    run(input, output);
}

#[test]
fn src_line_4965() {
    let input = r#"10) foo
   - bar"#;
    let output = r#"<ol start="10">
<li>foo</li>
</ol>
<ul>
<li>bar</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_4980() {
    let input = r#"- - foo"#;
    let output = r#"<ul>
<li>
<ul>
<li>foo</li>
</ul>
</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_4993() {
    let input = r#"1. - 2. foo"#;
    let output = r#"<ol>
<li>
<ul>
<li>
<ol start="2">
<li>foo</li>
</ol>
</li>
</ul>
</li>
</ol>"#;
    run(input, output);
}

#[test]
fn src_line_5012() {
    let input = r#"- # Foo
- Bar
  ---
  baz"#;
    let output = r#"<ul>
<li>
<h1>Foo</h1>
</li>
<li>
<h2>Bar</h2>
baz</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_5248() {
    let input = r#"- foo
- bar
+ baz"#;
    let output = r#"<ul>
<li>foo</li>
<li>bar</li>
</ul>
<ul>
<li>baz</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_5263() {
    let input = r#"1. foo
2. bar
3) baz"#;
    let output = r#"<ol>
<li>foo</li>
<li>bar</li>
</ol>
<ol start="3">
<li>baz</li>
</ol>"#;
    run(input, output);
}

#[test]
fn src_line_5282() {
    let input = r#"Foo
- bar
- baz"#;
    let output = r#"<p>Foo</p>
<ul>
<li>bar</li>
<li>baz</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_5359() {
    let input = r#"The number of windows in my house is
14.  The number of doors is 6."#;
    let output = r#"<p>The number of windows in my house is
14.  The number of doors is 6.</p>"#;
    run(input, output);
}

#[test]
fn src_line_5369() {
    let input = r#"The number of windows in my house is
1.  The number of doors is 6."#;
    let output = r#"<p>The number of windows in my house is</p>
<ol>
<li>The number of doors is 6.</li>
</ol>"#;
    run(input, output);
}

#[test]
fn src_line_5383() {
    let input = r#"- foo

- bar


- baz"#;
    let output = r#"<ul>
<li>
<p>foo</p>
</li>
<li>
<p>bar</p>
</li>
<li>
<p>baz</p>
</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_5404() {
    let input = r#"- foo
  - bar
    - baz


      bim"#;
    let output = r#"<ul>
<li>foo
<ul>
<li>bar
<ul>
<li>
<p>baz</p>
<p>bim</p>
</li>
</ul>
</li>
</ul>
</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_5434() {
    let input = r#"- foo
- bar

<!-- -->

- baz
- bim"#;
    let output = r#"<ul>
<li>foo</li>
<li>bar</li>
</ul>
<!-- -->
<ul>
<li>baz</li>
<li>bim</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_5455() {
    let input = r#"-   foo

    notcode

-   foo

<!-- -->

    code"#;
    let output = r#"<ul>
<li>
<p>foo</p>
<p>notcode</p>
</li>
<li>
<p>foo</p>
</li>
</ul>
<!-- -->
<pre><code>code
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_5486() {
    let input = r#"- a
 - b
  - c
   - d
  - e
 - f
- g"#;
    let output = r#"<ul>
<li>a</li>
<li>b</li>
<li>c</li>
<li>d</li>
<li>e</li>
<li>f</li>
<li>g</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_5507() {
    let input = r#"1. a

  2. b

   3. c"#;
    let output = r#"<ol>
<li>
<p>a</p>
</li>
<li>
<p>b</p>
</li>
<li>
<p>c</p>
</li>
</ol>"#;
    run(input, output);
}

#[test]
fn src_line_5531() {
    let input = r#"- a
 - b
  - c
   - d
    - e"#;
    let output = r#"<ul>
<li>a</li>
<li>b</li>
<li>c</li>
<li>d
- e</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_5551() {
    let input = r#"1. a

  2. b

    3. c"#;
    let output = r#"<ol>
<li>
<p>a</p>
</li>
<li>
<p>b</p>
</li>
</ol>
<pre><code>3. c
</code></pre>"#;
    run(input, output);
}

#[test]
fn src_line_5574() {
    let input = r#"- a
- b

- c"#;
    let output = r#"<ul>
<li>
<p>a</p>
</li>
<li>
<p>b</p>
</li>
<li>
<p>c</p>
</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_5596() {
    let input = r#"* a
*

* c"#;
    let output = r#"<ul>
<li>
<p>a</p>
</li>
<li></li>
<li>
<p>c</p>
</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_5618() {
    let input = r#"- a
- b

  c
- d"#;
    let output = r#"<ul>
<li>
<p>a</p>
</li>
<li>
<p>b</p>
<p>c</p>
</li>
<li>
<p>d</p>
</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_5640() {
    let input = r#"- a
- b

  [ref]: /url
- d"#;
    let output = r#"<ul>
<li>
<p>a</p>
</li>
<li>
<p>b</p>
</li>
<li>
<p>d</p>
</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_5663() {
    let input = r#"- a
- ```
  b


  ```
- c"#;
    let output = r#"<ul>
<li>a</li>
<li>
<pre><code>b


</code></pre>
</li>
<li>c</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_5689() {
    let input = r#"- a
  - b

    c
- d"#;
    let output = r#"<ul>
<li>a
<ul>
<li>
<p>b</p>
<p>c</p>
</li>
</ul>
</li>
<li>d</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_5713() {
    let input = r#"* a
  > b
  >
* c"#;
    let output = r#"<ul>
<li>a
<blockquote>
<p>b</p>
</blockquote>
</li>
<li>c</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_5733() {
    let input = r#"- a
  > b
  ```
  c
  ```
- d"#;
    let output = r#"<ul>
<li>a
<blockquote>
<p>b</p>
</blockquote>
<pre><code>c
</code></pre>
</li>
<li>d</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_5756() {
    let input = r#"- a"#;
    let output = r#"<ul>
<li>a</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_5765() {
    let input = r#"- a
  - b"#;
    let output = r#"<ul>
<li>a
<ul>
<li>b</li>
</ul>
</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_5782() {
    let input = r#"1. ```
   foo
   ```

   bar"#;
    let output = r#"<ol>
<li>
<pre><code>foo
</code></pre>
<p>bar</p>
</li>
</ol>"#;
    run(input, output);
}

#[test]
fn src_line_5801() {
    let input = r#"* foo
  * bar

  baz"#;
    let output = r#"<ul>
<li>
<p>foo</p>
<ul>
<li>bar</li>
</ul>
<p>baz</p>
</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_5819() {
    let input = r#"- a
  - b
  - c

- d
  - e
  - f"#;
    let output = r#"<ul>
<li>
<p>a</p>
<ul>
<li>b</li>
<li>c</li>
</ul>
</li>
<li>
<p>d</p>
<ul>
<li>e</li>
<li>f</li>
</ul>
</li>
</ul>"#;
    run(input, output);
}

#[test]
fn src_line_5853() {
    let input = r#"`hi`lo`"#;
    let output = r#"<p><code>hi</code>lo`</p>"#;
    run(input, output);
}

#[test]
fn src_line_5885() {
    let input = r#"`foo`"#;
    let output = r#"<p><code>foo</code></p>"#;
    run(input, output);
}

#[test]
fn src_line_5896() {
    let input = r#"`` foo ` bar ``"#;
    let output = r#"<p><code>foo ` bar</code></p>"#;
    run(input, output);
}

#[test]
fn src_line_5906() {
    let input = r#"` `` `"#;
    let output = r#"<p><code>``</code></p>"#;
    run(input, output);
}

#[test]
fn src_line_5914() {
    let input = r#"`  ``  `"#;
    let output = r#"<p><code> `` </code></p>"#;
    run(input, output);
}

#[test]
fn src_line_5923() {
    let input = r#"` a`"#;
    let output = r#"<p><code> a</code></p>"#;
    run(input, output);
}

#[test]
fn src_line_5932() {
    let input = r#"` b `"#;
    let output = r#"<p><code> b </code></p>"#;
    run(input, output);
}

#[test]
fn src_line_5940() {
    let input = r#"` `
`  `"#;
    let output = r#"<p><code> </code>
<code>  </code></p>"#;
    run(input, output);
}

#[test]
fn src_line_5951() {
    let input = "``
foo
bar \x20
baz
``";
    let output = r#"<p><code>foo bar   baz</code></p>"#;
    run(input, output);
}

#[test]
fn src_line_5961() {
    let input = "``
foo\x20
``";
    let output = r#"<p><code>foo </code></p>"#;
    run(input, output);
}

#[test]
fn src_line_5972() {
    let input = "`foo   bar\x20
baz`";
    let output = r#"<p><code>foo   bar  baz</code></p>"#;
    run(input, output);
}

#[test]
fn src_line_5989() {
    let input = r#"`foo\`bar`"#;
    let output = r#"<p><code>foo\</code>bar`</p>"#;
    run(input, output);
}

#[test]
fn src_line_6000() {
    let input = r#"``foo`bar``"#;
    let output = r#"<p><code>foo`bar</code></p>"#;
    run(input, output);
}

#[test]
fn src_line_6006() {
    let input = r#"` foo `` bar `"#;
    let output = r#"<p><code>foo `` bar</code></p>"#;
    run(input, output);
}

#[test]
fn src_line_6018() {
    let input = r#"*foo`*`"#;
    let output = r#"<p>*foo<code>*</code></p>"#;
    run(input, output);
}

#[test]
fn src_line_6027() {
    let input = r#"[not a `link](/foo`)"#;
    let output = r#"<p>[not a <code>link](/foo</code>)</p>"#;
    run(input, output);
}

#[test]
fn src_line_6037() {
    let input = r#"`<a href="`">`"#;
    let output = r#"<p><code>&lt;a href=&quot;</code>&quot;&gt;`</p>"#;
    run(input, output);
}

#[test]
fn src_line_6046() {
    let input = r#"<a href="`">`"#;
    let output = r#"<p><a href="`">`</p>"#;
    run(input, output);
}

#[test]
fn src_line_6055() {
    let input = r#"`<https://foo.bar.`baz>`"#;
    let output = r#"<p><code>&lt;https://foo.bar.</code>baz&gt;`</p>"#;
    run(input, output);
}

#[test]
fn src_line_6064() {
    let input = r#"<https://foo.bar.`baz>`"#;
    let output = r#"<p><a href="https://foo.bar.%60baz">https://foo.bar.`baz</a>`</p>"#;
    run(input, output);
}

#[test]
fn src_line_6074() {
    let input = r#"```foo``"#;
    let output = r#"<p>```foo``</p>"#;
    run(input, output);
}

#[test]
fn src_line_6081() {
    let input = r#"`foo"#;
    let output = r#"<p>`foo</p>"#;
    run(input, output);
}

#[test]
fn src_line_6090() {
    let input = r#"`foo``bar``"#;
    let output = r#"<p>`foo<code>bar</code></p>"#;
    run(input, output);
}

#[test]
fn src_line_6307() {
    let input = r#"*foo bar*"#;
    let output = r#"<p><em>foo bar</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_6317() {
    let input = r#"a * foo bar*"#;
    let output = r#"<p>a * foo bar*</p>"#;
    run(input, output);
}

#[test]
fn src_line_6328() {
    let input = r#"a*"foo"*"#;
    let output = r#"<p>a*&quot;foo&quot;*</p>"#;
    run(input, output);
}

#[test]
fn src_line_6337() {
    let input = r#"* a *"#;
    let output = r#"<p>* a *</p>"#;
    run(input, output);
}

#[test]
fn src_line_6346() {
    let input = r#"*$*alpha.

*£*bravo.

*€*charlie."#;
    let output = r#"<p>*$*alpha.</p>
<p>*£*bravo.</p>
<p>*€*charlie.</p>"#;
    run(input, output);
}

#[test]
fn src_line_6361() {
    let input = r#"foo*bar*"#;
    let output = r#"<p>foo<em>bar</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_6368() {
    let input = r#"5*6*78"#;
    let output = r#"<p>5<em>6</em>78</p>"#;
    run(input, output);
}

#[test]
fn src_line_6377() {
    let input = r#"_foo bar_"#;
    let output = r#"<p><em>foo bar</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_6387() {
    let input = r#"_ foo bar_"#;
    let output = r#"<p>_ foo bar_</p>"#;
    run(input, output);
}

#[test]
fn src_line_6397() {
    let input = r#"a_"foo"_"#;
    let output = r#"<p>a_&quot;foo&quot;_</p>"#;
    run(input, output);
}

#[test]
fn src_line_6406() {
    let input = r#"foo_bar_"#;
    let output = r#"<p>foo_bar_</p>"#;
    run(input, output);
}

#[test]
fn src_line_6413() {
    let input = r#"5_6_78"#;
    let output = r#"<p>5_6_78</p>"#;
    run(input, output);
}

#[test]
fn src_line_6420() {
    let input = r#"пристаням_стремятся_"#;
    let output = r#"<p>пристаням_стремятся_</p>"#;
    run(input, output);
}

#[test]
fn src_line_6430() {
    let input = r#"aa_"bb"_cc"#;
    let output = r#"<p>aa_&quot;bb&quot;_cc</p>"#;
    run(input, output);
}

#[test]
fn src_line_6441() {
    let input = r#"foo-_(bar)_"#;
    let output = r#"<p>foo-<em>(bar)</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_6453() {
    let input = r#"_foo*"#;
    let output = r#"<p>_foo*</p>"#;
    run(input, output);
}

#[test]
fn src_line_6463() {
    let input = r#"*foo bar *"#;
    let output = r#"<p>*foo bar *</p>"#;
    run(input, output);
}

#[test]
fn src_line_6472() {
    let input = r#"*foo bar
*"#;
    let output = r#"<p>*foo bar
*</p>"#;
    run(input, output);
}

#[test]
fn src_line_6485() {
    let input = r#"*(*foo)"#;
    let output = r#"<p>*(*foo)</p>"#;
    run(input, output);
}

#[test]
fn src_line_6495() {
    let input = r#"*(*foo*)*"#;
    let output = r#"<p><em>(<em>foo</em>)</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_6504() {
    let input = r#"*foo*bar"#;
    let output = r#"<p><em>foo</em>bar</p>"#;
    run(input, output);
}

#[test]
fn src_line_6517() {
    let input = r#"_foo bar _"#;
    let output = r#"<p>_foo bar _</p>"#;
    run(input, output);
}

#[test]
fn src_line_6527() {
    let input = r#"_(_foo)"#;
    let output = r#"<p>_(_foo)</p>"#;
    run(input, output);
}

#[test]
fn src_line_6536() {
    let input = r#"_(_foo_)_"#;
    let output = r#"<p><em>(<em>foo</em>)</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_6545() {
    let input = r#"_foo_bar"#;
    let output = r#"<p>_foo_bar</p>"#;
    run(input, output);
}

#[test]
fn src_line_6552() {
    let input = r#"_пристаням_стремятся"#;
    let output = r#"<p>_пристаням_стремятся</p>"#;
    run(input, output);
}

#[test]
fn src_line_6559() {
    let input = r#"_foo_bar_baz_"#;
    let output = r#"<p><em>foo_bar_baz</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_6570() {
    let input = r#"_(bar)_."#;
    let output = r#"<p><em>(bar)</em>.</p>"#;
    run(input, output);
}

#[test]
fn src_line_6579() {
    let input = r#"**foo bar**"#;
    let output = r#"<p><strong>foo bar</strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_6589() {
    let input = r#"** foo bar**"#;
    let output = r#"<p>** foo bar**</p>"#;
    run(input, output);
}

#[test]
fn src_line_6600() {
    let input = r#"a**"foo"**"#;
    let output = r#"<p>a**&quot;foo&quot;**</p>"#;
    run(input, output);
}

#[test]
fn src_line_6609() {
    let input = r#"foo**bar**"#;
    let output = r#"<p>foo<strong>bar</strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_6618() {
    let input = r#"__foo bar__"#;
    let output = r#"<p><strong>foo bar</strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_6628() {
    let input = r#"__ foo bar__"#;
    let output = r#"<p>__ foo bar__</p>"#;
    run(input, output);
}

#[test]
fn src_line_6636() {
    let input = r#"__
foo bar__"#;
    let output = r#"<p>__
foo bar__</p>"#;
    run(input, output);
}

#[test]
fn src_line_6648() {
    let input = r#"a__"foo"__"#;
    let output = r#"<p>a__&quot;foo&quot;__</p>"#;
    run(input, output);
}

#[test]
fn src_line_6657() {
    let input = r#"foo__bar__"#;
    let output = r#"<p>foo__bar__</p>"#;
    run(input, output);
}

#[test]
fn src_line_6664() {
    let input = r#"5__6__78"#;
    let output = r#"<p>5__6__78</p>"#;
    run(input, output);
}

#[test]
fn src_line_6671() {
    let input = r#"пристаням__стремятся__"#;
    let output = r#"<p>пристаням__стремятся__</p>"#;
    run(input, output);
}

#[test]
fn src_line_6678() {
    let input = r#"__foo, __bar__, baz__"#;
    let output = r#"<p><strong>foo, <strong>bar</strong>, baz</strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_6689() {
    let input = r#"foo-__(bar)__"#;
    let output = r#"<p>foo-<strong>(bar)</strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_6702() {
    let input = r#"**foo bar **"#;
    let output = r#"<p>**foo bar **</p>"#;
    run(input, output);
}

#[test]
fn src_line_6715() {
    let input = r#"**(**foo)"#;
    let output = r#"<p>**(**foo)</p>"#;
    run(input, output);
}

#[test]
fn src_line_6725() {
    let input = r#"*(**foo**)*"#;
    let output = r#"<p><em>(<strong>foo</strong>)</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_6732() {
    let input = r#"**Gomphocarpus (*Gomphocarpus physocarpus*, syn.
*Asclepias physocarpa*)**"#;
    let output = r#"<p><strong>Gomphocarpus (<em>Gomphocarpus physocarpus</em>, syn.
<em>Asclepias physocarpa</em>)</strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_6741() {
    let input = r#"**foo "*bar*" foo**"#;
    let output = r#"<p><strong>foo &quot;<em>bar</em>&quot; foo</strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_6750() {
    let input = r#"**foo**bar"#;
    let output = r#"<p><strong>foo</strong>bar</p>"#;
    run(input, output);
}

#[test]
fn src_line_6762() {
    let input = r#"__foo bar __"#;
    let output = r#"<p>__foo bar __</p>"#;
    run(input, output);
}

#[test]
fn src_line_6772() {
    let input = r#"__(__foo)"#;
    let output = r#"<p>__(__foo)</p>"#;
    run(input, output);
}

#[test]
fn src_line_6782() {
    let input = r#"_(__foo__)_"#;
    let output = r#"<p><em>(<strong>foo</strong>)</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_6791() {
    let input = r#"__foo__bar"#;
    let output = r#"<p>__foo__bar</p>"#;
    run(input, output);
}

#[test]
fn src_line_6798() {
    let input = r#"__пристаням__стремятся"#;
    let output = r#"<p>__пристаням__стремятся</p>"#;
    run(input, output);
}

#[test]
fn src_line_6805() {
    let input = r#"__foo__bar__baz__"#;
    let output = r#"<p><strong>foo__bar__baz</strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_6816() {
    let input = r#"__(bar)__."#;
    let output = r#"<p><strong>(bar)</strong>.</p>"#;
    run(input, output);
}

#[test]
fn src_line_6828() {
    let input = r#"*foo [bar](/url)*"#;
    let output = r#"<p><em>foo <a href="/url">bar</a></em></p>"#;
    run(input, output);
}

#[test]
fn src_line_6835() {
    let input = r#"*foo
bar*"#;
    let output = r#"<p><em>foo
bar</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_6847() {
    let input = r#"_foo __bar__ baz_"#;
    let output = r#"<p><em>foo <strong>bar</strong> baz</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_6854() {
    let input = r#"_foo _bar_ baz_"#;
    let output = r#"<p><em>foo <em>bar</em> baz</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_6861() {
    let input = r#"__foo_ bar_"#;
    let output = r#"<p><em><em>foo</em> bar</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_6868() {
    let input = r#"*foo *bar**"#;
    let output = r#"<p><em>foo <em>bar</em></em></p>"#;
    run(input, output);
}

#[test]
fn src_line_6875() {
    let input = r#"*foo **bar** baz*"#;
    let output = r#"<p><em>foo <strong>bar</strong> baz</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_6881() {
    let input = r#"*foo**bar**baz*"#;
    let output = r#"<p><em>foo<strong>bar</strong>baz</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_6905() {
    let input = r#"*foo**bar*"#;
    let output = r#"<p><em>foo**bar</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_6918() {
    let input = r#"***foo** bar*"#;
    let output = r#"<p><em><strong>foo</strong> bar</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_6925() {
    let input = r#"*foo **bar***"#;
    let output = r#"<p><em>foo <strong>bar</strong></em></p>"#;
    run(input, output);
}

#[test]
fn src_line_6932() {
    let input = r#"*foo**bar***"#;
    let output = r#"<p><em>foo<strong>bar</strong></em></p>"#;
    run(input, output);
}

#[test]
fn src_line_6943() {
    let input = r#"foo***bar***baz"#;
    let output = r#"<p>foo<em><strong>bar</strong></em>baz</p>"#;
    run(input, output);
}

#[test]
fn src_line_6949() {
    let input = r#"foo******bar*********baz"#;
    let output = r#"<p>foo<strong><strong><strong>bar</strong></strong></strong>***baz</p>"#;
    run(input, output);
}

#[test]
fn src_line_6958() {
    let input = r#"*foo **bar *baz* bim** bop*"#;
    let output = r#"<p><em>foo <strong>bar <em>baz</em> bim</strong> bop</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_6965() {
    let input = r#"*foo [*bar*](/url)*"#;
    let output = r#"<p><em>foo <a href="/url"><em>bar</em></a></em></p>"#;
    run(input, output);
}

#[test]
fn src_line_6974() {
    let input = r#"** is not an empty emphasis"#;
    let output = r#"<p>** is not an empty emphasis</p>"#;
    run(input, output);
}

#[test]
fn src_line_6981() {
    let input = r#"**** is not an empty strong emphasis"#;
    let output = r#"<p>**** is not an empty strong emphasis</p>"#;
    run(input, output);
}

#[test]
fn src_line_6994() {
    let input = r#"**foo [bar](/url)**"#;
    let output = r#"<p><strong>foo <a href="/url">bar</a></strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_7001() {
    let input = r#"**foo
bar**"#;
    let output = r#"<p><strong>foo
bar</strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_7013() {
    let input = r#"__foo _bar_ baz__"#;
    let output = r#"<p><strong>foo <em>bar</em> baz</strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_7020() {
    let input = r#"__foo __bar__ baz__"#;
    let output = r#"<p><strong>foo <strong>bar</strong> baz</strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_7027() {
    let input = r#"____foo__ bar__"#;
    let output = r#"<p><strong><strong>foo</strong> bar</strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_7034() {
    let input = r#"**foo **bar****"#;
    let output = r#"<p><strong>foo <strong>bar</strong></strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_7041() {
    let input = r#"**foo *bar* baz**"#;
    let output = r#"<p><strong>foo <em>bar</em> baz</strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_7048() {
    let input = r#"**foo*bar*baz**"#;
    let output = r#"<p><strong>foo<em>bar</em>baz</strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_7055() {
    let input = r#"***foo* bar**"#;
    let output = r#"<p><strong><em>foo</em> bar</strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_7062() {
    let input = r#"**foo *bar***"#;
    let output = r#"<p><strong>foo <em>bar</em></strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_7071() {
    let input = r#"**foo *bar **baz**
bim* bop**"#;
    let output = r#"<p><strong>foo <em>bar <strong>baz</strong>
bim</em> bop</strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_7080() {
    let input = r#"**foo [*bar*](/url)**"#;
    let output = r#"<p><strong>foo <a href="/url"><em>bar</em></a></strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_7089() {
    let input = r#"__ is not an empty emphasis"#;
    let output = r#"<p>__ is not an empty emphasis</p>"#;
    run(input, output);
}

#[test]
fn src_line_7096() {
    let input = r#"____ is not an empty strong emphasis"#;
    let output = r#"<p>____ is not an empty strong emphasis</p>"#;
    run(input, output);
}

#[test]
fn src_line_7106() {
    let input = r#"foo ***"#;
    let output = r#"<p>foo ***</p>"#;
    run(input, output);
}

#[test]
fn src_line_7113() {
    let input = r#"foo *\**"#;
    let output = r#"<p>foo <em>*</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_7120() {
    let input = r#"foo *_*"#;
    let output = r#"<p>foo <em>_</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_7127() {
    let input = r#"foo *****"#;
    let output = r#"<p>foo *****</p>"#;
    run(input, output);
}

#[test]
fn src_line_7134() {
    let input = r#"foo **\***"#;
    let output = r#"<p>foo <strong>*</strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_7141() {
    let input = r#"foo **_**"#;
    let output = r#"<p>foo <strong>_</strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_7152() {
    let input = r#"**foo*"#;
    let output = r#"<p>*<em>foo</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_7159() {
    let input = r#"*foo**"#;
    let output = r#"<p><em>foo</em>*</p>"#;
    run(input, output);
}

#[test]
fn src_line_7166() {
    let input = r#"***foo**"#;
    let output = r#"<p>*<strong>foo</strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_7173() {
    let input = r#"****foo*"#;
    let output = r#"<p>***<em>foo</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_7180() {
    let input = r#"**foo***"#;
    let output = r#"<p><strong>foo</strong>*</p>"#;
    run(input, output);
}

#[test]
fn src_line_7187() {
    let input = r#"*foo****"#;
    let output = r#"<p><em>foo</em>***</p>"#;
    run(input, output);
}

#[test]
fn src_line_7197() {
    let input = r#"foo ___"#;
    let output = r#"<p>foo ___</p>"#;
    run(input, output);
}

#[test]
fn src_line_7204() {
    let input = r#"foo _\__"#;
    let output = r#"<p>foo <em>_</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_7211() {
    let input = r#"foo _*_"#;
    let output = r#"<p>foo <em>*</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_7218() {
    let input = r#"foo _____"#;
    let output = r#"<p>foo _____</p>"#;
    run(input, output);
}

#[test]
fn src_line_7225() {
    let input = r#"foo __\___"#;
    let output = r#"<p>foo <strong>_</strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_7232() {
    let input = r#"foo __*__"#;
    let output = r#"<p>foo <strong>*</strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_7239() {
    let input = r#"__foo_"#;
    let output = r#"<p>_<em>foo</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_7250() {
    let input = r#"_foo__"#;
    let output = r#"<p><em>foo</em>_</p>"#;
    run(input, output);
}

#[test]
fn src_line_7257() {
    let input = r#"___foo__"#;
    let output = r#"<p>_<strong>foo</strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_7264() {
    let input = r#"____foo_"#;
    let output = r#"<p>___<em>foo</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_7271() {
    let input = r#"__foo___"#;
    let output = r#"<p><strong>foo</strong>_</p>"#;
    run(input, output);
}

#[test]
fn src_line_7278() {
    let input = r#"_foo____"#;
    let output = r#"<p><em>foo</em>___</p>"#;
    run(input, output);
}

#[test]
fn src_line_7288() {
    let input = r#"**foo**"#;
    let output = r#"<p><strong>foo</strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_7295() {
    let input = r#"*_foo_*"#;
    let output = r#"<p><em><em>foo</em></em></p>"#;
    run(input, output);
}

#[test]
fn src_line_7302() {
    let input = r#"__foo__"#;
    let output = r#"<p><strong>foo</strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_7309() {
    let input = r#"_*foo*_"#;
    let output = r#"<p><em><em>foo</em></em></p>"#;
    run(input, output);
}

#[test]
fn src_line_7319() {
    let input = r#"****foo****"#;
    let output = r#"<p><strong><strong>foo</strong></strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_7326() {
    let input = r#"____foo____"#;
    let output = r#"<p><strong><strong>foo</strong></strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_7337() {
    let input = r#"******foo******"#;
    let output = r#"<p><strong><strong><strong>foo</strong></strong></strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_7346() {
    let input = r#"***foo***"#;
    let output = r#"<p><em><strong>foo</strong></em></p>"#;
    run(input, output);
}

#[test]
fn src_line_7353() {
    let input = r#"_____foo_____"#;
    let output = r#"<p><em><strong><strong>foo</strong></strong></em></p>"#;
    run(input, output);
}

#[test]
fn src_line_7362() {
    let input = r#"*foo _bar* baz_"#;
    let output = r#"<p><em>foo _bar</em> baz_</p>"#;
    run(input, output);
}

#[test]
fn src_line_7369() {
    let input = r#"*foo __bar *baz bim__ bam*"#;
    let output = r#"<p><em>foo <strong>bar *baz bim</strong> bam</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_7378() {
    let input = r#"**foo **bar baz**"#;
    let output = r#"<p>**foo <strong>bar baz</strong></p>"#;
    run(input, output);
}

#[test]
fn src_line_7385() {
    let input = r#"*foo *bar baz*"#;
    let output = r#"<p>*foo <em>bar baz</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_7394() {
    let input = r#"*[bar*](/url)"#;
    let output = r#"<p>*<a href="/url">bar*</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7401() {
    let input = r#"_foo [bar_](/url)"#;
    let output = r#"<p>_foo <a href="/url">bar_</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7408() {
    let input = r#"*<img src="foo" title="*"/>"#;
    let output = r#"<p>*<img src="foo" title="*"/></p>"#;
    run(input, output);
}

#[test]
fn src_line_7415() {
    let input = r#"**<a href="**">"#;
    let output = r#"<p>**<a href="**"></p>"#;
    run(input, output);
}

#[test]
fn src_line_7422() {
    let input = r#"__<a href="__">"#;
    let output = r#"<p>__<a href="__"></p>"#;
    run(input, output);
}

#[test]
fn src_line_7429() {
    let input = r#"*a `*`*"#;
    let output = r#"<p><em>a <code>*</code></em></p>"#;
    run(input, output);
}

#[test]
fn src_line_7436() {
    let input = r#"_a `_`_"#;
    let output = r#"<p><em>a <code>_</code></em></p>"#;
    run(input, output);
}

#[test]
fn src_line_7443() {
    let input = r#"**a<https://foo.bar/?q=**>"#;
    let output = r#"<p>**a<a href="https://foo.bar/?q=**">https://foo.bar/?q=**</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7450() {
    let input = r#"__a<https://foo.bar/?q=__>"#;
    let output = r#"<p>__a<a href="https://foo.bar/?q=__">https://foo.bar/?q=__</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7538() {
    let input = r#"[link](/uri "title")"#;
    let output = r#"<p><a href="/uri" title="title">link</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7548() {
    let input = r#"[link](/uri)"#;
    let output = r#"<p><a href="/uri">link</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7554() {
    let input = r#"[](./target.md)"#;
    let output = r#"<p><a href="./target.md"></a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7561() {
    let input = r#"[link]()"#;
    let output = r#"<p><a href="">link</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7568() {
    let input = r#"[link](<>)"#;
    let output = r#"<p><a href="">link</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7575() {
    let input = r#"[]()"#;
    let output = r#"<p><a href=""></a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7584() {
    let input = r#"[link](/my uri)"#;
    let output = r#"<p>[link](/my uri)</p>"#;
    run(input, output);
}

#[test]
fn src_line_7590() {
    let input = r#"[link](</my uri>)"#;
    let output = r#"<p><a href="/my%20uri">link</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7599() {
    let input = r#"[link](foo
bar)"#;
    let output = r#"<p>[link](foo
bar)</p>"#;
    run(input, output);
}

#[test]
fn src_line_7607() {
    let input = r#"[link](<foo
bar>)"#;
    let output = r#"<p>[link](<foo
bar>)</p>"#;
    run(input, output);
}

#[test]
fn src_line_7618() {
    let input = r#"[a](<b)c>)"#;
    let output = r#"<p><a href="b)c">a</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7626() {
    let input = r#"[link](<foo\>)"#;
    let output = r#"<p>[link](&lt;foo&gt;)</p>"#;
    run(input, output);
}

#[test]
fn src_line_7635() {
    let input = r#"[a](<b)c
[a](<b)c>
[a](<b>c)"#;
    let output = r#"<p>[a](&lt;b)c
[a](&lt;b)c&gt;
[a](<b>c)</p>"#;
    run(input, output);
}

#[test]
fn src_line_7647() {
    let input = r#"[link](\(foo\))"#;
    let output = r#"<p><a href="(foo)">link</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7656() {
    let input = r#"[link](foo(and(bar)))"#;
    let output = r#"<p><a href="foo(and(bar))">link</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7665() {
    let input = r#"[link](foo(and(bar))"#;
    let output = r#"<p>[link](foo(and(bar))</p>"#;
    run(input, output);
}

#[test]
fn src_line_7672() {
    let input = r#"[link](foo\(and\(bar\))"#;
    let output = r#"<p><a href="foo(and(bar)">link</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7679() {
    let input = r#"[link](<foo(and(bar)>)"#;
    let output = r#"<p><a href="foo(and(bar)">link</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7689() {
    let input = r#"[link](foo\)\:)"#;
    let output = r#"<p><a href="foo):">link</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7698() {
    let input = r#"[link](#fragment)

[link](https://example.com#fragment)

[link](https://example.com?foo=3#frag)"#;
    let output = r##"<p><a href="#fragment">link</a></p>
<p><a href="https://example.com#fragment">link</a></p>
<p><a href="https://example.com?foo=3#frag">link</a></p>"##;
    run(input, output);
}

#[test]
fn src_line_7714() {
    let input = r#"[link](foo\bar)"#;
    let output = r#"<p><a href="foo%5Cbar">link</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7730() {
    let input = r#"[link](foo%20b&auml;)"#;
    let output = r#"<p><a href="foo%20b%C3%A4">link</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7741() {
    let input = r#"[link]("title")"#;
    let output = r#"<p><a href="%22title%22">link</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7750() {
    let input = r#"[link](/url "title")
[link](/url 'title')
[link](/url (title))"#;
    let output = r#"<p><a href="/url" title="title">link</a>
<a href="/url" title="title">link</a>
<a href="/url" title="title">link</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7764() {
    let input = r#"[link](/url "title \"&quot;")"#;
    let output = r#"<p><a href="/url" title="title &quot;&quot;">link</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7775() {
    let input = r#"[link](/url "title")"#;
    let output = r#"<p><a href="/url%C2%A0%22title%22">link</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7784() {
    let input = r#"[link](/url "title "and" title")"#;
    let output = r#"<p>[link](/url &quot;title &quot;and&quot; title&quot;)</p>"#;
    run(input, output);
}

#[test]
fn src_line_7793() {
    let input = r#"[link](/url 'title "and" title')"#;
    let output = r#"<p><a href="/url" title="title &quot;and&quot; title">link</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7818() {
    let input = r#"[link](   /uri
  "title"  )"#;
    let output = r#"<p><a href="/uri" title="title">link</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7829() {
    let input = r#"[link] (/uri)"#;
    let output = r#"<p>[link] (/uri)</p>"#;
    run(input, output);
}

#[test]
fn src_line_7839() {
    let input = r#"[link [foo [bar]]](/uri)"#;
    let output = r#"<p><a href="/uri">link [foo [bar]]</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7846() {
    let input = r#"[link] bar](/uri)"#;
    let output = r#"<p>[link] bar](/uri)</p>"#;
    run(input, output);
}

#[test]
fn src_line_7853() {
    let input = r#"[link [bar](/uri)"#;
    let output = r#"<p>[link <a href="/uri">bar</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7860() {
    let input = r#"[link \[bar](/uri)"#;
    let output = r#"<p><a href="/uri">link [bar</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7869() {
    let input = r#"[link *foo **bar** `#`*](/uri)"#;
    let output = r#"<p><a href="/uri">link <em>foo <strong>bar</strong> <code>#</code></em></a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7876() {
    let input = r#"[![moon](moon.jpg)](/uri)"#;
    let output = r#"<p><a href="/uri"><img src="moon.jpg" alt="moon" /></a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7885() {
    let input = r#"[foo [bar](/uri)](/uri)"#;
    let output = r#"<p>[foo <a href="/uri">bar</a>](/uri)</p>"#;
    run(input, output);
}

#[test]
fn src_line_7892() {
    let input = r#"[foo *[bar [baz](/uri)](/uri)*](/uri)"#;
    let output = r#"<p>[foo <em>[bar <a href="/uri">baz</a>](/uri)</em>](/uri)</p>"#;
    run(input, output);
}

#[test]
fn src_line_7899() {
    let input = r#"![[[foo](uri1)](uri2)](uri3)"#;
    let output = r#"<p><img src="uri3" alt="[foo](uri2)" /></p>"#;
    run(input, output);
}

#[test]
fn src_line_7909() {
    let input = r#"*[foo*](/uri)"#;
    let output = r#"<p>*<a href="/uri">foo*</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7916() {
    let input = r#"[foo *bar](baz*)"#;
    let output = r#"<p><a href="baz*">foo *bar</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7926() {
    let input = r#"*foo [bar* baz]"#;
    let output = r#"<p><em>foo [bar</em> baz]</p>"#;
    run(input, output);
}

#[test]
fn src_line_7936() {
    let input = r#"[foo <bar attr="](baz)">"#;
    let output = r#"<p>[foo <bar attr="](baz)"></p>"#;
    run(input, output);
}

#[test]
fn src_line_7943() {
    let input = r#"[foo`](/uri)`"#;
    let output = r#"<p>[foo<code>](/uri)</code></p>"#;
    run(input, output);
}

#[test]
fn src_line_7950() {
    let input = r#"[foo<https://example.com/?search=](uri)>"#;
    let output = r#"<p>[foo<a href="https://example.com/?search=%5D(uri)">https://example.com/?search=](uri)</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_7988() {
    let input = r#"[foo][bar]

[bar]: /url "title""#;
    let output = r#"<p><a href="/url" title="title">foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8003() {
    let input = r#"[link [foo [bar]]][ref]

[ref]: /uri"#;
    let output = r#"<p><a href="/uri">link [foo [bar]]</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8012() {
    let input = r#"[link \[bar][ref]

[ref]: /uri"#;
    let output = r#"<p><a href="/uri">link [bar</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8023() {
    let input = r#"[link *foo **bar** `#`*][ref]

[ref]: /uri"#;
    let output = r#"<p><a href="/uri">link <em>foo <strong>bar</strong> <code>#</code></em></a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8032() {
    let input = r#"[![moon](moon.jpg)][ref]

[ref]: /uri"#;
    let output = r#"<p><a href="/uri"><img src="moon.jpg" alt="moon" /></a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8043() {
    let input = r#"[foo [bar](/uri)][ref]

[ref]: /uri"#;
    let output = r#"<p>[foo <a href="/uri">bar</a>]<a href="/uri">ref</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8052() {
    let input = r#"[foo *bar [baz][ref]*][ref]

[ref]: /uri"#;
    let output = r#"<p>[foo <em>bar <a href="/uri">baz</a></em>]<a href="/uri">ref</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8067() {
    let input = r#"*[foo*][ref]

[ref]: /uri"#;
    let output = r#"<p>*<a href="/uri">foo*</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8076() {
    let input = r#"[foo *bar][ref]*

[ref]: /uri"#;
    let output = r#"<p><a href="/uri">foo *bar</a>*</p>"#;
    run(input, output);
}

#[test]
fn src_line_8088() {
    let input = r#"[foo <bar attr="][ref]">

[ref]: /uri"#;
    let output = r#"<p>[foo <bar attr="][ref]"></p>"#;
    run(input, output);
}

#[test]
fn src_line_8097() {
    let input = r#"[foo`][ref]`

[ref]: /uri"#;
    let output = r#"<p>[foo<code>][ref]</code></p>"#;
    run(input, output);
}

#[test]
fn src_line_8106() {
    let input = r#"[foo<https://example.com/?search=][ref]>

[ref]: /uri"#;
    let output = r#"<p>[foo<a href="https://example.com/?search=%5D%5Bref%5D">https://example.com/?search=][ref]</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8117() {
    let input = r#"[foo][BaR]

[bar]: /url "title""#;
    let output = r#"<p><a href="/url" title="title">foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8128() {
    let input = r#"[ẞ]

[SS]: /url"#;
    let output = r#"<p><a href="/url">ẞ</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8140() {
    let input = r#"[Foo
  bar]: /url

[Baz][Foo bar]"#;
    let output = r#"<p><a href="/url">Baz</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8153() {
    let input = r#"[foo] [bar]

[bar]: /url "title""#;
    let output = r#"<p>[foo] <a href="/url" title="title">bar</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8162() {
    let input = r#"[foo]
[bar]

[bar]: /url "title""#;
    let output = r#"<p>[foo]
<a href="/url" title="title">bar</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8203() {
    let input = r#"[foo]: /url1

[foo]: /url2

[bar][foo]"#;
    let output = r#"<p><a href="/url1">bar</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8218() {
    let input = r#"[bar][foo\!]

[foo!]: /url"#;
    let output = r#"<p>[bar][foo!]</p>"#;
    run(input, output);
}

#[test]
fn src_line_8230() {
    let input = r#"[foo][ref[]

[ref[]: /uri"#;
    let output = r#"<p>[foo][ref[]</p>
<p>[ref[]: /uri</p>"#;
    run(input, output);
}

#[test]
fn src_line_8240() {
    let input = r#"[foo][ref[bar]]

[ref[bar]]: /uri"#;
    let output = r#"<p>[foo][ref[bar]]</p>
<p>[ref[bar]]: /uri</p>"#;
    run(input, output);
}

#[test]
fn src_line_8250() {
    let input = r#"[[[foo]]]

[[[foo]]]: /url"#;
    let output = r#"<p>[[[foo]]]</p>
<p>[[[foo]]]: /url</p>"#;
    run(input, output);
}

#[test]
fn src_line_8260() {
    let input = r#"[foo][ref\[]

[ref\[]: /uri"#;
    let output = r#"<p><a href="/uri">foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8271() {
    let input = r#"[bar\\]: /uri

[bar\\]"#;
    let output = r#"<p><a href="/uri">bar\</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8283() {
    let input = r#"[]

[]: /uri"#;
    let output = r#"<p>[]</p>
<p>[]: /uri</p>"#;
    run(input, output);
}

#[test]
fn src_line_8293() {
    let input = r#"[
 ]

[
 ]: /uri"#;
    let output = r#"<p>[
]</p>
<p>[
]: /uri</p>"#;
    run(input, output);
}

#[test]
fn src_line_8316() {
    let input = r#"[foo][]

[foo]: /url "title""#;
    let output = r#"<p><a href="/url" title="title">foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8325() {
    let input = r#"[*foo* bar][]

[*foo* bar]: /url "title""#;
    let output = r#"<p><a href="/url" title="title"><em>foo</em> bar</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8336() {
    let input = r#"[Foo][]

[foo]: /url "title""#;
    let output = r#"<p><a href="/url" title="title">Foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8349() {
    let input = "[foo]\x20
[]

[foo]: /url \"title\"";
    let output = r#"<p><a href="/url" title="title">foo</a>
[]</p>"#;
    run(input, output);
}

#[test]
fn src_line_8369() {
    let input = r#"[foo]

[foo]: /url "title""#;
    let output = r#"<p><a href="/url" title="title">foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8378() {
    let input = r#"[*foo* bar]

[*foo* bar]: /url "title""#;
    let output = r#"<p><a href="/url" title="title"><em>foo</em> bar</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8387() {
    let input = r#"[[*foo* bar]]

[*foo* bar]: /url "title""#;
    let output = r#"<p>[<a href="/url" title="title"><em>foo</em> bar</a>]</p>"#;
    run(input, output);
}

#[test]
fn src_line_8396() {
    let input = r#"[[bar [foo]

[foo]: /url"#;
    let output = r#"<p>[[bar <a href="/url">foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8407() {
    let input = r#"[Foo]

[foo]: /url "title""#;
    let output = r#"<p><a href="/url" title="title">Foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8418() {
    let input = r#"[foo] bar

[foo]: /url"#;
    let output = r#"<p><a href="/url">foo</a> bar</p>"#;
    run(input, output);
}

#[test]
fn src_line_8430() {
    let input = r#"\[foo]

[foo]: /url "title""#;
    let output = r#"<p>[foo]</p>"#;
    run(input, output);
}

#[test]
fn src_line_8442() {
    let input = r#"[foo*]: /url

*[foo*]"#;
    let output = r#"<p>*<a href="/url">foo*</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8454() {
    let input = r#"[foo][bar]

[foo]: /url1
[bar]: /url2"#;
    let output = r#"<p><a href="/url2">foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8463() {
    let input = r#"[foo][]

[foo]: /url1"#;
    let output = r#"<p><a href="/url1">foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8473() {
    let input = r#"[foo]()

[foo]: /url1"#;
    let output = r#"<p><a href="">foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8481() {
    let input = r#"[foo](not a link)

[foo]: /url1"#;
    let output = r#"<p><a href="/url1">foo</a>(not a link)</p>"#;
    run(input, output);
}

#[test]
fn src_line_8492() {
    let input = r#"[foo][bar][baz]

[baz]: /url"#;
    let output = r#"<p>[foo]<a href="/url">bar</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8504() {
    let input = r#"[foo][bar][baz]

[baz]: /url1
[bar]: /url2"#;
    let output = r#"<p><a href="/url2">foo</a><a href="/url1">baz</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8517() {
    let input = r#"[foo][bar][baz]

[baz]: /url1
[foo]: /url2"#;
    let output = r#"<p>[foo]<a href="/url1">bar</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8540() {
    let input = r#"![foo](/url "title")"#;
    let output = r#"<p><img src="/url" alt="foo" title="title" /></p>"#;
    run(input, output);
}

#[test]
fn src_line_8547() {
    let input = r#"![foo *bar*]

[foo *bar*]: train.jpg "train & tracks""#;
    let output = r#"<p><img src="train.jpg" alt="foo bar" title="train &amp; tracks" /></p>"#;
    run(input, output);
}

#[test]
fn src_line_8556() {
    let input = r#"![foo ![bar](/url)](/url2)"#;
    let output = r#"<p><img src="/url2" alt="foo bar" /></p>"#;
    run(input, output);
}

#[test]
fn src_line_8563() {
    let input = r#"![foo [bar](/url)](/url2)"#;
    let output = r#"<p><img src="/url2" alt="foo bar" /></p>"#;
    run(input, output);
}

#[test]
fn src_line_8577() {
    let input = r#"![foo *bar*][]

[foo *bar*]: train.jpg "train & tracks""#;
    let output = r#"<p><img src="train.jpg" alt="foo bar" title="train &amp; tracks" /></p>"#;
    run(input, output);
}

#[test]
fn src_line_8586() {
    let input = r#"![foo *bar*][foobar]

[FOOBAR]: train.jpg "train & tracks""#;
    let output = r#"<p><img src="train.jpg" alt="foo bar" title="train &amp; tracks" /></p>"#;
    run(input, output);
}

#[test]
fn src_line_8595() {
    let input = r#"![foo](train.jpg)"#;
    let output = r#"<p><img src="train.jpg" alt="foo" /></p>"#;
    run(input, output);
}

#[test]
fn src_line_8602() {
    let input = r#"My ![foo bar](/path/to/train.jpg  "title"   )"#;
    let output = r#"<p>My <img src="/path/to/train.jpg" alt="foo bar" title="title" /></p>"#;
    run(input, output);
}

#[test]
fn src_line_8609() {
    let input = r#"![foo](<url>)"#;
    let output = r#"<p><img src="url" alt="foo" /></p>"#;
    run(input, output);
}

#[test]
fn src_line_8616() {
    let input = r#"![](/url)"#;
    let output = r#"<p><img src="/url" alt="" /></p>"#;
    run(input, output);
}

#[test]
fn src_line_8625() {
    let input = r#"![foo][bar]

[bar]: /url"#;
    let output = r#"<p><img src="/url" alt="foo" /></p>"#;
    run(input, output);
}

#[test]
fn src_line_8634() {
    let input = r#"![foo][bar]

[BAR]: /url"#;
    let output = r#"<p><img src="/url" alt="foo" /></p>"#;
    run(input, output);
}

#[test]
fn src_line_8645() {
    let input = r#"![foo][]

[foo]: /url "title""#;
    let output = r#"<p><img src="/url" alt="foo" title="title" /></p>"#;
    run(input, output);
}

#[test]
fn src_line_8654() {
    let input = r#"![*foo* bar][]

[*foo* bar]: /url "title""#;
    let output = r#"<p><img src="/url" alt="foo bar" title="title" /></p>"#;
    run(input, output);
}

#[test]
fn src_line_8665() {
    let input = r#"![Foo][]

[foo]: /url "title""#;
    let output = r#"<p><img src="/url" alt="Foo" title="title" /></p>"#;
    run(input, output);
}

#[test]
fn src_line_8677() {
    let input = "![foo]\x20
[]

[foo]: /url \"title\"";
    let output = r#"<p><img src="/url" alt="foo" title="title" />
[]</p>"#;
    run(input, output);
}

#[test]
fn src_line_8690() {
    let input = r#"![foo]

[foo]: /url "title""#;
    let output = r#"<p><img src="/url" alt="foo" title="title" /></p>"#;
    run(input, output);
}

#[test]
fn src_line_8699() {
    let input = r#"![*foo* bar]

[*foo* bar]: /url "title""#;
    let output = r#"<p><img src="/url" alt="foo bar" title="title" /></p>"#;
    run(input, output);
}

#[test]
fn src_line_8710() {
    let input = r#"![[foo]]

[[foo]]: /url "title""#;
    let output = r#"<p>![[foo]]</p>
<p>[[foo]]: /url &quot;title&quot;</p>"#;
    run(input, output);
}

#[test]
fn src_line_8722() {
    let input = r#"![Foo]

[foo]: /url "title""#;
    let output = r#"<p><img src="/url" alt="Foo" title="title" /></p>"#;
    run(input, output);
}

#[test]
fn src_line_8734() {
    let input = r#"!\[foo]

[foo]: /url "title""#;
    let output = r#"<p>![foo]</p>"#;
    run(input, output);
}

#[test]
fn src_line_8746() {
    let input = r#"\![foo]

[foo]: /url "title""#;
    let output = r#"<p>!<a href="/url" title="title">foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8779() {
    let input = r#"<http://foo.bar.baz>"#;
    let output = r#"<p><a href="http://foo.bar.baz">http://foo.bar.baz</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8786() {
    let input = r#"<https://foo.bar.baz/test?q=hello&id=22&boolean>"#;
    let output = r#"<p><a href="https://foo.bar.baz/test?q=hello&amp;id=22&amp;boolean">https://foo.bar.baz/test?q=hello&amp;id=22&amp;boolean</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8793() {
    let input = r#"<irc://foo.bar:2233/baz>"#;
    let output = r#"<p><a href="irc://foo.bar:2233/baz">irc://foo.bar:2233/baz</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8802() {
    let input = r#"<MAILTO:FOO@BAR.BAZ>"#;
    let output = r#"<p><a href="MAILTO:FOO@BAR.BAZ">MAILTO:FOO@BAR.BAZ</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8814() {
    let input = r#"<a+b+c:d>"#;
    let output = r#"<p><a href="a+b+c:d">a+b+c:d</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8821() {
    let input = r#"<made-up-scheme://foo,bar>"#;
    let output = r#"<p><a href="made-up-scheme://foo,bar">made-up-scheme://foo,bar</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8828() {
    let input = r#"<https://../>"#;
    let output = r#"<p><a href="https://../">https://../</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8835() {
    let input = r#"<localhost:5001/foo>"#;
    let output = r#"<p><a href="localhost:5001/foo">localhost:5001/foo</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8844() {
    let input = r#"<https://foo.bar/baz bim>"#;
    let output = r#"<p>&lt;https://foo.bar/baz bim&gt;</p>"#;
    run(input, output);
}

#[test]
fn src_line_8853() {
    let input = r#"<https://example.com/\[\>"#;
    let output = r#"<p><a href="https://example.com/%5C%5B%5C">https://example.com/\[\</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8875() {
    let input = r#"<foo@bar.example.com>"#;
    let output = r#"<p><a href="mailto:foo@bar.example.com">foo@bar.example.com</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8882() {
    let input = r#"<foo+special@Bar.baz-bar0.com>"#;
    let output = r#"<p><a href="mailto:foo+special@Bar.baz-bar0.com">foo+special@Bar.baz-bar0.com</a></p>"#;
    run(input, output);
}

#[test]
fn src_line_8891() {
    let input = r#"<foo\+@bar.example.com>"#;
    let output = r#"<p>&lt;foo+@bar.example.com&gt;</p>"#;
    run(input, output);
}

#[test]
fn src_line_8900() {
    let input = r#"<>"#;
    let output = r#"<p>&lt;&gt;</p>"#;
    run(input, output);
}

#[test]
fn src_line_8907() {
    let input = r#"< https://foo.bar >"#;
    let output = r#"<p>&lt; https://foo.bar &gt;</p>"#;
    run(input, output);
}

#[test]
fn src_line_8914() {
    let input = r#"<m:abc>"#;
    let output = r#"<p>&lt;m:abc&gt;</p>"#;
    run(input, output);
}

#[test]
fn src_line_8921() {
    let input = r#"<foo.bar.baz>"#;
    let output = r#"<p>&lt;foo.bar.baz&gt;</p>"#;
    run(input, output);
}

#[test]
fn src_line_8928() {
    let input = r#"https://example.com"#;
    let output = r#"<p>https://example.com</p>"#;
    run(input, output);
}

#[test]
fn src_line_8935() {
    let input = r#"foo@bar.example.com"#;
    let output = r#"<p>foo@bar.example.com</p>"#;
    run(input, output);
}

#[test]
fn src_line_9015() {
    let input = r#"<a><bab><c2c>"#;
    let output = r#"<p><a><bab><c2c></p>"#;
    run(input, output);
}

#[test]
fn src_line_9024() {
    let input = r#"<a/><b2/>"#;
    let output = r#"<p><a/><b2/></p>"#;
    run(input, output);
}

#[test]
fn src_line_9033() {
    let input = r#"<a  /><b2
data="foo" >"#;
    let output = r#"<p><a  /><b2
data="foo" ></p>"#;
    run(input, output);
}

#[test]
fn src_line_9044() {
    let input = r#"<a foo="bar" bam = 'baz <em>"</em>'
_boolean zoop:33=zoop:33 />"#;
    let output = r#"<p><a foo="bar" bam = 'baz <em>"</em>'
_boolean zoop:33=zoop:33 /></p>"#;
    run(input, output);
}

#[test]
fn src_line_9055() {
    let input = r#"Foo <responsive-image src="foo.jpg" />"#;
    let output = r#"<p>Foo <responsive-image src="foo.jpg" /></p>"#;
    run(input, output);
}

#[test]
fn src_line_9064() {
    let input = r#"<33> <__>"#;
    let output = r#"<p>&lt;33&gt; &lt;__&gt;</p>"#;
    run(input, output);
}

#[test]
fn src_line_9073() {
    let input = r#"<a h*#ref="hi">"#;
    let output = r#"<p>&lt;a h*#ref=&quot;hi&quot;&gt;</p>"#;
    run(input, output);
}

#[test]
fn src_line_9082() {
    let input = r#"<a href="hi'> <a href=hi'>"#;
    let output = r#"<p>&lt;a href=&quot;hi'&gt; &lt;a href=hi'&gt;</p>"#;
    run(input, output);
}

#[test]
fn src_line_9091() {
    let input = r#"< a><
foo><bar/ >
<foo bar=baz
bim!bop />"#;
    let output = r#"<p>&lt; a&gt;&lt;
foo&gt;&lt;bar/ &gt;
&lt;foo bar=baz
bim!bop /&gt;</p>"#;
    run(input, output);
}

#[test]
fn src_line_9106() {
    let input = r#"<a href='bar'title=title>"#;
    let output = r#"<p>&lt;a href='bar'title=title&gt;</p>"#;
    run(input, output);
}

#[test]
fn src_line_9115() {
    let input = r#"</a></foo >"#;
    let output = r#"<p></a></foo ></p>"#;
    run(input, output);
}

#[test]
fn src_line_9124() {
    let input = r#"</a href="foo">"#;
    let output = r#"<p>&lt;/a href=&quot;foo&quot;&gt;</p>"#;
    run(input, output);
}

#[test]
fn src_line_9133() {
    let input = r#"foo <!-- this is a --
comment - with hyphens -->"#;
    let output = r#"<p>foo <!-- this is a --
comment - with hyphens --></p>"#;
    run(input, output);
}

#[test]
fn src_line_9141() {
    let input = r#"foo <!--> foo -->

foo <!---> foo -->"#;
    let output = r#"<p>foo <!--> foo --&gt;</p>
<p>foo <!---> foo --&gt;</p>"#;
    run(input, output);
}

#[test]
fn src_line_9153() {
    let input = r#"foo <?php echo $a; ?>"#;
    let output = r#"<p>foo <?php echo $a; ?></p>"#;
    run(input, output);
}

#[test]
fn src_line_9162() {
    let input = r#"foo <!ELEMENT br EMPTY>"#;
    let output = r#"<p>foo <!ELEMENT br EMPTY></p>"#;
    run(input, output);
}

#[test]
fn src_line_9171() {
    let input = r#"foo <![CDATA[>&<]]>"#;
    let output = r#"<p>foo <![CDATA[>&<]]></p>"#;
    run(input, output);
}

#[test]
fn src_line_9181() {
    let input = r#"foo <a href="&ouml;">"#;
    let output = r#"<p>foo <a href="&ouml;"></p>"#;
    run(input, output);
}

#[test]
fn src_line_9190() {
    let input = r#"foo <a href="\*">"#;
    let output = r#"<p>foo <a href="\*"></p>"#;
    run(input, output);
}

#[test]
fn src_line_9197() {
    let input = r#"<a href="\"">"#;
    let output = r#"<p>&lt;a href=&quot;&quot;&quot;&gt;</p>"#;
    run(input, output);
}

#[test]
fn src_line_9211() {
    let input = "foo \x20
baz";
    let output = r#"<p>foo<br />
baz</p>"#;
    run(input, output);
}

#[test]
fn src_line_9223() {
    let input = r#"foo\
baz"#;
    let output = r#"<p>foo<br />
baz</p>"#;
    run(input, output);
}

#[test]
fn src_line_9234() {
    let input = "foo      \x20
baz";
    let output = r#"<p>foo<br />
baz</p>"#;
    run(input, output);
}

#[test]
fn src_line_9245() {
    let input = "foo \x20
     bar";
    let output = r#"<p>foo<br />
bar</p>"#;
    run(input, output);
}

#[test]
fn src_line_9254() {
    let input = r#"foo\
     bar"#;
    let output = r#"<p>foo<br />
bar</p>"#;
    run(input, output);
}

#[test]
fn src_line_9266() {
    let input = "*foo \x20
bar*";
    let output = r#"<p><em>foo<br />
bar</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_9275() {
    let input = r#"*foo\
bar*"#;
    let output = r#"<p><em>foo<br />
bar</em></p>"#;
    run(input, output);
}

#[test]
fn src_line_9286() {
    let input = "`code \x20
span`";
    let output = r#"<p><code>code   span</code></p>"#;
    run(input, output);
}

#[test]
fn src_line_9294() {
    let input = r#"`code\
span`"#;
    let output = r#"<p><code>code\ span</code></p>"#;
    run(input, output);
}

#[test]
fn src_line_9304() {
    let input = "<a href=\"foo \x20
bar\">";
    let output = "<p><a href=\"foo \x20
bar\"></p>";
    run(input, output);
}

#[test]
fn src_line_9313() {
    let input = r#"<a href="foo\
bar">"#;
    let output = r#"<p><a href="foo\
bar"></p>"#;
    run(input, output);
}

#[test]
fn src_line_9326() {
    let input = r#"foo\"#;
    let output = r#"<p>foo\</p>"#;
    run(input, output);
}

#[test]
fn src_line_9333() {
    let input = "foo \x20";
    let output = r#"<p>foo</p>"#;
    run(input, output);
}

#[test]
fn src_line_9340() {
    let input = r#"### foo\"#;
    let output = r#"<h3>foo\</h3>"#;
    run(input, output);
}

#[test]
fn src_line_9347() {
    let input = "### foo \x20";
    let output = r#"<h3>foo</h3>"#;
    run(input, output);
}

#[test]
fn src_line_9362() {
    let input = r#"foo
baz"#;
    let output = r#"<p>foo
baz</p>"#;
    run(input, output);
}

#[test]
fn src_line_9374() {
    let input = "foo\x20
 baz";
    let output = r#"<p>foo
baz</p>"#;
    run(input, output);
}

#[test]
fn src_line_9394() {
    let input = r#"hello $.;'there"#;
    let output = r#"<p>hello $.;'there</p>"#;
    run(input, output);
}

#[test]
fn src_line_9401() {
    let input = r#"Foo χρῆν"#;
    let output = r#"<p>Foo χρῆν</p>"#;
    run(input, output);
}

#[test]
fn src_line_9410() {
    let input = r#"Multiple     spaces"#;
    let output = r#"<p>Multiple     spaces</p>"#;
    run(input, output);
}
// end of auto-generated module
}
