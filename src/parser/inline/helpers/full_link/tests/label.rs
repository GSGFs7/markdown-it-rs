use super::*;

#[test]
fn labels_respect_raw_brackets_and_opaque_spans() {
    let md = parser::<false>();
    for label in LABELS {
        let source = format!("@[{label}]");
        assert_eq!(
            render(&md, &source),
            format!("<p>end={}</p>\n", label.len()),
            "{source}"
        );
    }
    assert_eq!(render(&md, "@[a[b]"), "<p>none</p>\n");
    assert_eq!(render(&md, "@[abc"), "<p>none</p>\n");
}

#[test]
fn terminal_close_precedes_dispatch_and_does_not_scan_suffix() {
    let mut md = parser::<false>();
    md.inline.add_rule::<Closing>();
    md.inline.add_rule::<DefaultCheck>();
    assert_eq!(render(&md, "@[x]?"), "<p>end=1</p>\n");
    assert_eq!(render(&md, "@[?]"), "<p>end=1</p>\n");
}

#[test]
fn nested_flag_preserves_consumption_rule() {
    let mut no = parser::<false>();
    no.inline.add_rule::<BracketToken>();
    assert_eq!(render(&no, "@[[x]]"), "<p>none</p>\n");
    let mut yes = parser::<true>();
    yes.inline.add_rule::<BracketToken>();
    assert_eq!(render(&yes, "@[[x]]"), "<p>end=3</p>\n");
}

#[test]
fn recursive_entry_at_depth_limit_yields_no_label() {
    let mut md = parser::<false>();
    md.max_nesting = 1;
    // An immediately closing bracket does not call `skipToken`, so it is
    // still accepted at the nesting limit.
    assert_eq!(render(&md, "@[]"), "<p>end=0</p>\n");
    // A non-empty label must be scanned with `skipToken`, which records
    // the failure once the nesting limit is reached.
    assert_eq!(render(&md, "@[x]"), "<p>none</p>\n");
}

#[test]
fn registered_link_and_image_check_respect_remaining_depth() {
    let mut document = crate::document::Document::new("", crate::document::Root::new(""));
    let mut md = MarkdownIt::empty();
    crate::plugins::cmark::inline::link::add(&mut md);
    crate::plugins::cmark::inline::image::add(&mut md);
    md.max_nesting = 3;
    let rules = md.inline.rules();

    for source in ["[雪](/url)", "[](/url)", "![雪](/img)", "![](/img)"] {
        for depth in [1, 2, 3] {
            let mut context =
                InlineState::new(&mut document, source, 0, source.len(), &md, rules, depth, 0);
            let result = if source.starts_with('!') {
                <LinkPrefixScanner<'!', true> as InlineRule>::check(&mut context)
            } else {
                <LinkScanner<false> as InlineRule>::check(&mut context)
            };
            // `depth` is the rule level of the check; a non-empty label
            // is rejected once that level reaches `maxNesting`. An empty
            // label closes before `skipToken` and is still accepted.
            let empty_label = source.starts_with("[]") || source.starts_with("![]");
            let expected = if depth < 3 || empty_label {
                Some(source.len())
            } else {
                None
            };
            assert_eq!(result, expected, "depth={depth}, source={source:?}");
            assert_eq!(context.remaining(), source);
            assert_eq!(context.depth, depth);
            assert_eq!(context.link_level, 0);
        }
    }
}

#[test]
fn label_scan_checks_unicode_prefixed_rule() {
    // Image label scanning calls the custom rule's check before its run.
    // The nested custom link contributes its text to the image alt value.
    assert_eq!(
        utf8_parser::<'雪'>().render("![雪[x](/inner)](/image)"),
        "<p><img src=\"/image\" alt=\"x\"></p>\n",
    );
}
