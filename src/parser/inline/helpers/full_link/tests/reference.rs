use super::*;

#[test]
fn reference_candidate_selects_label_and_consumed_end() {
    let mut references = ReferenceMap::default();
    references.insert("a".into(), "/shortcut".into(), None);
    references.insert("b".into(), "/full".into(), Some("title".into()));

    for (source, reference_end, href, title, end) in [
        ("[a]", None, "/shortcut", None, 3),
        ("[a][]", Some(4), "/shortcut", None, 5),
        ("[a][b]", Some(5), "/full", Some("title"), 6),
        ("[a][", None, "/shortcut", None, 3),
        ("[a](/unfinished", None, "/shortcut", None, 3),
    ] {
        let result =
            resolve_reference_link(source, 1, 2, reference_end.map(|end| (4, end)), &references)
                .expect(source);
        assert_eq!((result.label_start, result.label_end), (1, 2));
        assert_eq!(result.href.as_deref(), Some(href), "{source}");
        assert_eq!(result.title.as_deref(), title, "{source}");
        assert_eq!(result.end, end, "{source}");
    }

    // A missing explicit reference must not fall back to the first label.
    assert!(resolve_reference_link("[a][missing]", 1, 2, Some((4, 11)), &references).is_none());
}
