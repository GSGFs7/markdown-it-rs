use std::collections::{HashMap, HashSet};
use std::fmt;

use super::{
    BlockLeafBuilder,
    InlineLeafBuilder,
    InlinePairBuilder,
    InlinePlugin,
    LeafRule,
    Plugin,
};
use crate::MarkdownIt;
use crate::document::NodeValue;

#[derive(Debug, Default)]
struct InstalledPlugins(HashSet<String>);

/// A plugin assembled from one or more leaves and optional ordering rules.
///
/// ```
/// use markdown_it::{MarkdownIt, PluginSpec};
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
    children: Vec<PluginSpec>,
}

impl fmt::Debug for PluginSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PluginSpec")
            .field("name", &self.name)
            .field("installers", &self.installers.len())
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
            children: Vec::new(),
        }
    }

    /// Create a plugin from a static [`InlinePlugin`] type.
    ///
    /// The plugin name is taken from [`InlinePlugin::NAME`].
    ///
    /// ```
    /// use markdown_it::{Html, InlinePlugin, InlinePluginState, MarkdownIt, PluginSpec};
    ///
    /// #[derive(Debug)]
    /// struct Mark;
    ///
    /// impl InlinePlugin for Mark {
    ///     const NAME: &'static str = "mark";
    ///     const MARKER: char = '=';
    ///
    ///     fn parse(cx: &mut InlinePluginState<'_>) -> Option<Self> {
    ///         cx.consume("==").then_some(Mark)
    ///     }
    ///
    ///     fn render_html(&self, html: &mut Html) {
    ///         html.element("mark").text("marked");
    ///     }
    /// }
    ///
    /// let md = MarkdownIt::builder().plugin(PluginSpec::of::<Mark>()).build();
    /// assert_eq!(md.render("a == b"), "<p>a <mark>marked</mark> b</p>\n");
    /// ```
    pub fn of<P: InlinePlugin>() -> Self {
        Self::new(P::NAME).with_installer(Box::new(LeafRule::<P>::new()))
    }

    /// Start an inline leaf belonging to this plugin.
    ///
    /// Call `.parse(...)` and register a renderer with `.render_html(...)`,
    /// `.render_text(...)` or `.render_with(format, renderer)`, then `.finish()`
    /// to return to this specification. Multiple leaves can be added this way;
    /// their rule names must be unique within the plugin.
    ///
    /// ```
    /// use markdown_it::{MarkdownIt, PluginSpec};
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
    /// use markdown_it::{PluginSpec};
    /// #[derive(Debug)]
    /// struct Emoji;
    /// PluginSpec::new("emoji")
    ///     .inline_leaf::<Emoji>("shortcode")
    ///     .parse(|cx| cx.consume(":rocket:").then_some(Emoji))
    ///     .finish(); // no renderer
    /// ```
    ///
    /// ```compile_fail
    /// use markdown_it::{PluginSpec};
    /// #[derive(Debug)]
    /// struct Emoji;
    /// PluginSpec::new("emoji")
    ///     .inline_leaf::<Emoji>("shortcode")
    ///     .render_html(|_, html| { html.text("🚀"); })
    ///     .finish(); // no parser
    /// ```
    pub fn inline_leaf<T: NodeValue>(self, rule: impl Into<String>) -> InlineLeafBuilder<T> {
        InlineLeafBuilder::new(self, rule.into())
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
    /// use markdown_it::{MarkdownIt, PluginSpec};
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
    /// use markdown_it::PluginSpec;
    /// #[derive(Debug)]
    /// struct Notice(String);
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
    /// use markdown_it::PluginSpec;
    /// #[derive(Debug)]
    /// struct Notice(String);
    /// PluginSpec::new("notice")
    ///     .block_leaf::<Notice>("notice")
    ///     .render_html(|_, html| { html.text("notice"); })
    ///     .finish(); // no parser
    /// ```
    pub fn block_leaf<T: NodeValue>(self, rule: impl Into<String>) -> BlockLeafBuilder<T> {
        BlockLeafBuilder::new(self, rule.into())
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
    /// use markdown_it::{MarkdownIt, PluginSpec};
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
    /// use markdown_it::PluginSpec;
    /// #[derive(Debug)]
    /// struct Mark;
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
    /// use markdown_it::PluginSpec;
    /// #[derive(Debug)]
    /// struct Mark;
    /// PluginSpec::new("highlight")
    ///     .inline_pair::<Mark>("highlight")
    ///     .delimiter::<'=', 2, true>()
    ///     .value(|| Mark)
    ///     .finish();
    /// ```
    pub fn inline_pair<T: NodeValue>(self, rule: impl Into<String>) -> InlinePairBuilder<T> {
        InlinePairBuilder::new(self, rule.into())
    }

    /// Create a plugin from a custom install function.
    ///
    /// The function runs once when the plugin is installed and can modify the
    /// parser configuration or register rules and renderers.
    ///
    /// ```
    /// use markdown_it::{MarkdownIt, PluginSpec};
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
    /// use markdown_it::{MarkdownIt, PluginSpec};
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

    /// Install before another high-level plugin. The target must exist in this
    /// build; this orders all rules of the two plugins.
    pub fn before(mut self, plugin: impl Into<String>) -> Self {
        self.before.push(plugin.into());
        self
    }

    /// Install after another high-level plugin. The target may be another plugin
    /// in this build or an already installed plugin. Use this to declare a
    /// required plugin dependency.
    pub fn after(mut self, plugin: impl Into<String>) -> Self {
        self.after.push(plugin.into());
        self
    }

    /// Add any plugin to this group.
    ///
    /// This accepts static [`InlinePlugin`] values, [`PluginSpec`] values and
    /// custom plugin implementations. A nested [`PluginSpec`] keeps its own
    /// identity and ordering constraints, so dependencies between children are
    /// resolved inside the group. A child with the same name as this
    /// specification is merged into it.
    pub fn with(mut self, plugin: impl Plugin) -> Self {
        let child = plugin.into_spec();
        if child.name == self.name {
            self.installers.extend(child.installers);
            self.before.extend(child.before);
            self.after.extend(child.after);
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
        self.expand_with(&[], &[])
    }

    fn expand_with(
        self,
        inherited_before: &[String],
        inherited_after: &[String],
    ) -> Vec<PluginSpec> {
        let PluginSpec {
            name,
            installers,
            before,
            after,
            children,
        } = self;
        let before: Vec<_> = inherited_before.iter().chain(&before).cloned().collect();
        let after: Vec<_> = inherited_after.iter().chain(&after).cloned().collect();

        let mut expanded = Vec::with_capacity(1 + children.len());
        expanded.push(PluginSpec {
            name,
            installers,
            before: before.clone(),
            after: after.clone(),
            children: Vec::new(),
        });
        for child in children {
            expanded.extend(child.expand_with(&before, &after));
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
    /// Panics before registration if the plugin is invalid, a dependency is
    /// missing or cyclic, or the plugin is already installed.
    pub fn install(self, md: &mut MarkdownIt) {
        install_plugins(md, self.expand());
    }

    fn install_validated(self, md: &mut MarkdownIt) {
        for installer in self.installers {
            installer.install(md);
        }
        md.ext
            .get_or_insert_default::<InstalledPlugins>()
            .0
            .insert(self.name);
    }
}

impl Plugin for PluginSpec {
    fn into_spec(self) -> PluginSpec {
        self
    }
}

#[derive(Default)]
pub(super) struct RegistrationContext {
    pub rules: HashSet<std::any::TypeId>,
    pub pairs: HashMap<char, crate::parser::inline::helpers::emph_pair::PairRegistration>,
}

pub(super) trait PluginInstaller: Send {
    fn name(&self) -> &str;
    fn validate(&self);
    fn validate_registration(&self, _md: &MarkdownIt, _context: &mut RegistrationContext) {}
    fn install(self: Box<Self>, md: &mut MarkdownIt);
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

    fn install(self: Box<Self>, md: &mut MarkdownIt) {
        let Self { install, .. } = *self;
        install(md);
    }
}

// Validate the whole batch before installing anything. Previously installed
// plugins are fixed before this batch, so `before(existing)` is rejected.
fn installation_order(md: &MarkdownIt, plugins: &[PluginSpec]) -> Vec<usize> {
    let installed: HashSet<&str> = md
        .ext
        .get::<InstalledPlugins>()
        .map(|plugins| plugins.0.iter().map(String::as_str).collect())
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
        let item = order.add(plugin.name.clone(), index);
        for name in &plugin.before {
            assert!(
                !installed.contains(name.as_str()),
                "plugin {:?} cannot be ordered before installed plugin {name:?}",
                plugin.name
            );
            assert!(
                names.contains(name.as_str()),
                "missing dependency: {:?} requires {:?}",
                plugin.name,
                name
            );
            item.require(name.clone()).before(name.clone());
        }
        for name in &plugin.after {
            assert!(
                names.contains(name.as_str()),
                "missing dependency: {:?} requires {:?}",
                plugin.name,
                name
            );
            if !installed.contains(name.as_str()) {
                item.require(name.clone()).after(name.clone());
            }
        }
    }
    order.as_slice().to_vec()
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
}
