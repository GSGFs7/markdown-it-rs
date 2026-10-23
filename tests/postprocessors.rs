#[cfg(feature = "sourcemap")]
use markdown_it::Root;
use markdown_it::parser::core::{CoreRule, CoreState};
#[cfg(feature = "sourcemap")]
use markdown_it::plugins::sourcepos;
use markdown_it::plugins::{cmark, extra};
use markdown_it::{Document, EditBatch, MarkdownIt, NodeDraft, StructuralEvent, Text};

#[test]
fn typographer_precedes_custom_smartquotes_in_reverse_registration_order() {
    let mut md = MarkdownIt::new();
    extra::smartquotes::add_with::<'‹', '›', '«', '»'>(&mut md);
    extra::typographer::add(&mut md);
    assert_eq!(md.render("\"...\" '雪' (TM)"), "<p>«…» ‹雪› ™</p>\n");
}

#[test]
#[cfg(feature = "sourcemap")]
fn sourcepos_runs_after_tasklist_in_reverse_registration_order() {
    let mut md = MarkdownIt::new();
    sourcepos::add(&mut md);
    extra::tasklist::add(&mut md);
    md.render_options.xhtml_out = true;
    let doc = md.parse("- [x] 雪");
    let text = doc
        .events(doc.root())
        .find_map(|event| {
            if let StructuralEvent::Leaf(node) = event {
                node.cast::<Text>()
                    .filter(|text| text.content == "雪")
                    .map(|_| node)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(text.srcmap().unwrap().get_byte_offsets(), (6, 9));
    let html = md.render_html(&doc);
    assert!(
        html.contains("type=\"checkbox\" checked=\"\" /> 雪"),
        "{html}"
    );
    assert!(html.contains("data-sourcepos=\"1:1-1:7\""), "{html}");
    assert_eq!(md.render_as(&doc, "text"), "雪\n");
}

#[test]
#[cfg(feature = "sourcemap")]
fn sourcepos_deduplicates_existing_attributes_and_skips_generated_nodes() {
    let source = "# 雪";
    let mut root = NodeDraft::new(Root::new(source));
    let mut heading = NodeDraft::new(cmark::block::heading::ATXHeading { level: 1 });
    heading.set_srcmap(Some(markdown_it::common::sourcemap::SourcePos::new(
        0,
        source.len(),
    )));
    heading.attrs_mut().extend([
        ("data-sourcepos".into(), "stale".into()),
        ("data-sourcepos".into(), "duplicate".into()),
    ]);
    heading.push_child(NodeDraft::new(Text {
        content: "雪".into(),
    }));
    root.push_child(heading);
    let mut doc = Document::from_draft(source, root);
    sourcepos::edits(&doc).commit(&mut doc);
    let heading = doc.node(doc.children(doc.root())[0]);
    assert_eq!(
        heading.attrs(),
        &[("data-sourcepos".into(), "1:1-1:3".into())]
    );
    let generated = doc.node(heading.children()[0]);
    assert!(generated.srcmap().is_none());
    assert!(generated.attrs().is_empty());
}

#[test]
fn text_fallback_runs_typographer_and_custom_smartquotes() {
    let mut md = MarkdownIt::empty();
    extra::smartquotes::add_with::<'‹', '›', '«', '»'>(&mut md);
    extra::typographer::add(&mut md);
    assert_eq!(md.render("\"雪...\""), "«雪…»\n");
    assert_eq!(md.render(""), "");
}

#[test]
fn draft_drop_and_arena_insertion_handle_deep_generated_trees() {
    fn tree() -> NodeDraft {
        let mut node = NodeDraft::new(Text {
            content: "leaf".into(),
        });
        for _ in 0..50_000 {
            let mut parent = NodeDraft::new(cmark::block::paragraph::Paragraph);
            parent.push_child(node);
            node = parent;
        }
        node
    }
    drop(tree());
    let doc = Document::from_draft("", tree());
    assert_eq!(doc.len(), 50_001);
    assert_eq!(
        doc.events(doc.root())
            .filter(|event| matches!(event, StructuralEvent::Leaf(_)))
            .count(),
        1
    );
}

#[test]
fn appended_subtrees_keep_parent_links_and_existing_node_ids() {
    let md = MarkdownIt::new();
    let mut doc = md.parse("first");
    let root = doc.root();
    let first = doc.children(root)[0];
    let appended = doc.append_child(
        root,
        NodeDraft::new(Text {
            content: "second".into(),
        }),
    );
    assert_eq!(doc.children(root), &[first, appended]);
    assert_eq!(doc.parent(appended), Some(root));
    assert_eq!(md.render_html(&doc), "<p>first</p>\nsecond");
}

struct ConfiguredPostProcess {
    value: String,
}

impl CoreRule for ConfiguredPostProcess {
    fn run(&self, state: &mut CoreState<'_>) {
        let document = &mut *state.document;
        let mut edits = EditBatch::new();
        edits.set_attribute(document.root(), "configured", self.value.clone());
        edits.commit(document);
    }
}

#[test]
fn configured_core_rule_instance_modifies_the_document() {
    let mut md = MarkdownIt::empty();
    md.add_rule_instance(ConfiguredPostProcess {
        value: "runtime value".into(),
    });

    let document = md.parse("");
    assert_eq!(
        document.node(document.root()).attrs(),
        &[("configured".into(), "runtime value".into())]
    );
}

#[derive(Default)]
struct AppendStep<const STEP: u8>;

impl<const STEP: u8> CoreRule for AppendStep<STEP> {
    const NAMES: &'static [&'static str] = if STEP == 1 { &["opening"] } else { &[] };

    fn run(&self, state: &mut CoreState<'_>) {
        let document = &mut *state.document;
        let mut edits = EditBatch::new();
        edits.set_attribute(
            document.root(),
            "steps",
            format!("{}{STEP}", steps(document)),
        );
        edits.commit(document);
    }
}

fn steps(document: &Document) -> &str {
    document
        .node(document.root())
        .attrs()
        .iter()
        .find(|(name, _)| name == "steps")
        .map_or("", |(_, value)| value.as_str())
}

#[test]
fn core_rules_run_once_per_parse_and_rendering_does_not_repeat_them() {
    let mut md = MarkdownIt::empty();
    md.add_rule::<AppendStep<1>>();
    for commonmark in [false, true] {
        if commonmark {
            cmark::add(&mut md);
        }
        for source in ["", "text"] {
            let document = md.parse(source);
            assert_eq!(steps(&document), "1");
            md.render_html(&document);
            assert_eq!(steps(&document), "1");
            assert_eq!(steps(&md.parse(source)), "1");
        }
    }
}

#[test]
fn core_rules_observe_preceding_edits_in_type_and_named_order() {
    let mut md = MarkdownIt::empty();
    md.add_rule::<AppendStep<2>>().after::<AppendStep<1>>();
    md.add_rule::<AppendStep<3>>().after_named("opening");
    md.add_rule::<AppendStep<1>>().before_all();
    assert_eq!(steps(&md.parse("text")), "123");
}

#[derive(Default)]
struct FinalizeSteps;

impl CoreRule for FinalizeSteps {
    fn run(&self, state: &mut CoreState<'_>) {
        let document = &mut *state.document;
        let value = format!("{}F", steps(document));
        let mut edits = EditBatch::new();
        edits.set_attribute(document.root(), "steps", value);
        edits.commit(document);
    }
}

#[test]
fn document_transformations_execute_in_ruler_order() {
    use markdown_it::parser::block::BlockParserRule;
    use markdown_it::parser::inline::InlineParserRule;

    let mut md = MarkdownIt::empty();
    md.add_rule::<AppendStep<1>>().before::<BlockParserRule>();
    md.add_rule::<AppendStep<2>>()
        .after::<AppendStep<1>>()
        .before::<FinalizeSteps>();
    md.add_rule::<FinalizeSteps>().after::<InlineParserRule>();
    for commonmark in [false, true] {
        if commonmark {
            cmark::add(&mut md);
        }
        for source in ["", "text"] {
            assert_eq!(steps(&md.parse(source)), "12F");
        }
    }
}

#[test]
fn warmed_core_ruler_observes_addition_ordering_and_removal() {
    let mut md = MarkdownIt::empty();
    md.parse("");
    md.add_rule::<AppendStep<1>>();
    let document = md.parse("");
    assert_eq!(steps(&document), "1");

    md.add_rule::<AppendStep<2>>().before_named("opening");
    assert_eq!(steps(&md.parse("text")), "21");
    md.remove_rule::<AppendStep<1>>();
    assert!(!md.has_rule::<AppendStep<1>>());
    assert_eq!(steps(&md.parse("text")), "2");
    md.remove_rule::<AppendStep<2>>();
    assert_eq!(steps(&document), "1");
    assert_eq!(steps(&md.parse("text")), "");
}

#[test]
fn duplicate_configured_rule_is_rejected_and_can_be_explicitly_replaced() {
    let mut md = MarkdownIt::empty();
    md.add_rule_instance(ConfiguredPostProcess {
        value: "first".into(),
    });
    md.parse(""); // Warm the ruler cache.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        md.add_rule_instance(ConfiguredPostProcess {
            value: "second".into(),
        });
    }));
    assert!(result.is_err(), "duplicate registration must be rejected");
    let document = md.parse("");
    assert_eq!(
        document.node(document.root()).attrs(),
        &[("configured".into(), "first".into())]
    );

    md.remove_rule::<ConfiguredPostProcess>();
    md.add_rule_instance(ConfiguredPostProcess {
        value: "second".into(),
    });
    let document = md.parse("");
    assert_eq!(
        document.node(document.root()).attrs(),
        &[("configured".into(), "second".into())]
    );
}

#[test]
#[should_panic(expected = "core rule type is already registered")]
fn registering_a_type_already_used_as_an_alias_is_rejected() {
    let mut md = MarkdownIt::empty();
    md.add_rule::<AppendStep<1>>().alias::<AppendStep<2>>();
    md.add_rule::<AppendStep<2>>();
}

#[test]
fn parser_and_document_are_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<MarkdownIt>();
    assert_send_sync::<Document>();
}
