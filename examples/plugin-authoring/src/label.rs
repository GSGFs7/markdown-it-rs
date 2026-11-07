//! Configured inline leaf plugin.
//!
//! Uses a [`PluginSpec`] to match `@ok`, create a leaf node with a typed
//! `Label` payload, and render the configured text as `<mark>`.
//!
//! # What it demonstrates
//!
//! - Configuring a plugin with a runtime-provided value.
//! - Creating a leaf node with a typed payload.
//! - Rendering HTML through the plugin's render helper.
//!
//! # Test coverage
//!
//! - `configured_leaf_creates_typed_payload_and_escapes_rendered_text`: the
//!   node keeps the configured typed payload, and rendering preserves Unicode
//!   while escaping HTML.
//! - `failed_leaf_match_preserves_input_and_later_matches`: a failed match
//!   leaves the input untouched, so later `@ok` matches still work.

use markdown_it::plugin::PluginSpec;

#[derive(Debug)]
struct Label(String);

pub fn label_plugin(label: &str) -> PluginSpec {
    let label = label.to_owned();
    PluginSpec::new("configured-label")
        .inline_leaf::<Label>("label")
        .marker('@')
        .parse(move |cx| cx.consume("@ok").then(|| Label(label.clone())))
        .render_html(|label, html| {
            html.element("mark").text(&label.0);
        })
        .finish()
}

#[cfg(test)]
mod tests {
    use markdown_it::MarkdownIt;

    use super::*;

    #[test]
    fn configured_leaf_creates_typed_payload_and_escapes_rendered_text() {
        let md = MarkdownIt::builder().plugin(label_plugin("雪<&")).build();
        assert_eq!(md.render("@ok"), "<p><mark>雪&lt;&amp;</mark></p>\n");
        let doc = md.parse("@ok");
        let paragraph = doc.children(doc.root())[0];
        let node = doc.children(paragraph)[0];
        assert_eq!(doc.value::<Label>(node).unwrap().0, "雪<&");
    }

    #[test]
    fn failed_leaf_match_preserves_input_and_later_matches() {
        let md = MarkdownIt::builder().plugin(label_plugin("yes")).build();
        assert_eq!(
            md.render("前@no @ok尾"),
            "<p>前@no <mark>yes</mark>尾</p>\n"
        );
    }
}
