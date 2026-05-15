//! Task list syntax, like `- [ ] todo` and `- [x] done`.

use crate::parser::core::CoreRule;
use crate::parser::inline::builtin::InlineParserRule;
use crate::parser::inline::Text;
use crate::plugins::cmark::block::list::{BulletList, ListItem, OrderedList};
use crate::plugins::cmark::block::paragraph::Paragraph;
use crate::{MarkdownIt, Node, NodeValue};

#[derive(Debug)]
pub struct TaskListMarker {
    pub checked: bool,
}

impl NodeValue for TaskListMarker {
    fn render(&self, _node: &Node, fmt: &mut dyn crate::Renderer) {
        let mut attrs = vec![
            ("class", "task-list-item-checkbox".to_owned()),
            // prevent checking by user
            ("disabled", String::new()),
            ("type", "checkbox".to_owned()),
        ];

        if self.checked {
            attrs.push(("checked", String::new()));
        }

        // render a checkbox `<input type="checkbox" ... />`
        fmt.self_close("input", &attrs);
        fmt.text_raw(" ");
    }
}

// --- scanner ---

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
        item.attrs.push(("class", "task-list-item".to_owned()));

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
            node.attrs.push(("class", "contains-task-list".to_owned()));
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

// --- pub method ---

pub fn add(md: &mut MarkdownIt) {
    // after all the inline rule
    // make sure we are get the final AST
    md.add_rule::<TaskListScanner>().after::<InlineParserRule>();
}

// --- unit test ---

#[cfg(test)]
mod tests {
    fn run(input: &str, output: &str) {
        let md = &mut crate::MarkdownIt::new();
        crate::plugins::cmark::add(md);
        crate::plugins::extra::tasklist::add(md);

        let html = md.parse(input).render();
        assert_eq!(html, output);
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
        let md = &mut crate::MarkdownIt::new();
        crate::plugins::cmark::add(md);
        crate::plugins::extra::tasklist::add(md);

        let ast = md.parse("- [ ] todo");
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
}
