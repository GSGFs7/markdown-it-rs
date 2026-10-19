use markdown_it::parser::block::builtin::BlockParserRule;
use markdown_it::parser::core::{CoreRule, CoreState};
use markdown_it::parser::inline::builtin::InlineParserRule;
use markdown_it::plugins::{cmark, extra, sourcepos};
use markdown_it::{EditBatch, MarkdownIt, Root, StructuralEvent, Text};

#[derive(Default)]
struct Observe<const POSITION: u8>;

impl<const POSITION: u8> CoreRule for Observe<POSITION> {
    fn run(&self, state: &mut CoreState<'_>) {
        let document = &*state.document;
        let has_blocks = !document.children(document.root()).is_empty();
        let has_text = document
            .events(document.root())
            .any(|event| matches!(event, StructuralEvent::Leaf(node) if node.is::<Text>()));
        assert_eq!(has_blocks, POSITION > 0);
        assert_eq!(has_text, POSITION > 1);
        state
            .root_ext_mut()
            .get_or_insert_default::<Vec<u8>>()
            .push(POSITION);
    }
}

#[derive(Default)]
struct ObserveSharedState;

impl CoreRule for ObserveSharedState {
    fn run(&self, state: &mut CoreState<'_>) {
        let document = &mut *state.document;
        let order = document
            .node(document.root())
            .cast::<Root>()
            .unwrap()
            .ext
            .get::<Vec<u8>>()
            .unwrap();
        let mut edits = EditBatch::new();
        edits.set_attribute(document.root(), "order", format!("{order:?}"));
        edits.commit(document);
    }
}

#[test]
fn rules_observe_the_actual_document_and_shared_state_at_their_position() {
    let mut md = MarkdownIt::new();
    // Register in reverse order to exercise dependencies.
    md.add_rule::<Observe<2>>().after::<InlineParserRule>();
    md.add_rule::<Observe<1>>()
        .after::<BlockParserRule>()
        .before::<InlineParserRule>();
    md.add_rule::<Observe<0>>().before::<BlockParserRule>();
    md.add_rule::<ObserveSharedState>()
        .after::<Observe<1>>()
        .before::<InlineParserRule>();
    for _ in 0..2 {
        let document = md.parse("**text**");
        assert_eq!(document.node(document.root()).attrs()[0].1, "[0, 1]");
        assert_eq!(
            document
                .node(document.root())
                .cast::<Root>()
                .unwrap()
                .ext
                .get::<Vec<u8>>()
                .unwrap(),
            &[0, 1, 2]
        );
    }
}

#[test]
fn builtin_transformations_follow_footnote_finalization_in_either_registration_order() {
    fn parser(reverse: bool) -> MarkdownIt {
        let mut md = MarkdownIt::empty();
        cmark::add(&mut md);
        if !reverse {
            extra::footnote::add(&mut md);
        }
        sourcepos::add(&mut md);
        extra::heading_anchors::add(&mut md);
        extra::smartquotes::add(&mut md);
        extra::typographer::add(&mut md);
        extra::tasklist::add(&mut md);
        if reverse {
            extra::footnote::add(&mut md);
        }
        md
    }
    let source = "# Title[^note]\n\n[^note]: # \"Note...\"\n\n    - [x] done";
    let expected = parser(false).render(source);
    assert!(expected.contains("footnotes"), "{expected}");
    assert!(expected.contains("id=\"note\""), "{expected}");
    assert!(expected.contains("“Note…”"), "{expected}");
    assert!(expected.contains("checkbox"), "{expected}");
    assert_eq!(parser(true).render(source), expected);
}

#[test]
fn core_rules_receive_document_source_and_current_parser_configuration() {
    use std::sync::{Arc, Mutex};

    struct ContextRule(Arc<Mutex<Vec<(String, bool)>>>);
    impl CoreRule for ContextRule {
        fn run(&self, state: &mut CoreState<'_>) {
            assert_eq!(state.source.as_ptr(), state.document.source().as_ptr());
            self.0
                .lock()
                .unwrap()
                .push((state.source.to_owned(), state.md.render_options.xhtml_out));
        }
    }

    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut md = MarkdownIt::empty();
    md.add_rule_instance(ContextRule(Arc::clone(&calls)));
    let document = md.parse("雪\0");
    assert!(document.node(document.root()).srcmap().is_some());
    md.render_options.xhtml_out = true;
    md.parse("雪\0");
    assert_eq!(
        *calls.lock().unwrap(),
        [("雪\u{FFFD}".into(), false), ("雪\u{FFFD}".into(), true)]
    );
}
