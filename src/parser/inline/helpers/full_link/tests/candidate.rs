use super::*;

#[test]
fn candidate_combines_label_target_and_reference() {
    let mut document = crate::document::Document::new("", crate::document::Root::new(""));
    let md = parser::<false>();
    let ruleset = md.inline.rules();
    let mut references = ReferenceMap::default();
    references.insert("x".into(), "/shortcut".into(), None);
    references.insert("ref".into(), "/ref".into(), Some("t".into()));

    for (source, href, title, end) in [
        ("[x](/inline)", "/inline", None, 12),
        ("[x][ref]", "/ref", Some("t"), 8),
        ("[x][]", "/shortcut", None, 5),
        ("[x]", "/shortcut", None, 3),
        ("[x][", "/shortcut", None, 3),
        ("[x](/unfinished", "/shortcut", None, 3),
    ] {
        let mut context =
            InlineState::new(&mut document, source, 0, source.len(), &md, ruleset, 0, 0);
        let result = scan_link_candidate(
            &mut context,
            0,
            false,
            Some(&references),
            InlineTargetFallback::Reference,
        )
        .expect(source);
        assert_eq!((result.label_start, result.label_end), (1, 2));
        assert_eq!(result.href.as_deref(), Some(href), "{source}");
        assert_eq!(result.title.as_deref(), title, "{source}");
        assert_eq!(result.end, end, "{source}");
        assert_eq!(context.remaining(), source);
        assert_eq!(context.trailing_text(), "");
        assert_eq!(context.link_level, 0);
    }

    for source in [
        "[x][missing]",
        "[missing]",
        "[unclosed",
        "[x](",
        "[x]( \t\n",
    ] {
        let mut context =
            InlineState::new(&mut document, source, 0, source.len(), &md, ruleset, 0, 0);
        assert!(
            scan_link_candidate(
                &mut context,
                0,
                false,
                Some(&references),
                InlineTargetFallback::Reference
            )
            .is_none()
        );
    }
}

#[test]
fn candidate_offsets_are_relative_to_current_window() {
    let mut document = crate::document::Document::new("", crate::document::Root::new(""));
    let md = parser::<false>();
    let ruleset = md.inline.rules();
    let source = "前雪[x](/url)";
    let mut context = InlineState::new(
        &mut document,
        source,
        "前".len(),
        source.len(),
        &md,
        ruleset,
        0,
        0,
    );
    let result = scan_link_candidate(
        &mut context,
        "雪".len(),
        false,
        None,
        InlineTargetFallback::Reference,
    )
    .unwrap();
    assert_eq!(
        (result.label_start, result.label_end, result.end),
        (4, 5, 12)
    );
    assert_eq!(result.href.as_deref(), Some("/url"));
    assert_eq!(context.remaining(), "雪[x](/url)");
    assert!(
        scan_link_candidate(
            &mut context,
            1,
            false,
            None,
            InlineTargetFallback::Reference
        )
        .is_none()
    );
}

#[test]
fn candidate_combines_opaque_labels_with_target() {
    let mut document = crate::document::Document::new("", crate::document::Root::new(""));
    let md = parser::<false>();
    let ruleset = md.inline.rules();
    for label in LABELS {
        let source = format!("[{label}](/url)");
        let mut context =
            InlineState::new(&mut document, &source, 0, source.len(), &md, ruleset, 0, 0);
        let result = scan_link_candidate(
            &mut context,
            0,
            false,
            None,
            InlineTargetFallback::Reference,
        )
        .unwrap_or_else(|| panic!("{source}"));
        // `]` bytes inside opaque spans must not shorten the label.
        assert_eq!(
            (result.label_start, result.label_end),
            (1, label.len() + 1),
            "{source}"
        );
        assert_eq!(result.href.as_deref(), Some("/url"), "{source}");
        assert_eq!(result.title, None, "{source}");
        assert_eq!(result.end, source.len(), "{source}");
    }
}

#[test]
fn candidate_requires_reference_map_and_label_window() {
    let mut document = crate::document::Document::new("", crate::document::Root::new(""));
    let md = parser::<false>();
    let ruleset = md.inline.rules();

    // Inline targets parse without references, shortcut references do not.
    let source = "[x](/url)";
    let mut context = InlineState::new(&mut document, source, 0, source.len(), &md, ruleset, 0, 0);
    let result = scan_link_candidate(
        &mut context,
        0,
        false,
        None,
        InlineTargetFallback::Reference,
    )
    .expect(source);
    assert_eq!(result.href.as_deref(), Some("/url"));
    assert_eq!(result.end, source.len());

    for source in ["[x]", "[x][ref]"] {
        let mut context =
            InlineState::new(&mut document, source, 0, source.len(), &md, ruleset, 0, 0);
        assert!(
            scan_link_candidate(
                &mut context,
                0,
                false,
                None,
                InlineTargetFallback::Reference
            )
            .is_none(),
            "{source}"
        );
    }

    // Label scanning consumes one nesting level; when the budget is spent
    // the whole candidate is rejected even though the syntax is valid.
    let mut md = parser::<false>();
    md.max_nesting = 2;
    let ruleset = md.inline.rules();
    let source = "[x](/url)";
    let mut root = InlineState::new(&mut document, source, 0, source.len(), &md, ruleset, 0, 0);
    assert!(
        scan_link_candidate(&mut root, 0, false, None, InlineTargetFallback::Reference).is_some()
    );

    let mut nested = root
        .child_state_at(
            0..source.len(),
            root.depth.saturating_add(2),
            root.link_level,
        )
        .unwrap();
    assert_eq!(nested.depth, 2);
    assert!(
        scan_link_candidate(&mut nested, 0, false, None, InlineTargetFallback::Reference).is_none()
    );
}

#[test]
fn candidate_respects_window_edges_and_parent_cursor() {
    let mut document = crate::document::Document::new("", crate::document::Root::new(""));
    let md = parser::<false>();
    let ruleset = md.inline.rules();

    // The full source parses, but the window ends before the closing
    // parenthesis; bytes after `remaining()` must stay invisible.
    let source = "[x](/url)tail";
    let close = source.find(')').unwrap();
    let mut truncated = InlineState::new(&mut document, source, 0, close, &md, ruleset, 0, 0);
    assert_eq!(truncated.remaining(), "[x](/url");
    assert!(
        scan_link_candidate(
            &mut truncated,
            0,
            false,
            None,
            InlineTargetFallback::Reference
        )
        .is_none()
    );

    // Including the closing parenthesis makes the same window parse.
    let mut complete = InlineState::new(&mut document, source, 0, close + 1, &md, ruleset, 0, 0);
    let result = scan_link_candidate(
        &mut complete,
        0,
        false,
        None,
        InlineTargetFallback::Reference,
    )
    .unwrap();
    assert_eq!(result.end, close + 1);

    // After the parent cursor advances, results stay relative to
    // `remaining()`, not to the start of the session.
    let source = "前雪[x](/url)";
    let mut context = InlineState::new(
        &mut document,
        source,
        "前".len(),
        source.len(),
        &md,
        ruleset,
        0,
        0,
    );
    context.skip_token().unwrap();
    assert_eq!(context.remaining(), "[x](/url)");
    let result = scan_link_candidate(
        &mut context,
        0,
        false,
        None,
        InlineTargetFallback::Reference,
    )
    .unwrap();
    assert_eq!(
        (result.label_start, result.label_end, result.end),
        (1, 2, 9)
    );
    assert_eq!(result.href.as_deref(), Some("/url"));
    assert_eq!(context.remaining(), "[x](/url)");
}

#[test]
fn failed_inline_target_probes_reference_after_a_whole_utf8_character() {
    let mut document = crate::document::Document::new("", crate::document::Root::new(""));
    let md = parser::<false>();
    let ruleset = md.inline.rules();
    let mut references = ReferenceMap::default();
    references.insert("r".into(), "/url".into(), None);

    // The title probe stops on the emoji, so the reference probe must advance
    // past the complete character (not a UTF-16 code unit) to see `[r]`.
    let source = "[x]( <u> 🦀[r]";
    let mut context = InlineState::new(&mut document, source, 0, source.len(), &md, ruleset, 0, 0);
    let result = scan_link_candidate(
        &mut context,
        0,
        false,
        Some(&references),
        InlineTargetFallback::Reference,
    )
    .expect(source);
    assert_eq!(result.href.as_deref(), Some("/url"));
    assert_eq!(result.end, source.len());
}
