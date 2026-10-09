//! Task list syntax, like `- [ ] todo` and `- [x] done`.

use crate::MarkdownIt;
use crate::common::sourcemap::SourcePos;
use crate::document::edit::EditBatch;
use crate::document::transform::DocumentTransform;
use crate::document::{
    Document,
    HtmlAttributes,
    NodeDraft,
    NodeId,
    NodeValue,
    StructuralEvent,
    Text,
};
use crate::plugins::cmark::block::list::{BulletList, ListItem, OrderedList};
use crate::plugins::cmark::block::paragraph::Paragraph;

// --- pub method ---

pub fn add(md: &mut MarkdownIt) {
    md.add_document_renderer::<TaskListMarker, _>("html", TaskListMarkerRenderer);
    md.add_document_renderer::<TaskListMarker, _>("text", crate::render::EmptyDocumentRenderer);
    md.add_document_transform::<TaskListDocumentTransform>()
        .before::<crate::plugins::sourcepos::SourcePosDocumentTransform>();
}

#[derive(Debug)]
pub struct TaskListMarker {
    pub checked: bool,
}

impl NodeValue for TaskListMarker {}

struct TaskListScanner;
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

        for event in document.events(document.root()) {
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
                    // remove task list marker text; it is replaced by a checkbox
                    // at render time
                    edits.replace_text(plan.text, 0..plan.marker_len, "");
                    if let Some(source_map) = plan.source_map {
                        let (start, end) = source_map.get_byte_offsets();
                        // update source map
                        edits.set_source_map(
                            plan.text,
                            Some(SourcePos::new(start + plan.marker_len, end)),
                        );
                    }
                    edits.insert_before(plan.marker_target, marker_draft(plan.checked));
                }

                let item_node = document.node(plan.item);
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
    let item_node = document.node(item);
    if !item_node.is::<ListItem>() {
        return None;
    }

    let first = *item_node.children().first()?;
    let first_node = document.node(first);
    // Loose list items wrap their content in a paragraph, compact ones do not.
    let (text, marker_target) = if first_node.is::<Paragraph>() {
        (*first_node.children().first()?, first)
    } else {
        (first, first)
    };
    let text_node = document.node(text);
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

// render a checkbox `<input type="checkbox" ... />`
struct TaskListMarkerRenderer;
impl crate::DocumentNodeRenderer<TaskListMarker> for TaskListMarkerRenderer {
    fn render(
        &self,
        _: crate::NodeRef<'_>,
        marker: &TaskListMarker,
        context: &mut crate::DocumentRenderContext<'_>,
        output: &mut crate::DocumentWriter,
    ) {
        let mut attrs = vec![
            ("class".into(), "task-list-item-checkbox".into()),
            ("disabled".into(), String::new()),
            ("type".into(), "checkbox".into()),
        ];
        if marker.checked {
            attrs.push(("checked".into(), String::new()));
        }
        crate::render::write_html_self_close(output, "input", &attrs, context.options().xhtml_out);
        output.write_str(" ");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(input: &str, expected: &str) {
        let mut md = MarkdownIt::new();
        add(&mut md);
        assert_eq!(md.render(input), expected);
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
}
