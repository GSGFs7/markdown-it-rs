//! Task list syntax, like `- [ ] todo` and `- [x] done`.

use crate::common::sourcemap::SourcePos;
use crate::parser::core::CoreRule;
use crate::parser::document::{Document, NodeDraft, NodeId, StructuralEvent};
use crate::parser::document_edit::EditBatch;
use crate::parser::document_transform::DocumentTransform;
use crate::parser::inline::Text;
use crate::parser::inline::builtin::InlineParserRule;
use crate::parser::main::MarkdownIt;
use crate::parser::node::{HtmlAttributes, Node, NodeValue};
use crate::plugins::cmark::block::list::{BulletList, ListItem, OrderedList};
use crate::plugins::cmark::block::paragraph::Paragraph;
use crate::plugins::sourcepos::SourcePosDocumentTransform;

// --- pub method ---

pub fn add(md: &mut MarkdownIt) {
    // after all the inline rule
    // make sure we are get the final AST
    md.add_rule::<TaskListScanner>().after::<InlineParserRule>();
}

pub fn add_document(md: &mut MarkdownIt) {
    md.add_document_transform::<TaskListDocumentTransform>()
        .before::<SourcePosDocumentTransform>();
}

#[derive(Debug)]
pub struct TaskListMarker {
    pub checked: bool,
}

impl NodeValue for TaskListMarker {
    fn render(&self, _node: &Node, fmt: &mut dyn crate::Renderer) {
        let mut attrs = vec![
            ("class".into(), "task-list-item-checkbox".to_owned()),
            // prevent checking by user
            ("disabled".into(), String::new()),
            ("type".into(), "checkbox".to_owned()),
        ];

        if self.checked {
            attrs.push(("checked".into(), String::new()));
        }

        // render a checkbox `<input type="checkbox" ... />`
        fmt.self_close("input", &attrs);
        fmt.text_raw(" ");
    }
}

#[doc(hidden)]
pub struct TaskListScanner;

impl TaskListScanner {
    /// find lenght
    fn marker_len(content: &str) -> Option<(bool, usize)> {
        // - [x] done
        // --^^^
        let is_checked = match content.as_bytes().get(..3)? {
            b"[ ]" => false,
            b"[x]" => true,
            b"[X]" => true,
            _ => return None,
        };

        // - [x] done
        // -----^  (has a white space?)
        match content.as_bytes().get(3) {
            Some(b' ' | b'\t') => Some((is_checked, 4)),
            None => Some((is_checked, 3)),
            // if not have a white space
            // it not a task list
            Some(_) => None,
        }
    }

    /// remove task list marker text
    /// it will be replaced with a checkbox in render stage
    fn strip_marker(nodes: &mut Vec<Node>) -> Option<bool> {
        let node = nodes.first_mut()?;
        let text = node.cast_mut::<Text>()?;
        let (is_checked, len) = Self::marker_len(&text.content)?;

        let text_is_empty = {
            // avoid ownership
            text.content.drain(..len);
            text.content.is_empty()
        };

        // update source map
        if let Some(map) = node.srcmap {
            let (start, end) = map.get_byte_offsets();
            node.srcmap = Some(crate::common::sourcemap::SourcePos::new(start + len, end));
        }

        if text_is_empty {
            nodes.remove(0);
        }

        Some(is_checked)
    }

    /// mark task list
    fn process_item(item: &mut Node) -> Option<()> {
        if !item.is::<ListItem>() {
            return None;
        }

        // process "loose list" & "compact list"
        let (checked, inline_nodes) = if item.children.first().is_some_and(|n| n.is::<Paragraph>())
        {
            // loose list
            //
            // ```markdown
            // - item1
            //
            // - item2
            // ```
            //
            // it will be render to:
            //
            // ```html
            // <ul>
            //   <li><p>item1</p></li>
            //   <li><p>item2</p></li>
            // </ul>
            // ```
            let paragraph = item.children.first_mut().unwrap();
            (
                // find marker in the paragraph children
                Self::strip_marker(&mut paragraph.children)?,
                &mut item.children,
            )
        } else {
            // compact list
            //
            // ```markdown
            // - item1
            // - item2
            // ```
            //
            // it will be rendered to:
            //
            // ```html
            // <ul>
            //   <li>item1</li>
            //   <li>item2</li>
            // </ul>
            // ```
            (Self::strip_marker(&mut item.children)?, &mut item.children)
        };

        inline_nodes.insert(0, Node::new(TaskListMarker { checked }));
        add_class(&mut item.attrs, "task-list-item");

        Some(())
    }

    /// find & process list item
    fn process_list(node: &mut Node) {
        if !node.is::<BulletList>() && !node.is::<OrderedList>() {
            return;
        }

        let mut contains_task = false;
        for child in node.children.iter_mut() {
            if Self::process_item(child).is_some() {
                contains_task = true;
            }
        }
        if contains_task {
            add_class(&mut node.attrs, "contains-task-list");
        }
    }
}

impl CoreRule for TaskListScanner {
    const NAMES: &'static [&'static str] = &["tasklist", "task_list"];

    fn run(root: &mut Node, _md: &MarkdownIt) {
        // traverse all nodes
        root.walk_mut(|node, _| {
            Self::process_list(node);
        });
    }
}

#[derive(Clone, Copy, Debug)]
struct TaskItemPlan {
    item: NodeId,
    text: NodeId,
    marker_target: NodeId,
    checked: bool,
    marker_len: usize,
    text_len: usize,
    source_map: Option<SourcePos>,
}

/// Arena-backed task-list transform.
#[derive(Debug, Default)]
pub struct TaskListDocumentTransform;

impl DocumentTransform for TaskListDocumentTransform {
    const KEY: &'static str = "extra::tasklist";
    const ALIASES: &'static [&'static str] = &["tasklist", "task_list"];

    fn run(&self, document: &Document) -> EditBatch {
        let mut edits = EditBatch::new();

        for event in document.events(document.root()).unwrap() {
            let list = match event {
                StructuralEvent::Enter(node)
                    if node.is::<BulletList>() || node.is::<OrderedList>() =>
                {
                    node
                }
                _ => continue,
            };

            let mut contains_task = false;
            for &item in list.children() {
                let Some(plan) = task_item_plan(document, item) else {
                    continue;
                };
                contains_task = true;

                if plan.marker_len == plan.text_len {
                    if plan.marker_target == plan.text {
                        edits.replace_node(plan.text, marker_draft(plan.checked));
                    } else {
                        edits.remove_node(plan.text);
                        edits.insert_before(plan.marker_target, marker_draft(plan.checked));
                    }
                } else {
                    edits.replace_text(plan.text, 0..plan.marker_len, "");
                    if let Some(source_map) = plan.source_map {
                        let (start, end) = source_map.get_byte_offsets();
                        edits.set_source_map(
                            plan.text,
                            Some(SourcePos::new(start + plan.marker_len, end)),
                        );
                    }
                    edits.insert_before(plan.marker_target, marker_draft(plan.checked));
                }

                let item_node = document.node(plan.item).unwrap();
                edits.set_attribute(
                    plan.item,
                    "class",
                    merged_class(item_node.attrs(), "task-list-item"),
                );
            }

            if contains_task {
                edits.set_attribute(
                    list.id(),
                    "class",
                    merged_class(list.attrs(), "contains-task-list"),
                );
            }
        }

        edits
    }
}

// --- helper ---

// check if it is a task list item
fn task_item_plan(document: &Document, item: NodeId) -> Option<TaskItemPlan> {
    let item_node = document.node(item).ok()?;
    if !item_node.is::<ListItem>() {
        return None;
    }

    let first = *item_node.children().first()?;
    let first_node = document.node(first).ok()?;
    let (text, marker_target) = if first_node.is::<Paragraph>() {
        (*first_node.children().first()?, first)
    } else {
        (first, first)
    };
    let text_node = document.node(text).ok()?;
    let text_value = text_node.cast::<Text>()?;
    let (checked, marker_len) = TaskListScanner::marker_len(&text_value.content)?;

    Some(TaskItemPlan {
        item,
        text,
        marker_target,
        checked,
        marker_len,
        text_len: text_value.content.len(),
        source_map: text_node.srcmap(),
    })
}

fn marker_draft(checked: bool) -> NodeDraft {
    NodeDraft::new(TaskListMarker { checked })
}

fn merged_class(attrs: &HtmlAttributes, class: &str) -> String {
    let mut value = attrs
        .iter()
        .filter(|attribute| attribute.0 == "class")
        .map(|attribute| attribute.1.as_str())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if !value.split_ascii_whitespace().any(|token| token == class) {
        if !value.is_empty() {
            value.push(' ');
        }
        value.push_str(class);
    }
    value
}

fn add_class(attrs: &mut HtmlAttributes, class: &str) {
    let value = merged_class(attrs, class);
    if let Some(index) = attrs.iter().position(|attribute| attribute.0 == "class") {
        attrs[index].1 = value;
        let mut kept = false;
        attrs.retain(|attribute| {
            if attribute.0 == "class" {
                let keep = !kept;
                kept = true;
                keep
            } else {
                true
            }
        });
    } else {
        attrs.push(("class".into(), value));
    }
}

// --- unit test ---

#[cfg(test)]
mod tests {
    use super::{TaskListDocumentTransform, TaskListScanner};
    use crate::parser::core::CoreRule;
    use crate::plugins::cmark::block::list::{BulletList, ListItem};
    use crate::{Document, MarkdownIt, Node};

    fn parser() -> MarkdownIt {
        let mut md = MarkdownIt::empty();
        crate::plugins::cmark::add(&mut md);
        md
    }

    fn run(input: &str, output: &str) {
        let mut legacy = parser();
        crate::plugins::extra::tasklist::add(&mut legacy);
        let legacy_html = legacy.parse(input).render();

        let parser = parser();
        let mut transforms = MarkdownIt::empty();
        crate::plugins::extra::tasklist::add_document(&mut transforms);
        let mut document = parser.parse_document(input);
        transforms.run_document_transforms(&mut document).unwrap();
        let document_html = document.into_legacy().render();

        assert_eq!(legacy_html, output);
        assert_eq!(document_html, legacy_html);
    }

    #[test]
    fn unchecked_item() {
        run(
            "- [ ] todo",
            "<ul class=\"contains-task-list\">\n<li class=\"task-list-item\"><input class=\"task-list-item-checkbox\" disabled=\"\" type=\"checkbox\"> todo</li>\n</ul>\n",
        );
    }

    #[test]
    fn checked_item() {
        run(
            "- [x] done",
            "<ul class=\"contains-task-list\">\n<li class=\"task-list-item\"><input class=\"task-list-item-checkbox\" disabled=\"\" type=\"checkbox\" checked=\"\"> done</li>\n</ul>\n",
        );
    }

    #[test]
    fn checked_item_uppercase_marker() {
        run(
            "- [X] done",
            "<ul class=\"contains-task-list\">\n<li class=\"task-list-item\"><input class=\"task-list-item-checkbox\" disabled=\"\" type=\"checkbox\" checked=\"\"> done</li>\n</ul>\n",
        );
    }

    #[test]
    fn invalid_marker_text_is_not_a_task_item() {
        run("- ni hao", "<ul>\n<li>ni hao</li>\n</ul>\n");
        run("- [y] todo", "<ul>\n<li>[y] todo</li>\n</ul>\n");
        run("- abc", "<ul>\n<li>abc</li>\n</ul>\n");
    }

    #[test]
    fn marker_requires_space_or_line_end() {
        run("- [x]done", "<ul>\n<li>[x]done</li>\n</ul>\n");
    }

    #[test]
    fn marker_can_be_followed_by_tab() {
        run(
            "- [ ]\ttodo",
            "<ul class=\"contains-task-list\">\n<li class=\"task-list-item\"><input class=\"task-list-item-checkbox\" disabled=\"\" type=\"checkbox\"> todo</li>\n</ul>\n",
        );
    }

    #[test]
    fn empty_task_items() {
        run(
            "- [ ]",
            "<ul class=\"contains-task-list\">\n<li class=\"task-list-item\"><input class=\"task-list-item-checkbox\" disabled=\"\" type=\"checkbox\"> </li>\n</ul>\n",
        );
        run(
            "- [x]",
            "<ul class=\"contains-task-list\">\n<li class=\"task-list-item\"><input class=\"task-list-item-checkbox\" disabled=\"\" type=\"checkbox\" checked=\"\"> </li>\n</ul>\n",
        );
    }

    #[test]
    fn ordered_list_items() {
        run(
            "1. [ ] todo",
            "<ol class=\"contains-task-list\">\n<li class=\"task-list-item\"><input class=\"task-list-item-checkbox\" disabled=\"\" type=\"checkbox\"> todo</li>\n</ol>\n",
        );
        run(
            "3. [x] done",
            "<ol class=\"contains-task-list\" start=\"3\">\n<li class=\"task-list-item\"><input class=\"task-list-item-checkbox\" disabled=\"\" type=\"checkbox\" checked=\"\"> done</li>\n</ol>\n",
        );
    }

    #[test]
    fn mixed_task_and_plain_items() {
        run(
            "- [x] done\n- plain",
            "<ul class=\"contains-task-list\">\n<li class=\"task-list-item\"><input class=\"task-list-item-checkbox\" disabled=\"\" type=\"checkbox\" checked=\"\"> done</li>\n<li>plain</li>\n</ul>\n",
        );
    }

    #[test]
    fn nested_task_list_marks_only_nested_list() {
        run(
            "- parent\n  - [x] child",
            "<ul>\n<li>parent\n<ul class=\"contains-task-list\">\n<li class=\"task-list-item\"><input class=\"task-list-item-checkbox\" disabled=\"\" type=\"checkbox\" checked=\"\"> child</li>\n</ul>\n</li>\n</ul>\n",
        );
    }

    #[test]
    fn inline_nodes_after_marker_are_preserved() {
        run(
            "- [x] **done**",
            "<ul class=\"contains-task-list\">\n<li class=\"task-list-item\"><input class=\"task-list-item-checkbox\" disabled=\"\" type=\"checkbox\" checked=\"\"> <strong>done</strong></li>\n</ul>\n",
        );
    }

    #[test]
    fn stripped_marker_updates_text_source_map() {
        let mut md = parser();
        crate::plugins::extra::tasklist::add(&mut md);

        let ast = md.parse("- [ ] todo");
        let text = &ast.children[0].children[0].children[1];

        assert_eq!(
            text.cast::<crate::parser::inline::Text>().unwrap().content,
            "todo",
        );
        assert_eq!(text.srcmap.unwrap().get_byte_offsets(), (6, 10));

        let parser = parser();
        let mut transforms = MarkdownIt::empty();
        crate::plugins::extra::tasklist::add_document(&mut transforms);
        let mut document = parser.parse_document("- [ ] todo");
        transforms.run_document_transforms(&mut document).unwrap();
        let ast = document.into_legacy();
        let text = &ast.children[0].children[0].children[1];
        assert_eq!(
            text.cast::<crate::parser::inline::Text>().unwrap().content,
            "todo",
        );
        assert_eq!(text.srcmap.unwrap().get_byte_offsets(), (6, 10));
    }

    #[test]
    fn loose_item() {
        run(
            "- [ ] todo\n\n  details",
            "<ul class=\"contains-task-list\">\n<li class=\"task-list-item\"><input class=\"task-list-item-checkbox\" disabled=\"\" type=\"checkbox\"> \n<p>todo</p>\n<p>details</p>\n</li>\n</ul>\n",
        );
    }

    #[test]
    fn not_at_start() {
        run("- a [x] task", "<ul>\n<li>a [x] task</li>\n</ul>\n");
    }

    #[test]
    fn existing_classes_are_preserved_and_normalized() {
        let parser = parser();
        let source = "- [x] done";
        let mut legacy = parser.parse(source);
        add_existing_classes(&mut legacy);
        TaskListScanner::run(&mut legacy, &parser);

        let mut document_root = parser.parse(source);
        add_existing_classes(&mut document_root);
        let mut document = Document::from_legacy(source, document_root);
        let mut transforms = MarkdownIt::empty();
        super::add_document(&mut transforms);
        transforms.run_document_transforms(&mut document).unwrap();

        let legacy_html = legacy.render();
        assert_eq!(document.into_legacy().render(), legacy_html);
        assert!(legacy_html.contains(r#"<ul class="outer contains-task-list">"#));
        assert!(legacy_html.contains(r#"<li class="item task-list-item">"#));
        assert_eq!(legacy_html.matches("class=\"outer").count(), 1);
    }

    #[test]
    fn document_runner_is_explicit_and_legacy_registration_is_separate() {
        let base_parser = parser();
        let mut transforms = MarkdownIt::empty();
        super::add_document(&mut transforms);
        let document = base_parser.parse_document("- [ ] todo");
        assert_eq!(
            document.into_legacy().render(),
            "<ul>\n<li>[ ] todo</li>\n</ul>\n"
        );

        let mut legacy = parser();
        super::add(&mut legacy);
        assert!(
            !legacy
                .document_transforms
                .contains::<TaskListDocumentTransform>()
        );
    }

    #[test]
    fn tasklist_runs_before_sourcepos_when_registered_in_reverse() {
        let source = "- [x] done";
        let mut legacy = parser();
        super::add(&mut legacy);
        crate::plugins::sourcepos::add(&mut legacy);
        let legacy_html = legacy.parse(source).render();

        let parser = parser();
        let mut transforms = MarkdownIt::empty();
        crate::plugins::sourcepos::add_document(&mut transforms);
        super::add_document(&mut transforms);
        let mut document = parser.parse_document(source);
        transforms.run_document_transforms(&mut document).unwrap();

        assert_eq!(document.into_legacy().render(), legacy_html);
    }

    fn add_existing_classes(root: &mut Node) {
        root.walk_mut(|node, _| {
            if node.is::<BulletList>() {
                node.attrs.push(("class".into(), "outer".into()));
                node.attrs.push(("class".into(), String::new()));
            } else if node.is::<ListItem>() {
                node.attrs.push(("class".into(), "item".into()));
            }
        });
    }
}
