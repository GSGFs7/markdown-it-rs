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
// TESTGEN: fixtures/markdown-it/commonmark_extras.txt
#[rustfmt::skip]
mod fixtures_markdown_it_commonmark_extras_txt {
use super::run;
// this part of the file is auto-generated
// don't edit it, otherwise your changes might be lost
#[test]
fn issue_commonmark_cmark_383() {
    let input = r#"*****Hello*world****"#;
    let output = r#"<p>**<em><strong>Hello<em>world</em></strong></em></p>"#;
    run(input, output);
}

#[test]
fn issue_246_double_escaping_in_alt() {
    let input = r#"![&](#)"#;
    let output = r##"<p><img src="#" alt="&amp;"></p>"##;
    run(input, output);
}

#[test]
fn strip_markdown_in_alt_tags() {
    let input = r#"![*strip* [markdown __in__ alt](#)](#)"#;
    let output = r##"<p><img src="#" alt="strip markdown in alt"></p>"##;
    run(input, output);
}

#[test]
fn issue_55() {
    let input = r#"![test]

![test](foo bar)"#;
    let output = r#"<p>![test]</p>
<p>![test](foo bar)</p>"#;
    run(input, output);
}

#[test]
fn reference_labels_i_k_touppercase_is_i_k_but_these_should_still_be_equivalent() {
    let input = r#"[İϴΩKÅ]

[i̇θωkå]: /url"#;
    let output = r#"<p><a href="/url">İϴΩKÅ</a></p>"#;
    run(input, output);
}

#[test]
fn reference_labels_support_ligatures_equivalent_according_to_unicode_case_folding() {
    let input = r#"[ﬀﬁﬂ]

[fffifl]: /url"#;
    let output = r#"<p><a href="/url">ﬀﬁﬂ</a></p>"#;
    run(input, output);
}

#[test]
fn reference_can_be_interrupted_by_other_rules() {
    let input = r#"[foo]: /url 'title
 - - -
'

[foo]"#;
    let output = r#"<p>[foo]: /url 'title</p>
<hr>
<p>’</p>
<p>[foo]</p>"#;
    run(input, output);
}

#[test]
fn escape_character_in_link_reference_title_doesn_t_escape_newlines() {
    let input = r#"[foo]: /url "
hello
\
\
\
world
"

[foo]"#;
    let output = r#"<p><a href="/url" title="
hello
\
\
\
world
">foo</a></p>"#;
    run(input, output);
}

#[test]
fn issue_35_should_work_as_punctuation() {
    let input = r#"an **(:**<br>"#;
    let output = r#"<p>an <strong>(:</strong><br></p>"#;
    run(input, output);
}

#[test]
fn should_unescape_only_needed_things_in_link_destinations_titles() {
    let input = r#"[test](<\f\o\o\>\\>)

[test](foo "\\\"\b\a\r")"#;
    let output = r#"<p><a href="%5Cf%5Co%5Co%3E%5C">test</a></p>
<p><a href="foo" title="\&quot;\b\a\r">test</a></p>"#;
    run(input, output);
}

#[test]
fn not_a_closing_tag() {
    let input = r#"</ 123>"#;
    let output = r#"<p>&lt;/ 123&gt;</p>"#;
    run(input, output);
}

#[test]
fn escaping_entities_in_links() {
    let input = r#"[](<&quot;> "&amp;&ouml;")

[](<\&quot;> "\&amp;\&ouml;")

[](<\\&quot;> "\\&quot;\\&ouml;")"#;
    let output = r#"<p><a href="%22" title="&amp;ö"></a></p>
<p><a href="&amp;quot;" title="&amp;amp;&amp;ouml;"></a></p>
<p><a href="%5C%22" title="\&quot;\ö"></a></p>"#;
    run(input, output);
}

#[test]
fn checking_combination_of_replaceentities_and_unescapemd() {
    let input = r#"~~~ &amp;&bad;\&amp;\\&amp;
just a funny little fence
~~~"#;
    let output = r#"<pre><code class="&amp;&amp;bad;&amp;amp;\&amp;">just a funny little fence
</code></pre>"#;
    run(input, output);
}

#[test]
fn underscore_between_punctuation_chars_should_be_able_to_close_emphasis() {
    let input = r#"_(hai)_."#;
    let output = r#"<p><em>(hai)</em>.</p>"#;
    run(input, output);
}

#[test]
fn regression_test_should_not_match_emphasis_markers_in_different_link_tags() {
    let input = r#"[*b]() [c*]()"#;
    let output = r#"<p><a href="">*b</a> <a href="">c*</a></p>"#;
    run(input, output);
}

#[test]
fn those_are_two_separate_blockquotes() {
    let input = r#" - > foo
  > bar"#;
    let output = r#"<ul>
<li>
<blockquote>
<p>foo</p>
</blockquote>
</li>
</ul>
<blockquote>
<p>bar</p>
</blockquote>"#;
    run(input, output);
}

#[test]
fn blockquote_should_terminate_itself_after_paragraph_continuation() {
    let input = r#"- list
    > blockquote
blockquote continuation
    - next list item"#;
    let output = r#"<ul>
<li>list
<blockquote>
<p>blockquote
blockquote continuation</p>
</blockquote>
<ul>
<li>next list item</li>
</ul>
</li>
</ul>"#;
    run(input, output);
}

#[test]
fn regression_test_code_block_regular_paragraph() {
    let input = r#">     foo
> bar"#;
    let output = r#"<blockquote>
<pre><code>foo
</code></pre>
<p>bar</p>
</blockquote>"#;
    run(input, output);
}

#[test]
fn regression_test_tabs_in_lists_830() {
    let input = "1. asd
    2. asd

---

1. asd
\t2. asd";
    let output = r#"<ol>
<li>asd
2. asd</li>
</ol>
<hr>
<ol>
<li>asd
2. asd</li>
</ol>"#;
    run(input, output);
}

#[test]
fn blockquotes_inside_indented_lists_should_terminate_correctly() {
    let input = r#"   - a
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
fn don_t_output_empty_class_here() {
    let input = r#"```&#x20;
test
```"#;
    let output = r#"<pre><code>test
</code></pre>"#;
    run(input, output);
}

#[test]
fn setext_header_text_supports_lazy_continuations() {
    let input = r#" - foo
bar
   ==="#;
    let output = r#"<ul>
<li>
<h1>foo
bar</h1>
</li>
</ul>"#;
    run(input, output);
}

#[test]
fn but_setext_header_underline_doesn_t() {
    let input = r#" - foo
   bar
  ==="#;
    let output = r#"<ul>
<li>foo
bar
===</li>
</ul>"#;
    run(input, output);
}

#[test]
fn tabs_should_be_stripped_from_the_beginning_of_the_line() {
    let input = " foo
 bar
\tbaz";
    let output = r#"<p>foo
bar
baz</p>"#;
    run(input, output);
}

#[test]
fn tabs_should_not_cause_hardbreak_eol_tabs_aren_t_stripped_in_commonmark_0_27() {
    let input = "foo1\t
foo2   \x20
bar";
    let output = "<p>foo1\t
foo2<br>
bar</p>";
    run(input, output);
}

#[test]
fn list_item_terminating_quote_should_not_be_paragraph_continuation() {
    let input = r#"1. foo
   > quote
2. bar"#;
    let output = r#"<ol>
<li>foo
<blockquote>
<p>quote</p>
</blockquote>
</li>
<li>bar</li>
</ol>"#;
    run(input, output);
}

#[test]
fn link_destination_cannot_contain() {
    let input = r#"[](<foo<bar>)

[](<foo\<bar>)"#;
    let output = r#"<p>[](&lt;foo<bar>)</p>
<p><a href="foo%3Cbar"></a></p>"#;
    run(input, output);
}

#[test]
fn link_title_cannot_contain_when_opened_with_it() {
    let input = r#"[](url (xxx())

[](url (xxx\())"#;
    let output = r#"<p>[](url (xxx())</p>
<p><a href="url" title="xxx("></a></p>"#;
    run(input, output);
}

#[test]
fn escaped_space_is_not_allowed_in_link_destination_commonmark_commonmark_493() {
    let input = r#"[link](a\ b)"#;
    let output = r#"<p>[link](a\ b)</p>"#;
    run(input, output);
}

#[test]
fn allow_eol_in_processing_instructions_commonmark_commonmark_js_196() {
    let input = r#"a <?
?>"#;
    let output = r#"<p>a <?
?></p>"#;
    run(input, output);
}

#[test]
fn allow_meta_tag_in_an_inline_context_commonmark_commonmark_spec_527() {
    let input = r#"City:
<span itemprop="contentLocation" itemscope itemtype="https://schema.org/City">
  <meta itemprop="name" content="Springfield">
</span>"#;
    let output = r#"<p>City:
<span itemprop="contentLocation" itemscope itemtype="https://schema.org/City">
<meta itemprop="name" content="Springfield">
</span></p>"#;
    run(input, output);
}

#[test]
fn coverage_directive_can_terminate_paragraph() {
    let input = r#"a
<?php"#;
    let output = r#"<p>a</p>
<?php"#;
    run(input, output);
}

#[test]
fn coverage_nested_email_autolink_silent_mode() {
    let input = r#"*<foo@bar.com>*"#;
    let output = r#"<p><em><a href="mailto:foo@bar.com">foo@bar.com</a></em></p>"#;
    run(input, output);
}

#[test]
fn coverage_unpaired_nested_backtick_silent_mode() {
    let input = r#"*`foo*"#;
    let output = r#"<p><em>`foo</em></p>"#;
    run(input, output);
}

#[test]
fn coverage_should_continue_scanning_after_closing_despite_cache() {
    let input = r#"```aaa``bbb``ccc```ddd``eee``"#;
    let output = r#"<p><code>aaa``bbb``ccc</code>ddd<code>eee</code></p>"#;
    run(input, output);
}

#[test]
fn coverage_entities() {
    let input = r#"*&*

*&#x20;*

*&amp;*"#;
    let output = r#"<p><em>&amp;</em></p>
<p><em> </em></p>
<p><em>&amp;</em></p>"#;
    run(input, output);
}

#[test]
fn coverage_escape() {
    let input = r#"*\a*"#;
    let output = r#"<p><em>\a</em></p>"#;
    run(input, output);
}

#[test]
fn coverage_parselinkdestination() {
    let input = r#"[foo](<
bar>)

[foo](<bar)"#;
    let output = r#"<p>[foo](&lt;
bar&gt;)</p>
<p>[foo](&lt;bar)</p>"#;
    run(input, output);
}

#[test]
fn coverage_parselinktitle() {
    let input = r#"[foo](bar "ba)

[foo](bar "ba\
z")"#;
    let output = r#"<p>[foo](bar &quot;ba)</p>
<p><a href="bar" title="ba\
z">foo</a></p>"#;
    run(input, output);
}

#[test]
fn coverage_image() {
    let input = r#"![test]( x )"#;
    let output = r#"<p><img src="x" alt="test"></p>"#;
    run(input, output);
}

#[test]
fn unnamed() {
    let input = r#"![test][foo]

[bar]: 123"#;
    let output = r#"<p>![test][foo]</p>"#;
    run(input, output);
}

#[test]
fn unnamed_1() {
    let input = r#"![test][[[

[bar]: 123"#;
    let output = r#"<p>![test][[[</p>"#;
    run(input, output);
}

#[test]
fn unnamed_2() {
    let input = r#"![test]("#;
    let output = r#"<p>![test](</p>"#;
    run(input, output);
}

#[test]
fn coverage_link() {
    let input = r#"[test]("#;
    let output = r#"<p>[test](</p>"#;
    run(input, output);
}

#[test]
fn coverage_reference() {
    let input = r#"[
test\
]: 123
foo
bar"#;
    let output = r#"<p>foo
bar</p>"#;
    run(input, output);
}

#[test]
fn unnamed_3() {
    let input = r#"[
test
]"#;
    let output = r#"<p>[
test
]</p>"#;
    run(input, output);
}

#[test]
fn unnamed_4() {
    let input = r#"> [foo]: bar
[foo]"#;
    let output = r#"<blockquote></blockquote>
<p><a href="bar">foo</a></p>"#;
    run(input, output);
}

#[test]
fn coverage_tabs_in_blockquotes() {
    let input = ">\t\ttest

 >\t\ttest

  >\t\ttest

> ---
>\t\ttest

 > ---
 >\t\ttest

  > ---
  >\t\ttest

>\t\t\ttest

 >\t\t\ttest

  >\t\t\ttest

> ---
>\t\t\ttest

 > ---
 >\t\t\ttest

  > ---
  >\t\t\ttest";
    let output = "<blockquote>
<pre><code>  test
</code></pre>
</blockquote>
<blockquote>
<pre><code> test
</code></pre>
</blockquote>
<blockquote>
<pre><code>test
</code></pre>
</blockquote>
<blockquote>
<hr>
<pre><code>  test
</code></pre>
</blockquote>
<blockquote>
<hr>
<pre><code> test
</code></pre>
</blockquote>
<blockquote>
<hr>
<pre><code>test
</code></pre>
</blockquote>
<blockquote>
<pre><code>  \ttest
</code></pre>
</blockquote>
<blockquote>
<pre><code> \ttest
</code></pre>
</blockquote>
<blockquote>
<pre><code>\ttest
</code></pre>
</blockquote>
<blockquote>
<hr>
<pre><code>  \ttest
</code></pre>
</blockquote>
<blockquote>
<hr>
<pre><code> \ttest
</code></pre>
</blockquote>
<blockquote>
<hr>
<pre><code>\ttest
</code></pre>
</blockquote>";
    run(input, output);
}

#[test]
fn coverage_tabs_in_lists() {
    let input = "1. \tfoo

\t     bar";
    let output = r#"<ol>
<li>
<p>foo</p>
<pre><code> bar
</code></pre>
</li>
</ol>"#;
    run(input, output);
}

#[test]
fn coverage_various_tags_not_interrupting_blockquotes_because_of_indentation() {
    let input = r#"> foo
    - - - -

> foo
    # not a heading"#;
    let output = r#"<blockquote>
<p>foo
- - - -</p>
</blockquote>
<blockquote>
<p>foo
# not a heading</p>
</blockquote>"#;
    run(input, output);
}

#[test]
fn coverage_entities_with_code_10ffff_made_this_way_for_compatibility_with_commonmark_js() {
    let input = r#"&#x110000;

&#x1100000;"#;
    let output = r#"<p>�</p>
<p>&amp;#x1100000;</p>"#;
    run(input, output);
}

#[test]
fn issue_696_blockquotes_should_remember_their_level() {
    let input = r#">>> foo
bar
>>> baz"#;
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
fn issue_696_blockquotes_should_stop_when_outdented_from_a_list() {
    let input = r#"1. >>> foo
   bar
baz
   >>> foo
  >>> bar
   >>> baz"#;
    let output = r#"<ol>
<li>
<blockquote>
<blockquote>
<blockquote>
<p>foo
bar
baz
foo</p>
</blockquote>
</blockquote>
</blockquote>
</li>
</ol>
<blockquote>
<blockquote>
<blockquote>
<p>bar
baz</p>
</blockquote>
</blockquote>
</blockquote>"#;
    run(input, output);
}

#[test]
fn issue_772_header_rule_should_not_interfere_with_html_tags() {
    let input = r#"<!--
==
-->

<pre>
==
</pre>"#;
    let output = r#"<!--
==
-->
<pre>
==
</pre>"#;
    run(input, output);
}

#[test]
fn softbreak_in_image_description() {
    let input = r#"There is a newline in this image ![here
it is](https://github.com/executablebooks/)"#;
    let output = r#"<p>There is a newline in this image <img src="https://github.com/executablebooks/" alt="here
it is"></p>"#;
    run(input, output);
}

#[test]
fn hardbreak_in_image_description() {
    let input = r#"There is a newline in this image ![here\
it is](https://github.com/executablebooks/)"#;
    let output = r#"<p>There is a newline in this image <img src="https://github.com/executablebooks/" alt="here
it is"></p>"#;
    run(input, output);
}

#[test]
fn html_in_image_description() {
    let input = r#"![text <textarea> text](image.png)"#;
    let output = r#"<p><img src="image.png" alt="text &lt;textarea&gt; text"></p>"#;
    run(input, output);
}

#[test]
fn code_in_image_description_1142() {
    let input = r#"![foo *bar* `baz` bla](image.png)"#;
    let output = r#"<p><img src="image.png" alt="foo bar baz bla"></p>"#;
    run(input, output);
}

#[test]
fn https_github_com_commonmark_commonmark_js_pull_279() {
    let input = r#"&parag;

&para

&para;"#;
    let output = r#"<p>&amp;parag;</p>
<p>&amp;para</p>
<p>¶</p>"#;
    run(input, output);
}

#[test]
fn issue_1067_don_t_trim_non_ascii_whitespaces() {
    let input = "# 　U+3000　 \x20

  　U+3000　 \x20
=

  　U+3000　 \x20";
    let output = r#"<h1>　U+3000　</h1>
<h1>　U+3000　</h1>
<p>　U+3000　</p>"#;
    run(input, output);
}

#[test]
fn issue_1071_recognize_non_bmp_punctuations_and_symbols() {
    let input = r#"a*a∇*a

a*∇a*a

a*a𝜵*a

a*𝜵a*a

a*𐬼a*a

a*a𐬼*a"#;
    let output = r#"<p>a*a∇*a</p>
<p>a*∇a*a</p>
<p>a*a𝜵*a</p>
<p>a*𝜵a*a</p>
<p>a*𐬼a*a</p>
<p>a*a𐬼*a</p>"#;
    run(input, output);
}

#[test]
fn issue_1144_comment_html_block_type_2_must_not_end_on_a_blank_line_inside_a_list() {
    let input = r#"1. item

    <!--
    a

    b
    -->
    c"#;
    let output = r#"<ol>
<li>
<p>item</p>
 <!--
 a

 b
 -->
<p>c</p>
</li>
</ol>"#;
    run(input, output);
}

#[test]
fn issue_1144_comment_html_block_spanning_a_blank_line_inside_a_blockquote() {
    let input = r#"> <!--
> a
>
> b
> -->
> c"#;
    let output = r#"<blockquote>
<!--
a

b
-->
<p>c</p>
</blockquote>"#;
    run(input, output);
}

#[test]
fn does_not_consume_the_space_so_two_trailing_spaces_still_form_a_hard_line_break_6_7() {
    let input = "a\\ \x20
b";
    let output = r#"<p>a\<br>
b</p>"#;
    run(input, output);
}

#[test]
fn literal_backslash_and_the_space_ends_the_destination() {
    let input = r#"[a](/url\ )"#;
    let output = r#"<p><a href="/url%5C">a</a></p>"#;
    run(input, output);
}

#[test]
fn backslash_before_a_space_in_a_link_destination_followed_by_a_title() {
    let input = r#"[a](/url\ "title")"#;
    let output = r#"<p><a href="/url%5C" title="title">a</a></p>"#;
    run(input, output);
}

#[test]
fn html_block_type_4_declaration_starts_with_plus_any_ascii_letter_not_only_uppercase_commonmark_4_6_start_condition_4_a_lowercase_declaration_must_interrupt_a_paragraph() {
    let input = r#"foo
<!bar baz>"#;
    let output = r#"<p>foo</p>
<!bar baz>"#;
    run(input, output);
}

#[test]
fn disturb_code_span_scanning_of_the_rest_of_the_line() {
    let input = r#"[foo `bar` baz`"#;
    let output = r#"<p>[foo <code>bar</code> baz`</p>"#;
    run(input, output);
}

#[test]
fn the_same_holds_for_an_unclosed_image_label_which_uses_the_same_lookahead() {
    let input = r#"![alt `code` x`"#;
    let output = r#"<p>![alt <code>code</code> x`</p>"#;
    run(input, output);
}

#[test]
fn valid_measuring_that_run_as_a_shorter_one_would_swallow_into_a_code_span() {
    let input = r#"[`](``)"#;
    let output = r#"<p><a href="%60%60">`</a></p>"#;
    run(input, output);
}

#[test]
fn same_with_the_run_left_outside_the_link_entirely() {
    let input = r#"[`a](/x)``b"#;
    let output = r#"<p><a href="/x">`a</a>``b</p>"#;
    run(input, output);
}

#[test]
fn the_label_cancels_the_link_commonmark_0_31_2_6_1() {
    let input = r#"[foo `bar](/url)` baz"#;
    let output = r#"<p>[foo <code>bar](/url)</code> baz</p>"#;
    run(input, output);
}

#[test]
fn no_stripping_when_a_code_span_is_all_spaces_commonmark_0_31_2_6_1() {
    let input = r#"`   `"#;
    let output = r#"<p><code>   </code></p>"#;
    run(input, output);
}
// end of auto-generated module
}
///////////////////////////////////////////////////////////////////////////
