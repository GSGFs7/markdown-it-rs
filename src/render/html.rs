//! HTML output helpers shared by the built-in HTML renderers.

use std::collections::HashMap;

use super::{RenderContext, Writer};
use crate::common::utils::escape_html;
use crate::document::{HtmlAttribute, NodeRef};

pub(crate) fn render_html_link(
    node: NodeRef<'_>,
    url: &str,
    title: Option<&str>,
    context: &mut RenderContext<'_>,
    output: &mut Writer,
) {
    let title = title.filter(|title| !title.is_empty());
    let extra = [("href", url), ("title", title.unwrap_or_default())];
    output.write_str("<a");
    write_html_attrs_with(
        output,
        node.attrs(),
        &extra[..1 + usize::from(title.is_some())],
    );
    output.write_char('>');
    context.render_children(node.id(), output);
    write_html_close(output, "a");
}

pub(crate) fn write_html_attrs(output: &mut Writer, attrs: &[HtmlAttribute]) {
    if attrs.is_empty() {
        return;
    }
    write_html_attrs_with(output, attrs, &[]);
}

// avoid attr clone
pub(crate) fn write_html_attrs_with(
    output: &mut Writer,
    attrs: &[HtmlAttribute],
    extra: &[(&str, &str)],
) {
    const LINEAR_SCAN_LIMIT: usize = 8;

    let count = attrs.len() + extra.len();
    let attrs = attrs
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .chain(extra.iter().copied());

    // small list, using linear scan. avoid hashmap overhead.
    // O(n^2), but very fast with small data.
    if count <= LINEAR_SCAN_LIMIT {
        for (index, (name, _)) in attrs.clone().enumerate() {
            // deduplication
            if attrs
                .clone()
                .take(index)
                .any(|(previous, _)| previous == name)
            {
                continue;
            }
            write_html_attr_group(
                output,
                name,
                attrs
                    .clone()
                    .filter_map(|(candidate, value)| (candidate == name).then_some(value)),
            );
        }
        return;
    }

    let mut values = HashMap::<&str, Vec<&str>>::new();
    let mut order = Vec::with_capacity(count);
    for (name, value) in attrs {
        values.entry(name).or_default().push(value);
        order.push(name);
    }
    for name in order {
        let Some(parts) = values.remove(name) else {
            continue;
        };
        write_html_attr_group(output, name, parts.into_iter());
    }
}

fn write_html_attr_group<'a>(
    output: &mut Writer,
    name: &str,
    values: impl Iterator<Item = &'a str>,
) {
    if name == "class" || name == "style" {
        output.write_char(' ');
        output.write_str(&escape_html(name));
        output.write_str("=\"");
        let separator = if name == "class" { ' ' } else { ';' };
        for (index, value) in values.enumerate() {
            if index != 0 {
                output.write_char(separator);
            }
            output.write_str(&escape_html(value));
        }
        output.write_char('"');
    } else {
        for value in values {
            output.write_char(' ');
            output.write_str(&escape_html(name));
            output.write_str("=\"");
            output.write_str(&escape_html(value));
            output.write_char('"');
        }
    }
}

pub(crate) fn write_html_open(output: &mut Writer, tag: &str, attrs: &[HtmlAttribute]) {
    output.write_char('<');
    output.write_str(tag);
    write_html_attrs(output, attrs);
    output.write_char('>');
}

pub(crate) fn write_html_close(output: &mut Writer, tag: &str) {
    output.write_str("</");
    output.write_str(tag);
    output.write_char('>');
}

pub(crate) fn write_html_self_close(
    output: &mut Writer,
    tag: &str,
    attrs: &[HtmlAttribute],
    xhtml: bool,
) {
    output.write_char('<');
    output.write_str(tag);
    write_html_attrs(output, attrs);
    if xhtml {
        output.write_str(" /");
    }
    output.write_char('>');
}

pub(crate) fn write_html_text(output: &mut Writer, value: &str) {
    output.write_escaped_html(value);
}
