fn run(input: &str, output: &str) {
    let output = if output.is_empty() {
        "".to_owned()
    } else {
        output.to_owned() + "\n"
    };

    let md = &mut markdown_it::MarkdownIt::new();
    markdown_it::plugins::cmark::add(md);
    markdown_it::plugins::html::add(md);
    markdown_it::plugins::extra::math::add(md);

    let node = md.parse(&(input.to_owned() + "\n"));
    node.walk(|node, _| assert!(node.srcmap.is_some()));

    // fix 'style' attrs order in katex
    fn normalize_styles(html: &str) -> String {
        let re = regex::Regex::new(r#"style="([^"]+)""#).unwrap();
        re.replace_all(html, |caps: &regex::Captures| {
            let mut styles: Vec<&str> = caps[1]
                .split(';')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            styles.sort();
            format!(r#"style="{}""#, styles.join("; ") + ";")
        })
        .into_owned()
    }

    let actual = normalize_styles(&node.render());
    let expected = normalize_styles(&output);
    assert_eq!(actual, expected);

    let _ = md.parse(input.trim_end());
}

#[test]
#[cfg(not(feature = "katex"))]
fn math_block_multiline() {
    let input = r#"$$
E=mc^2
$$"#;

    let output = r#"<div class="math-block">E=mc^2</div>"#;

    run(input, output)
}

#[test]
#[cfg(feature = "katex")]
fn math_block_multiline() {
    let input = r#"$$
E=mc^2
$$"#;

    let output = r#"<div class="math-block"><span class="katex"><span class="katex-mathml"><math xmlns="http://www.w3.org/1998/Math/MathML"><semantics><mrow><mi>E</mi><mo>=</mo><mi>m</mi><msup><mi>c</mi><mn>2</mn></msup></mrow><annotation encoding="application/x-tex">E=mc^2</annotation></semantics></math></span><span class="katex-html" aria-hidden="true"><span class="base"><span class="strut" style="height:0.6833em;"></span><span class="mord mathnormal" style="margin-right:0.0576em;">E</span><span class="mspace" style="margin-right:0.2778em;"></span><span class="mrel">=</span><span class="mspace" style="margin-right:0.2778em;"></span></span><span class="base"><span class="strut" style="height:0.8141em;"></span><span class="mord mathnormal">m</span><span class="mord"><span class="mord mathnormal">c</span><span class="msupsub"><span class="vlist-t"><span class="vlist-r"><span class="vlist" style="height:0.8141em;"><span style="margin-right:0.05em;top:-3.063em;"><span class="pstrut" style="height:2.7em;"></span><span class="sizing reset-size6 size3 mtight"><span class="mord mtight">2</span></span></span></span></span></span></span></span></span></span></span></div>"#;

    run(input, output)
}

#[test]
#[cfg(not(feature = "katex"))]
fn math_block_with_empty_line() {
    let input = r#"$$

E=mc^2


$$"#;

    let output = r#"<div class="math-block">E=mc^2</div>"#;

    run(input, output)
}

#[test]
#[cfg(feature = "katex")]
fn math_block_with_empty_line() {
    let input = r#"$$

E=mc^2


$$"#;

    let output = r#"<div class="math-block"><span class="katex"><span class="katex-mathml"><math xmlns="http://www.w3.org/1998/Math/MathML"><semantics><mrow><mi>E</mi><mo>=</mo><mi>m</mi><msup><mi>c</mi><mn>2</mn></msup></mrow><annotation encoding="application/x-tex">E=mc^2</annotation></semantics></math></span><span class="katex-html" aria-hidden="true"><span class="base"><span class="strut" style="height:0.6833em;"></span><span class="mord mathnormal" style="margin-right:0.0576em;">E</span><span class="mspace" style="margin-right:0.2778em;"></span><span class="mrel">=</span><span class="mspace" style="margin-right:0.2778em;"></span></span><span class="base"><span class="strut" style="height:0.8141em;"></span><span class="mord mathnormal">m</span><span class="mord"><span class="mord mathnormal">c</span><span class="msupsub"><span class="vlist-t"><span class="vlist-r"><span class="vlist" style="height:0.8141em;"><span style="top:-3.063em;margin-right:0.05em;"><span class="pstrut" style="height:2.7em;"></span><span class="sizing reset-size6 size3 mtight"><span class="mord mtight">2</span></span></span></span></span></span></span></span></span></span></span></div>"#;

    run(input, output)
}

#[test]
#[cfg(not(feature = "katex"))]
fn math_inline() {
    let input = r#"$E=mc^2$"#;

    let output = r#"<p><span class="math-inline">E=mc^2</span></p>"#;

    run(input, output)
}

#[test]
#[cfg(feature = "katex")]
fn math_inline() {
    let input = r#"$E=mc^2$"#;

    let output = r#"<p><span class="math-inline"><span class="katex"><span class="katex-mathml"><math xmlns="http://www.w3.org/1998/Math/MathML"><semantics><mrow><mi>E</mi><mo>=</mo><mi>m</mi><msup><mi>c</mi><mn>2</mn></msup></mrow><annotation encoding="application/x-tex">E=mc^2</annotation></semantics></math></span><span class="katex-html" aria-hidden="true"><span class="base"><span class="strut" style="height:0.6833em;"></span><span class="mord mathnormal" style="margin-right:0.0576em;">E</span><span class="mspace" style="margin-right:0.2778em;"></span><span class="mrel">=</span><span class="mspace" style="margin-right:0.2778em;"></span></span><span class="base"><span class="strut" style="height:0.8141em;"></span><span class="mord mathnormal">m</span><span class="mord"><span class="mord mathnormal">c</span><span class="msupsub"><span class="vlist-t"><span class="vlist-r"><span class="vlist" style="height:0.8141em;"><span style="margin-right:0.05em; top:-3.063em;"><span class="pstrut" style="height:2.7em;"></span><span class="sizing reset-size6 size3 mtight"><span class="mord mtight">2</span></span></span></span></span></span></span></span></span></span></span></span></p>"#;

    run(input, output)
}

#[test]
#[cfg(not(feature = "katex"))]
fn math_inline_mixed() {
    let input = r#"something$E=mc^2$something"#;

    let output = r#"<p>something<span class="math-inline">E=mc^2</span>something</p>"#;

    run(input, output)
}

#[test]
#[cfg(feature = "katex")]
fn math_inline_mixed() {
    let input = r#"something$E=mc^2$something"#;

    let output = r#"<p>something<span class="math-inline"><span class="katex"><span class="katex-mathml"><math xmlns="http://www.w3.org/1998/Math/MathML"><semantics><mrow><mi>E</mi><mo>=</mo><mi>m</mi><msup><mi>c</mi><mn>2</mn></msup></mrow><annotation encoding="application/x-tex">E=mc^2</annotation></semantics></math></span><span class="katex-html" aria-hidden="true"><span class="base"><span class="strut" style="height:0.6833em;"></span><span class="mord mathnormal" style="margin-right:0.0576em;">E</span><span class="mspace" style="margin-right:0.2778em;"></span><span class="mrel">=</span><span class="mspace" style="margin-right:0.2778em;"></span></span><span class="base"><span class="strut" style="height:0.8141em;"></span><span class="mord mathnormal">m</span><span class="mord"><span class="mord mathnormal">c</span><span class="msupsub"><span class="vlist-t"><span class="vlist-r"><span class="vlist" style="height:0.8141em;"><span style="margin-right:0.05em;top:-3.063em;"><span class="pstrut" style="height:2.7em;"></span><span class="sizing reset-size6 size3 mtight"><span class="mord mtight">2</span></span></span></span></span></span></span></span></span></span></span></span>something</p>"#;

    run(input, output)
}

#[test]
fn math_inline_with_spaces_not_allowed() {
    let input = r#"$ E=mc^2 $"#;
    let output = r#"<p>$ E=mc^2 $</p>"#;
    run(input, output);

    let input = r#"$E=mc^2 $"#;
    let output = r#"<p>$E=mc^2 $</p>"#;
    run(input, output);

    let input = r#"$ E=mc^2$"#;
    let output = r#"<p>$ E=mc^2$</p>"#;
    run(input, output);
}

#[test]
fn math_inline_with_digit_after_closing() {
    let input = r#"$10 to $20"#;
    let output = r#"<p>$10 to $20</p>"#;
    run(input, output);

    let input = r#"$E=mc^2$1"#;
    let output = r#"<p>$E=mc^2$1</p>"#;
    run(input, output);
}
