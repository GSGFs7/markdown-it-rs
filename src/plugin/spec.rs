use std::collections::{HashMap, HashSet};
use std::fmt;

use super::transform::TransformInstaller;
use super::{
    BlockContainerBuilder,
    BlockLeafBuilder,
    InlineContainerBuilder,
    InlineLeafBuilder,
    InlinePairBuilder,
    Plugin,
};
use crate::MarkdownIt;
use crate::document::{Document, NodeValue};

// Preserve every logical rule's owner and local name, including rules lowered
// into shared block dispatchers and pair scanners. This is metadata, not a map
// of independently addressable low-level execution units.
#[derive(Debug, Default)]
struct InstalledPlugins(HashMap<String, HashSet<String>>);

/// A plugin assembled from syntax rules, transforms and optional dependencies.
///
/// Rule names are local to the plugin; see the [module documentation](crate::plugin)
/// for identity and ordering semantics.
///
/// ```
/// use markdown_it::MarkdownIt;
/// use markdown_it::plugin::PluginSpec;
///
/// #[derive(Debug)]
/// struct Tag(String);
///
/// let plugin = PluginSpec::new("tag")
///     .inline_leaf::<Tag>("tag")
///     .marker('@')
///     .parse(|cx| {
///         if !cx.consume("@") {
///             return None;
///         }
///         Some(Tag("x".into()))
///     })
///     .render_html(|tag, html| {
///         html.element("code").text(&tag.0);
///     })
///     .finish();
///
/// let md = MarkdownIt::builder().plugin(plugin).build();
/// assert_eq!(md.render("@"), "<p><code>x</code></p>\n");
/// ```
pub struct PluginSpec {
    name: String,
    installers: Vec<Box<dyn PluginInstaller>>,
    before: Vec<String>,
    after: Vec<String>,
    require: Vec<String>,
    groups: Vec<String>,
    children: Vec<PluginSpec>,
}

impl fmt::Debug for PluginSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PluginSpec")
            .field("name", &self.name)
            .field("installers", &self.installers.len())
            .field("groups", &self.groups)
            .field("children", &self.children.len())
            .finish()
    }
}

impl PluginSpec {
    /// Create an empty plugin specification.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            installers: Vec::new(),
            before: Vec::new(),
            after: Vec::new(),
            require: Vec::new(),
            groups: Vec::new(),
            children: Vec::new(),
        }
    }

    /// Start an inline leaf belonging to this plugin.
    ///
    /// Call `.parse(...)` and register a renderer with `.render_html(...)`,
    /// `.render_text(...)` or `.render_with(format, renderer)`, then `.finish()`
    /// to return to this specification. Multiple leaves can be added this way;
    /// their rule names must be unique within the plugin.
    ///
    /// ```
    /// use markdown_it::MarkdownIt;
    /// use markdown_it::plugin::PluginSpec;
    ///
    /// #[derive(Debug)]
    /// struct Emoji;
    ///
    /// let emoji = PluginSpec::new("emoji")
    ///     .inline_leaf::<Emoji>("shortcode")
    ///     .marker(':')
    ///     .parse(|cx| cx.consume(":rocket:").then_some(Emoji))
    ///     .render_html(|_, html| {
    ///         html.element("span").class("emoji").text("🚀");
    ///     })
    ///     .finish();
    /// let md = MarkdownIt::builder().plugin(emoji).build();
    /// assert_eq!(md.render(":rocket:"), "<p><span class=\"emoji\">🚀</span></p>\n");
    /// ```
    ///
    /// Missing callbacks are rejected at compile time:
    ///
    /// ```compile_fail
    /// use markdown_it::plugin::PluginSpec;
    ///
    /// #[derive(Debug)]
    /// struct Emoji;
    ///
    /// PluginSpec::new("emoji")
    ///     .inline_leaf::<Emoji>("shortcode")
    ///     .parse(|cx| cx.consume(":rocket:").then_some(Emoji))
    ///     .finish(); // no renderer
    /// ```
    ///
    /// ```compile_fail
    /// use markdown_it::plugin::PluginSpec;
    ///
    /// #[derive(Debug)]
    /// struct Emoji;
    ///
    /// PluginSpec::new("emoji")
    ///     .inline_leaf::<Emoji>("shortcode")
    ///     .render_html(|_, html| { html.text("🚀"); })
    ///     .finish(); // no parser
    /// ```
    pub fn inline_leaf<T: NodeValue>(self, rule: impl Into<String>) -> InlineLeafBuilder<T> {
        InlineLeafBuilder::new(self, rule.into())
    }

    /// Start an inline container belonging to this plugin.
    ///
    /// A container parses a body as independent inline children and attaches
    /// them to a payload node. Call `.parse(...)` and register a renderer with
    /// `.html_tag(...)`, `.render_html(...)`, `.render_text(...)` or
    /// `.render_with(...)`, then `.finish()`.
    ///
    /// ```
    /// use markdown_it::MarkdownIt;
    /// use markdown_it::plugin::PluginSpec;
    ///
    /// #[derive(Debug)]
    /// struct Span;
    ///
    /// let span = PluginSpec::new("span")
    ///     .inline_container::<Span>("span")
    ///     .marker('{')
    ///     .parse(|cx| {
    ///         let source = cx.source();
    ///         if !source.starts_with('{') {
    ///             return None;
    ///         }
    ///
    ///         let end = source.find('}')?;
    ///         cx.advance(end + 1);
    ///         Some((Span, 1..end))
    ///     })
    ///     .render_html(|_, html| {
    ///         let children = html.render_children();
    ///         html.element("span")
    ///             .attr("data-kind", "note")
    ///             .raw(children);
    ///     })
    ///     .finish();
    ///
    /// let md = MarkdownIt::builder().plugin(span).build();
    /// assert_eq!(
    ///     md.render("{*hi*}"),
    ///     "<p><span data-kind=\"note\"><em>hi</em></span></p>\n"
    /// );
    /// ```
    ///
    /// The `find('}')` above is only a simple example. It matches the first
    /// closing brace, does not support nested paired delimiters, and does not
    /// skip `}` inside code spans or escapes. Plugins with those requirements
    /// need a more careful source scanner or the low-level rule API.
    ///
    /// Missing callbacks are rejected:
    ///
    /// ```compile_fail
    /// use markdown_it::plugin::PluginSpec;
    ///
    /// #[derive(Debug)]
    /// struct Span;
    ///
    /// PluginSpec::new("span")
    ///     .inline_container::<Span>("span")
    ///     .parse(|cx| {
    ///         cx.advance(1);
    ///         Some((Span, 0..0))
    ///     })
    ///     .finish(); // no renderer
    /// ```
    ///
    /// ```compile_fail
    /// use markdown_it::plugin::PluginSpec;
    ///
    /// #[derive(Debug)]
    /// struct Span;
    ///
    /// PluginSpec::new("span")
    ///     .inline_container::<Span>("span")
    ///     .html_tag("span")
    ///     .finish(); // no parser
    /// ```
    pub fn inline_container<T: NodeValue>(
        self,
        rule: impl Into<String>,
    ) -> InlineContainerBuilder<T> {
        InlineContainerBuilder::new(self, rule.into())
    }

    /// Start a block leaf belonging to this plugin.
    ///
    /// A block leaf parses source lines, emits one payload node, and renders
    /// its own output. Call `.parse(...)` and register a renderer with
    /// `.render_html(...)`, `.render_text(...)` or `.render_with(format, renderer)`,
    /// then `.finish()`. Use [`BlockLeafBuilder::interrupts_paragraph`] if it may
    /// start without a preceding blank line.
    ///
    /// ```
    /// use markdown_it::MarkdownIt;
    /// use markdown_it::plugin::PluginSpec;
    ///
    /// #[derive(Debug)]
    /// struct Notice(String);
    ///
    /// let notice = PluginSpec::new("notice")
    ///     .block_leaf::<Notice>("notice")
    ///     .interrupts_paragraph(true)
    ///     .parse(|cx| {
    ///         let text = cx.line(0)?.strip_prefix("!!! ")?.to_owned();
    ///         cx.advance_lines(1);
    ///         Some(Notice(text))
    ///     })
    ///     .render_html(|notice, html| {
    ///         html.element("aside").class("notice").text(&notice.0);
    ///     })
    ///     .finish();
    ///
    /// let md = MarkdownIt::builder().plugin(notice).build();
    /// assert_eq!(
    ///     md.render("before\n!!! hello\nafter"),
    ///     "<p>before</p>\n<aside class=\"notice\">hello</aside>\n<p>after</p>\n"
    /// );
    /// ```
    ///
    /// Missing callbacks are rejected at compile time:
    ///
    /// ```compile_fail
    /// use markdown_it::plugin::PluginSpec;
    ///
    /// #[derive(Debug)]
    /// struct Notice(String);
    ///
    /// PluginSpec::new("notice")
    ///     .block_leaf::<Notice>("notice")
    ///     .parse(|cx| {
    ///         cx.advance_lines(1);
    ///         Some(Notice(String::new()))
    ///     })
    ///     .finish(); // no renderer
    /// ```
    ///
    /// ```compile_fail
    /// use markdown_it::plugin::PluginSpec;
    ///
    /// #[derive(Debug)]
    /// struct Notice(String);
    ///
    /// PluginSpec::new("notice")
    ///     .block_leaf::<Notice>("notice")
    ///     .render_html(|_, html| { html.text("notice"); })
    ///     .finish(); // no parser
    /// ```
    pub fn block_leaf<T: NodeValue>(self, rule: impl Into<String>) -> BlockLeafBuilder<T> {
        BlockLeafBuilder::new(self, rule.into())
    }

    /// Start a fenced block container belonging to this plugin.
    ///
    /// A container parses a fixed opening region, then parses its body as
    /// nested block markup and attaches the resulting nodes to a payload node.
    /// This high-level form handles fences whose body is a contiguous line
    /// range without rewriting each line's prefix. Containers that strip a
    /// per-line prefix, such as blockquotes, still require a low-level
    /// [`BlockRule`](crate::parser::block::BlockRule).
    ///
    /// Call `.parse(...)` and register a renderer with `.html_tag(...)`,
    /// `.render_html(...)`, `.render_text(...)` or `.render_with(...)`, then
    /// `.finish()`. Use [`BlockContainerBuilder::interrupts_paragraph`] if the
    /// opening marker may start without a preceding blank line.
    ///
    /// ```
    /// use markdown_it::MarkdownIt;
    /// use markdown_it::plugin::{BlockContainerParse, PluginSpec};
    ///
    /// #[derive(Debug)]
    /// struct Spoiler;
    ///
    /// let spoiler = PluginSpec::new("spoiler")
    ///     .block_container::<Spoiler>("spoiler")
    ///     .parse(|cx| {
    ///         if cx.line(0)? != "%%%" {
    ///             return None;
    ///         }
    ///
    ///         // find the close tag from remaining lines
    ///         let close = (1..cx.remaining_lines())
    ///             .find(|&line| cx.line(line) == Some("%%%"))?;
    ///
    ///         // return plugin type & cursor state
    ///         Some(BlockContainerParse::new(Spoiler, close - 1).close_lines(1))
    ///     })
    ///     .html_tag("details")
    ///     .finish();
    ///
    /// let md = MarkdownIt::builder().plugin(spoiler).build();
    /// assert_eq!(
    ///     md.render("%%%\nbody *here*\n%%%\nafter"),
    ///     "<details>\n<p>body <em>here</em></p>\n</details>\n<p>after</p>\n"
    /// );
    /// ```
    ///
    /// Missing callbacks are rejected at compile time:
    ///
    /// ```compile_fail
    /// use markdown_it::plugin::{BlockContainerParse, PluginSpec};
    ///
    /// #[derive(Debug)]
    /// struct Admonition;
    ///
    /// PluginSpec::new("admonition")
    ///     .block_container::<Admonition>("admonition")
    ///     .parse(|_| Some(BlockContainerParse::new(Admonition, 0)))
    ///     .finish(); // no renderer
    /// ```
    ///
    /// ```compile_fail
    /// use markdown_it::plugin::PluginSpec;
    ///
    /// #[derive(Debug)]
    /// struct Admonition;
    ///
    /// PluginSpec::new("admonition")
    ///     .block_container::<Admonition>("admonition")
    ///     .html_tag("div")
    ///     .finish(); // no parser
    /// ```
    pub fn block_container<T: NodeValue>(
        self,
        rule: impl Into<String>,
    ) -> BlockContainerBuilder<T> {
        BlockContainerBuilder::new(self, rule.into())
    }

    /// Start an emphasis-style inline pair belonging to this plugin.
    ///
    /// Pairs reuse the emphasis delimiter scanner and matching algorithm to
    /// create nodes with parsed inline children. Call
    /// [`delimiter`](InlinePairBuilder::delimiter),
    /// [`value`](InlinePairBuilder::value) and
    /// register a renderer with [`html_tag`](InlinePairBuilder::html_tag) or
    /// [`render_with`](InlinePairBuilder::render_with), then `.finish()`.
    ///
    /// ```
    /// use markdown_it::MarkdownIt;
    /// use markdown_it::plugin::PluginSpec;
    ///
    /// #[derive(Debug)]
    /// struct Mark;
    ///
    /// let highlight = PluginSpec::new("highlight")
    ///     .inline_pair::<Mark>("highlight")
    ///     .delimiter::<'=', 2, true>()
    ///     .value(|| Mark)
    ///     .html_tag("mark")
    ///     .finish();
    ///
    /// let md = MarkdownIt::builder().plugin(highlight).build();
    /// assert_eq!(md.render("a ==marked== b"), "<p>a <mark>marked</mark> b</p>\n");
    /// ```
    ///
    /// A delimiter must be selected before finishing the pair:
    ///
    /// ```compile_fail
    /// use markdown_it::plugin::PluginSpec;
    ///
    /// #[derive(Debug)]
    /// struct Mark;
    ///
    /// PluginSpec::new("highlight")
    ///     .inline_pair::<Mark>("highlight")
    ///     .value(|| Mark)
    ///     .html_tag("mark")
    ///     .finish();
    /// ```
    ///
    /// A renderer must also be registered:
    ///
    /// ```compile_fail
    /// use markdown_it::plugin::PluginSpec;
    ///
    /// #[derive(Debug)]
    /// struct Mark;
    ///
    /// PluginSpec::new("highlight")
    ///     .inline_pair::<Mark>("highlight")
    ///     .delimiter::<'=', 2, true>()
    ///     .value(|| Mark)
    ///     .finish();
    /// ```
    pub fn inline_pair<T: NodeValue>(self, rule: impl Into<String>) -> InlinePairBuilder<T> {
        InlinePairBuilder::new(self, rule.into())
    }

    /// Register a document rewrite that runs after inline parsing.
    ///
    /// The callback runs once per parse and may mutate values, attributes,
    /// source maps or structure. It is registered after [`InlineParserRule`],
    /// but still runs if that rule is removed (inline nodes may then be missing).
    ///
    /// Transforms of one plugin run in declaration order and honor plugin-level
    /// [`before`](Self::before)/[`after`](Self::after) constraints. The callback
    /// is `Fn`: use interior mutability for state.
    ///
    /// [`InlineParserRule`]: crate::parser::inline::InlineParserRule
    ///
    /// ```
    /// use markdown_it::MarkdownIt;
    /// use markdown_it::document::Text;
    /// use markdown_it::plugin::PluginSpec;
    ///
    /// let plugin = PluginSpec::new("prefix").transform("prefix", |document, _md| {
    ///     let paragraph = document.children(document.root())[0];
    ///     let first = document.children(paragraph)[0];
    ///     let prefix = document.create_node(Text {
    ///         content: "> ".into(),
    ///     });
    ///     document.insert_before(first, prefix);
    /// });
    ///
    /// let md = MarkdownIt::builder().plugin(plugin).build();
    /// assert_eq!(md.render("hello"), "<p>&gt; hello</p>\n");
    /// ```
    pub fn transform<F>(self, rule: impl Into<String>, transform: F) -> Self
    where
        F: Fn(&mut Document, &MarkdownIt) + Send + Sync + 'static,
    {
        let rule = rule.into();
        self.with_installer(Box::new(TransformInstaller::new(rule, transform)))
    }

    /// Create a plugin from a custom install function.
    ///
    /// The function runs once when the plugin is installed and can modify the
    /// parser configuration or register rules and renderers.
    ///
    /// ```
    /// use markdown_it::MarkdownIt;
    /// use markdown_it::plugin::PluginSpec;
    ///
    /// let limit = 7;
    /// let plugin = PluginSpec::custom("limit", move |md| {
    ///     md.max_nesting = limit;
    /// });
    /// assert_eq!(plugin.name(), "limit");
    ///
    /// let md = MarkdownIt::builder().plugin(plugin).build();
    ///
    /// assert_eq!(md.max_nesting, 7);
    /// ```
    pub fn custom(
        name: impl Into<String>,
        install: impl FnOnce(&mut MarkdownIt) + Send + 'static,
    ) -> Self {
        let name = name.into();
        Self::new(name.clone()).with_installer(Box::new(FnInstaller {
            name,
            install: Box::new(install),
        }))
    }

    /// Name used for duplicate-plugin diagnostics.
    ///
    /// ```rust
    /// use markdown_it::MarkdownIt;
    /// use markdown_it::plugin::PluginSpec;
    ///
    /// let plugin = PluginSpec::custom("limit", |md| {
    ///     md.max_nesting = 7;
    /// });
    ///
    /// assert_eq!(plugin.name(), "limit");
    /// ```
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Order this plugin before `plugin` when that plugin is present.
    ///
    /// This only adds an ordering constraint; use [`require`](Self::require) to
    /// make the target mandatory. It affects installation order and the match
    /// order of rules sharing a marker, but does not override low-level
    /// `before`/`after`.
    pub fn before(mut self, plugin: impl Into<String>) -> Self {
        self.before.push(plugin.into());
        self
    }

    /// Install after another high-level plugin when that plugin is present.
    ///
    /// The target may be another plugin in this build or an already installed
    /// plugin. This only adds an ordering constraint; use
    /// [`require`](Self::require) to make the target mandatory.
    pub fn after(mut self, plugin: impl Into<String>) -> Self {
        self.after.push(plugin.into());
        self
    }

    /// Require another high-level plugin or group to exist.
    ///
    /// Unlike [`before`](Self::before) and [`after`](Self::after), this fails
    /// installation when the target is neither already installed nor part of
    /// the current installation batch. It does not add an ordering constraint.
    pub fn require(mut self, plugin: impl Into<String>) -> Self {
        self.require.push(plugin.into());
        self
    }

    /// Add any plugin to this group.
    ///
    /// This accepts [`PluginSpec`] values and custom [`Plugin`] implementations.
    /// A nested [`PluginSpec`] keeps its own
    /// identity and ordering constraints, so dependencies between children are
    /// resolved inside the group. A child with the same name as this
    /// specification is merged into it.
    pub fn with(mut self, plugin: impl Plugin) -> Self {
        let child = plugin.into_spec();
        if child.name == self.name {
            self.installers.extend(child.installers);
            self.before.extend(child.before);
            self.after.extend(child.after);
            self.require.extend(child.require);
            self.children.extend(child.children);
        } else {
            self.children.push(child);
        }
        self
    }

    pub(super) fn with_installer(mut self, installer: Box<dyn PluginInstaller>) -> Self {
        self.installers.push(installer);
        self
    }

    pub(super) fn expand(self) -> Vec<PluginSpec> {
        self.expand_with(&[], &[], &[])
    }

    fn expand_with(
        self,
        inherited_before: &[String],
        inherited_after: &[String],
        inherited_groups: &[String],
    ) -> Vec<PluginSpec> {
        let PluginSpec {
            name,
            installers,
            before,
            after,
            require,
            groups: _,
            children,
        } = self;
        let before: Vec<_> = inherited_before.iter().chain(&before).cloned().collect();
        let after: Vec<_> = inherited_after.iter().chain(&after).cloned().collect();
        let groups = inherited_groups.to_vec();

        let mut expanded = Vec::with_capacity(1 + children.len());
        expanded.push(PluginSpec {
            name: name.clone(),
            installers,
            before: before.clone(),
            after: after.clone(),
            require,
            groups: groups.clone(),
            children: Vec::new(),
        });

        let mut child_groups = inherited_groups.to_vec();
        child_groups.push(name);
        for child in children {
            expanded.extend(child.expand_with(&before, &after, &child_groups));
        }
        expanded
    }

    fn validate(&self) {
        assert!(!self.name.is_empty(), "plugin name must not be empty");

        let mut rules = HashSet::new();
        for installer in &self.installers {
            installer.validate();
            assert!(
                rules.insert(installer.name()),
                "plugin {:?} rule {:?} was added more than once",
                self.name,
                installer.name()
            );
        }
    }

    /// Validate and install this plugin into an existing parser.
    ///
    /// Validation happens before anything is installed; rule chains are
    /// compiled before this method returns.
    ///
    /// # Panics
    ///
    /// Panics before registration on an invalid specification, missing or
    /// cyclic dependency, or duplicate plugin. A custom installer with an
    /// invalid low-level rule chain panics after installation, leaving the
    /// parser partially modified; discard it in that case.
    pub fn install(self, md: &mut MarkdownIt) {
        install_plugins(md, self.expand());
    }

    fn install_validated(self, md: &mut MarkdownIt) {
        let PluginSpec {
            name,
            installers,
            before,
            after,
            groups,
            ..
        } = self;

        let context = PluginInstallContext {
            name: &name,
            before: &before,
            after: &after,
            groups: &groups,
        };
        let rules = installers
            .iter()
            .map(|installer| installer.name().to_owned())
            .collect();
        for installer in installers {
            installer.install(md, &context);
        }
        super::transform::install_ordering_anchor(md, &context);
        md.ext
            .get_or_insert_default::<InstalledPlugins>()
            .0
            .insert(name, rules);
    }
}

impl Plugin for PluginSpec {
    fn into_spec(self) -> PluginSpec {
        self
    }
}

#[derive(Default)]
pub(super) struct RegistrationContext {
    pub pairs: HashMap<char, crate::parser::inline::helpers::emph_pair::PairRegistration>,
}

/// Plugin identity and ordering available to installers.
pub(super) struct PluginInstallContext<'a> {
    pub name: &'a str,
    pub before: &'a [String],
    pub after: &'a [String],
    pub groups: &'a [String],
}

pub(super) trait PluginInstaller: Send {
    fn name(&self) -> &str;
    fn validate(&self);
    fn validate_registration(&self, _md: &MarkdownIt, _context: &mut RegistrationContext) {}
    fn install(self: Box<Self>, md: &mut MarkdownIt, context: &PluginInstallContext<'_>);
}

type InstallFn = Box<dyn FnOnce(&mut MarkdownIt) + Send + 'static>;

struct FnInstaller {
    name: String,
    install: InstallFn,
}

impl PluginInstaller for FnInstaller {
    fn name(&self) -> &str {
        &self.name
    }

    fn validate(&self) {
        assert!(
            !self.name.is_empty(),
            "custom plugin name must not be empty"
        );
    }

    fn install(self: Box<Self>, md: &mut MarkdownIt, _context: &PluginInstallContext<'_>) {
        let Self { install, .. } = *self;
        install(md);
    }
}

// Validate the whole batch before installing anything. Previously installed
// plugins are fixed before this batch, so `before(existing)` is rejected.
// After installation, `install_plugins` compiles every rule chain so low-level
// dependency errors from custom installers do not wait until the first parse.
fn installation_order(md: &MarkdownIt, plugins: &[PluginSpec]) -> Vec<usize> {
    let installed: HashSet<&str> = md
        .ext
        .get::<InstalledPlugins>()
        .map(|plugins| plugins.0.keys().map(String::as_str).collect())
        .unwrap_or_default();
    let mut names = installed.clone();
    let mut context = RegistrationContext::default();
    for plugin in plugins {
        plugin.validate();
        for installer in &plugin.installers {
            installer.validate_registration(md, &mut context);
        }
        assert!(
            names.insert(plugin.name.as_str()),
            "plugin {:?} was added more than once",
            plugin.name
        );
    }

    let mut order = crate::common::ruler::Ruler::<String, usize>::new();
    for (index, plugin) in plugins.iter().enumerate() {
        for name in &plugin.require {
            assert!(
                names.contains(name.as_str()),
                "missing dependency: {:?} requires {:?}",
                plugin.name,
                name
            );
        }

        let item = order.add(plugin.name.clone(), index);
        for name in &plugin.before {
            assert!(
                !installed.contains(name.as_str()),
                "plugin {:?} cannot be ordered before installed plugin {name:?}",
                plugin.name
            );
            item.before(name.clone());
        }
        for name in &plugin.after {
            item.after(name.clone());
        }
    }
    order.iter().copied().collect()
}

pub(super) fn install_plugins(md: &mut MarkdownIt, plugins: Vec<PluginSpec>) {
    let order = installation_order(md, &plugins);
    let mut plugins: Vec<_> = plugins.into_iter().map(Some).collect();
    for index in order {
        plugins[index]
            .take()
            .expect("plugin installation order contained a duplicate")
            .install_validated(md);
    }
    md.compile_rules();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_metadata_preserves_local_names_for_all_rule_kinds() {
        #[derive(Debug)]
        struct Item;
        #[derive(Debug)]
        struct Static;

        let syntax = PluginSpec::new("syntax")
            .inline_leaf::<Item>("inline-leaf")
            .parse(|_| None)
            .render_text(|_, _| {})
            .finish()
            .inline_container::<Item>("inline-container")
            .parse(|_| None)
            .render_text(|_, _| {})
            .finish()
            .block_leaf::<Item>("block-leaf")
            .parse(|_| None)
            .render_text(|_, _| {})
            .finish()
            .block_container::<Item>("block-container")
            .parse(|_| None)
            .render_text(|_, _| {})
            .finish()
            .inline_pair::<Item>("pair")
            .delimiter::<'|', 2, true>()
            .value(|| Item)
            .render_text(|_, _| {})
            .finish()
            .transform("rewrite", |_, _| {});
        let group = PluginSpec::new("group")
            .with(syntax)
            .with(
                PluginSpec::new("static")
                    .inline_leaf::<Static>("static")
                    .parse(|_| None)
                    .render_html(|_, _| {})
                    .finish(),
            )
            .with(PluginSpec::new("group").transform("rewrite", |_, _| {}));
        let md = MarkdownIt::builder().plugin(group).build();
        let installed = &md.ext.get::<InstalledPlugins>().unwrap().0;
        assert_eq!(installed.len(), 3);
        assert_eq!(
            installed["syntax"],
            HashSet::from(
                [
                    "inline-leaf",
                    "inline-container",
                    "block-leaf",
                    "block-container",
                    "pair",
                    "rewrite",
                ]
                .map(str::to_owned)
            )
        );
        assert_eq!(installed["static"], HashSet::from(["static".to_owned()]));
        assert_eq!(installed["group"], HashSet::from(["rewrite".to_owned()]));
    }
}
