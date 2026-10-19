mod common;

fn run(input: &str, output: &str) {
    let expected = if output.is_empty() {
        String::new()
    } else {
        output.to_owned() + "\n"
    };

    let md = common::markdown_it_fixture_parser();
    let source = input.to_owned() + "\n";
    let actual = md.render(&source);
    assert_eq!(actual, expected);

    let document = md.parse(&source);
    assert_eq!(md.render_html(&document), actual);
    md.render_as(&document, "text");
}

#[test]
fn short_delimiter_failures_do_not_hide_longer_strikethrough_matches() {
    let mut md = markdown_it::MarkdownIt::with_preset(markdown_it::Preset::MarkdownItDefault);
    md.max_nesting = 3;
    for (source, expected) in [
        ("-~~~~;~\n文~~~~\n", "<p>-<s><s>;~\n文</s></s></p>\n"),
        ("~~a~ b~~", "<p><s>a~ b</s></p>\n"),
        ("~~a~ b~~~", "<p><s>a~ b</s>~</p>\n"),
        ("-~~~~;~\n文~~~~尾", "<p>-<s><s>;~\n文</s></s>尾</p>\n"),
    ] {
        assert_eq!(md.render(source), expected, "{source:?}");
    }
}

#[test]
fn strikethrough_does_not_apply_the_emphasis_rule_of_three() {
    for (source, expected) in [
        ("-~~~~;~~", "<p>-~~<s>;</s></p>"),
        ("-~~;~~~~", "<p>-<s>;</s>~~</p>"),
        ("-~~~~~;~~", "<p>-~~~<s>;</s></p>"),
        ("-~~~~;~~~", "<p>-~~<s>;</s>~</p>"),
        ("-~~~~;~~~~", "<p>-<s><s>;</s></s></p>"),
        ("a~~~~b~~", "<p>a~~<s>b</s></p>"),
        ("a~~b~~~~", "<p>a<s>b</s>~~</p>"),
        ("-****;**", "<p>-****;**</p>"),
        ("-____;__", "<p>-____;__</p>"),
    ] {
        run(source, expected);
    }
}

#[test]
fn partially_matched_strikethrough_keeps_source_offsets() {
    let md = markdown_it::MarkdownIt::with_preset(markdown_it::Preset::MarkdownItDefault);
    let source = "雪 -~~~~;~~ 尾";
    let document = md.parse(source);
    let strike = document
        .events(document.root())
        .find_map(|event| {
            let node = event.node();
            node.is::<markdown_it::plugins::extra::strikethrough::Strikethrough>()
                .then_some(node)
        })
        .unwrap();
    #[cfg(feature = "sourcemap")]
    let content_start = source.find(';').unwrap();
    #[cfg(feature = "sourcemap")]
    assert_eq!(
        strike.srcmap().unwrap().get_byte_offsets(),
        (content_start - 2, content_start + 3)
    );
    let text = document.node(strike.children()[0]);
    assert_eq!(text.cast::<markdown_it::Text>().unwrap().content, ";");
    #[cfg(feature = "sourcemap")]
    assert_eq!(
        text.srcmap().unwrap().get_byte_offsets(),
        (content_start, content_start + 1)
    );
    assert_eq!(md.render_html(&document), "<p>雪 -~~<s>;</s> 尾</p>\n");
}
