//! Inline wrapping plugin.
//!
//! Consumes `%` and records the completed node range of the current inline
//! session in an `on_finish` callback. A core rule then wraps that range in a
//! [`Wrapped`] node after inline parsing. This plugin changes the AST without
//! defining an HTML renderer.
//!
//! # What it demonstrates
//!
//! - Recording work in `on_finish` after pending Unicode text has become nodes.
//! - Discovering callback data from a later core rule via `Document`.
//! - Replacing sibling ranges while preserving node order and parent links.
//! - Scheduling the wrapper between inline parsing and dependent core rules.
//!
//! # Test coverage
//!
//! - `finish_callback_wraps_pending_unicode_text`: finish callbacks include
//!   pending Unicode text.
//! - `wrapping_preserves_order_and_existing_subtrees`: wrapping keeps node
//!   order, existing subtrees, and parent relationships.
//! - `finish_callback_stays_in_its_inline_session`: callbacks affect only their
//!   own inline session and leave other paragraphs unchanged.
//! - `empty_sessions_do_not_create_wrappers_and_repeated_requests_coalesce`:
//!   empty sessions create no wrappers; repeated requests coalesce into one
//!   wrapper and leave no temporary marker behind.
//! - `wrapping_is_scheduled_after_inline_and_before_dependent_core_rules`: rule
//!   dependencies order wrapping after inline parsing and before dependent core
//!   rules, regardless of registration order.

use markdown_it::MarkdownIt;
use markdown_it::document::NodeId;
use markdown_it::parser::core::{CoreRule, CoreState};
use markdown_it::parser::inline::{InlineParserRule, InlineRule, InlineState};

#[derive(Debug)]
pub struct Wrapped;

pub struct WrapAtFinish;

#[derive(Debug)]
struct PendingWrap(NodeId);

/// Install both stages; wrapping becomes visible after inline parsing finishes.
pub fn install(md: &mut MarkdownIt) {
    md.inline.add_rule::<WrapAtFinish>();
    md.add_rule::<ApplyWraps>().after::<InlineParserRule>();
}

impl InlineRule for WrapAtFinish {
    const MARKER: char = '%';

    fn check(state: &mut InlineState<'_>) -> Option<usize> {
        state.remaining().starts_with('%').then_some(1)
    }

    fn run(state: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        let len = Self::check(state)?;
        state.on_finish(record_wrap);
        Some((None, len))
    }
}

fn record_wrap(state: &mut InlineState<'_>) {
    let (Some(&first), Some(&last)) = (state.nodes().first(), state.nodes().last()) else {
        return;
    };
    // Record on the first node rather than in source-wide scratch: core rules
    // can discover it after the session's nodes attach to their parent.
    state
        .document
        .node_mut(first)
        .ext_mut()
        .insert(PendingWrap(last));
}

#[derive(Default)]
struct ApplyWraps;

impl CoreRule for ApplyWraps {
    fn run(&self, state: &mut CoreState<'_>) {
        let document = &mut *state.document;
        let mut stack = vec![document.root()];
        let mut ranges = Vec::new();
        while let Some(id) = stack.pop() {
            stack.extend(document.children(id).iter().rev().copied());
            if let Some(range) = document.node_mut(id).ext_mut().remove::<PendingWrap>() {
                ranges.push((id, range.0));
            }
        }
        for (first, last) in ranges {
            let parent = document.parent(first).expect("inline nodes are attached");
            let siblings = document.children(parent);
            let start = siblings.iter().position(|&id| id == first).unwrap();
            let end = siblings.iter().position(|&id| id == last).unwrap();
            let mut children = document.take_children(parent);
            let wrapper = document.create_node(Wrapped);
            for child in children.drain(start..=end) {
                document.push_child(wrapper, child);
            }
            children.insert(start, wrapper);
            document.replace_children(parent, children);
        }
    }
}

#[cfg(test)]
mod tests {
    use markdown_it::document::Document;
    use markdown_it::document::edit::EditBatch;

    use super::*;

    fn parse(source: &str) -> Document {
        let mut md = MarkdownIt::new();
        markdown_it::plugins::cmark::add(&mut md);
        install(&mut md);
        md.parse(source)
    }

    fn assert_wrapped(document: &Document, paragraph: NodeId) -> NodeId {
        let children = document.children(paragraph);
        assert_eq!(children.len(), 1, "original siblings must be replaced");
        let wrapper = children[0];
        assert!(document.value::<Wrapped>(wrapper).is_some());
        assert_eq!(document.parent(wrapper), Some(paragraph));
        for &child in document.children(wrapper) {
            assert_eq!(document.parent(child), Some(wrapper));
        }
        wrapper
    }

    #[test]
    fn finish_callback_wraps_pending_unicode_text() {
        let doc = parse("前雪尾%");
        let paragraph = doc.children(doc.root())[0];
        let wrapper = assert_wrapped(&doc, paragraph);
        let children = doc.children(wrapper);
        assert_eq!(children.len(), 1);
        assert_eq!(doc.text(children[0]), Some("前雪尾"));
    }

    #[test]
    fn wrapping_preserves_order_and_existing_subtrees() {
        let doc = parse("前 *重点* `代码` 尾%");
        let paragraph = doc.children(doc.root())[0];
        let wrapper = assert_wrapped(&doc, paragraph);
        let children = doc.children(wrapper);
        assert_eq!(children.len(), 5);
        assert_eq!(doc.text(children[0]), Some("前 "));
        assert_eq!(doc.text(children[2]), Some(" "));
        assert_eq!(doc.text(children[4]), Some(" 尾"));
        let emphasis = children[1];
        let text = doc.children(emphasis)[0];
        assert_eq!(doc.text(text), Some("重点"));
        assert_eq!(doc.parent(text), Some(emphasis));
    }

    #[test]
    fn finish_callback_stays_in_its_inline_session() {
        let doc = parse("第一段%\n\n第二段");
        let paragraphs = doc.children(doc.root());
        assert_eq!(paragraphs.len(), 2);
        assert_wrapped(&doc, paragraphs[0]);
        let second = doc.children(paragraphs[1]);
        assert_eq!(second.len(), 1);
        assert_eq!(doc.text(second[0]), Some("第二段"));
    }

    #[test]
    fn empty_sessions_do_not_create_wrappers_and_repeated_requests_coalesce() {
        let empty = parse("%");
        let paragraph = empty.children(empty.root())[0];
        assert!(empty.children(paragraph).is_empty());
        let doc = parse("雪%%");
        let paragraph = doc.children(doc.root())[0];
        let wrapper = assert_wrapped(&doc, paragraph);
        let text = doc.children(wrapper)[0];
        assert_eq!(doc.text(text), Some("雪"));
        assert!(doc.node(text).ext().get::<PendingWrap>().is_none());
    }

    #[test]
    fn wrapping_is_scheduled_after_inline_and_before_dependent_core_rules() {
        #[derive(Default)]
        struct Observe;
        impl CoreRule for Observe {
            fn run(&self, state: &mut CoreState<'_>) {
                let doc = &*state.document;
                let paragraph = doc.children(doc.root())[0];
                assert_wrapped(doc, paragraph);
                let mut edits = EditBatch::new();
                edits.set_attribute(paragraph, "observed", "yes");
                edits.commit(state.document);
            }
        }
        let mut md = MarkdownIt::new();
        markdown_it::plugins::cmark::add(&mut md);
        // Register the consumer first: constraints, not registration order,
        // must put it after the producer.
        md.add_rule::<Observe>().after::<ApplyWraps>();
        install(&mut md);
        let doc = md.parse("雪%");
        let paragraph = doc.children(doc.root())[0];
        assert!(
            doc.node(paragraph)
                .attrs()
                .iter()
                .any(|(key, value)| key == "observed" && value == "yes")
        );
    }
}
