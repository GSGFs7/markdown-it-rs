//! High-level inline containers.
//!
//! A container parses its body (or multiple parts) as independent child inline
//! sessions, groups each parsed range under an [`InlineContainerPart`], and
//! attaches those parts to a payload node. Use
//! [`PluginSpec::inline_container`](crate::plugin::PluginSpec::inline_container)
//! to build one.

use std::marker::PhantomData;
use std::ops::Range;
use std::sync::Arc;

use crate::MarkdownIt;
use crate::document::{Node, NodeId, NodeValue};
use crate::parser::inline::InlineState;
use crate::parser::inline::helpers::paired_span::{self, PairMatch, PairOptions};
use crate::plugin::container_context::{ContainerHtml, ContainerText};
use crate::plugin::html::validate_tag;
use crate::plugin::renderer::{
    ContainerHtmlRenderer,
    ContainerTextRenderer,
    Renderers,
    WithRenderer,
};
use crate::plugin::spec::{PluginInstallContext, PluginInstaller};
use crate::plugin::{InlinePluginState, PluginSpec};
use crate::render::{NodeRenderer, RenderContext, Writer, write_html_close, write_html_open};

/// Cursor passed to an inline container parser.
///
/// The cursor starts at the container's first byte. Consumption is committed
/// only when the callback returns `Some`, so a failed match rolls back
/// automatically. The public API records source ranges, which the builder
/// parses after a successful match. Pair scans run inline rule checks, so
/// callbacks must be side-effect free. Matching may depend on the active inline
/// rules, `max_nesting` and shared caches.
///
/// ## Cursor and parts
///
/// | Method                                 | Advances cursor | Creates part |
/// | -------------------------------------- | --------------- | ------------ |
/// | [`consume`](Self::consume)             | yes             | no           |
/// | [`consume_char`](Self::consume_char)   | yes             | no           |
/// | [`consume_pair`](Self::consume_pair)   | yes             | no           |
/// | [`advance_bytes`](Self::advance_bytes) | yes             | no           |
/// | [`scan_pair`](Self::scan_pair)         | no              | no           |
/// | [`part`](Self::part)                   | no              | yes          |
/// | [`part_at`](Self::part_at)             | no              | yes          |
/// | [`consume_part`](Self::consume_part)   | yes             | yes          |
///
/// Recorded ranges are parsed as independent [`InlineContainerPart`] children;
/// see that type for their semantics.
pub struct InlineContainerState<'a> {
    cursor: InlinePluginState<'a>,
    boundary: InlineState<'a>,
    markdown_it: &'a MarkdownIt,
    parts: Vec<Range<usize>>,
}

impl<'a> InlineContainerState<'a> {
    fn new(state: &'a mut InlineState<'_>) -> Self {
        let markdown_it = state.markdown_it();
        let depth = state.depth;
        let link_level = state.link_level;
        let end = state.remaining().len();
        let session = state
            .child_session_at(0..end, depth, link_level)
            .expect("valid container source window");
        Self {
            cursor: InlinePluginState::new(session.source, markdown_it),
            boundary: session.state,
            markdown_it,
            parts: Vec::new(),
        }
    }

    /// Unconsumed source available to this rule.
    ///
    /// The returned reference borrows the underlying source, not `self`, so it
    /// remains usable after advancing the cursor.
    pub fn remaining(&self) -> &'a str {
        self.cursor.remaining()
    }

    /// Return the part of [`remaining`](Self::remaining) covered by `range`.
    ///
    /// Offsets are relative to the remaining source. Returns `None` if the
    /// range is reversed, out of bounds, or not on UTF-8 boundaries.
    pub fn remaining_slice(&self, range: Range<usize>) -> Option<&'a str> {
        self.cursor.remaining_slice(range)
    }

    /// The complete source window visible to this rule.
    ///
    /// The returned reference borrows the underlying source, not `self`.
    pub fn source(&self) -> &'a str {
        self.cursor.source()
    }

    /// Return the part of [`source`](Self::source) covered by `range`.
    ///
    /// Offsets are relative to the complete source window. Returns `None` if
    /// the range is reversed, out of bounds, or not on UTF-8 boundaries.
    pub fn source_slice(&self, range: Range<usize>) -> Option<&'a str> {
        self.cursor.source_slice(range)
    }

    /// Parser instance that owns the active inline rule.
    pub fn markdown_it(&self) -> &'a MarkdownIt {
        self.markdown_it
    }

    /// Number of bytes consumed so far.
    pub fn consumed_bytes(&self) -> usize {
        self.cursor.consumed_bytes()
    }

    /// Whether the unconsumed source starts with `literal`.
    pub fn starts_with(&self, literal: &str) -> bool {
        self.cursor.starts_with(literal)
    }

    /// Consume `literal` if it is next, returning whether it matched.
    pub fn consume(&mut self, literal: &str) -> bool {
        self.cursor.consume(literal)
    }

    /// Consume one character, returning it when one is available.
    pub fn consume_char(&mut self) -> Option<char> {
        self.cursor.consume_char()
    }

    /// Advance the cursor by exactly `bytes` bytes.
    ///
    /// Returns `false` without moving the cursor when the range is outside the
    /// source or not on a UTF-8 boundary.
    pub fn advance_bytes(&mut self, bytes: usize) -> bool {
        self.cursor.advance_bytes(bytes)
    }

    /// Scan a paired span at the start of [`remaining`](Self::remaining).
    ///
    /// Matched inline constructs (code spans, links) are skipped, so their
    /// markers cannot close or nest the pair. Nested container rules do not
    /// run; nesting follows `PairOptions`. At the depth limit the body is
    /// scanned literally. No child nodes are emitted.
    ///
    /// Offsets are relative to the remaining source. Use
    /// [`consume_pair`](Self::consume_pair) to scan and advance in one step.
    /// Results depend on the active inline rules and `max_nesting`.
    pub fn scan_pair(&mut self, options: &PairOptions<'_>) -> Option<PairMatch> {
        let offset = self.consumed_bytes();
        paired_span::scan_inline(&mut self.boundary, offset, options)
    }

    /// Scan a paired span and consume its bytes.
    ///
    /// The returned offsets are relative to the previous
    /// [`remaining`](Self::remaining), before the cursor advances.
    pub fn consume_pair(&mut self, options: &PairOptions<'_>) -> Option<PairMatch> {
        let matched = self.scan_pair(options)?;
        self.advance_bytes(matched.consumed).then_some(matched)
    }

    /// Record a part covering `range` of [`remaining`](Self::remaining).
    ///
    /// Offsets are relative to the current cursor position. This does not move
    /// the cursor. Call it before [`advance_bytes`](Self::advance_bytes) when the part
    /// starts at the current position.
    ///
    /// The recorded range is validated against the final consumed span after
    /// the callback returns. An invalid range is a plugin implementation error
    /// and panics.
    pub fn part(&mut self, range: Range<usize>) {
        let base = self.consumed_bytes();
        let start = base.saturating_add(range.start);
        let end = base.saturating_add(range.end);
        self.parts.push(start..end);
    }

    /// Record a part using offsets relative to the complete container source.
    ///
    /// Unlike [`part`](Self::part), this does not depend on the current cursor
    /// position. This does not move the cursor. The range is validated against
    /// the final consumed span after the callback returns.
    pub fn part_at(&mut self, range: Range<usize>) {
        self.parts.push(range);
    }

    /// Scan a paired span at the current cursor, consume it, and record its
    /// body as a part.
    ///
    /// The returned match uses the same offsets as
    /// [`consume_pair`](Self::consume_pair).
    pub fn consume_part(&mut self, options: &PairOptions<'_>) -> Option<PairMatch> {
        let base = self.consumed_bytes();
        let matched = self.consume_pair(options)?;
        let start = base.saturating_add(matched.body.start);
        let end = base.saturating_add(matched.body.end);
        self.parts.push(start..end);
        Some(matched)
    }

    fn take_parts(&mut self) -> Vec<Range<usize>> {
        std::mem::take(&mut self.parts)
    }
}

pub(super) struct ContainerTagRenderer<T> {
    tag: String,
    payload: PhantomData<fn() -> T>,
}

impl<T> ContainerTagRenderer<T> {
    fn new(tag: String) -> Self {
        Self {
            tag,
            payload: PhantomData,
        }
    }
}

impl<T: NodeValue> NodeRenderer<T> for ContainerTagRenderer<T> {
    fn render(&self, node: &Node, _: &T, context: &mut RenderContext<'_>, output: &mut Writer) {
        write_html_open(output, &self.tag, node.attrs());
        context.render_children(node.id(), output);
        write_html_close(output, &self.tag);
    }
}

type ParseFn<T> =
    Arc<dyn for<'a> Fn(&mut InlineContainerState<'a>) -> Option<T> + Send + Sync + 'static>;

/// Builds one inline container inside a [`PluginSpec`].
///
/// A parser and at least one renderer are required before [`finish`](Self::finish).
/// The parser records parts through [`InlineContainerState`]; see that type for
/// cursor and part-recording methods. The marker defaults to `'\0'`, which
/// tries matching at every position.
#[must_use = "call finish() to add this container to its plugin"]
pub struct InlineContainerBuilder<T: NodeValue, P = (), R = ()> {
    plugin: PluginSpec,
    rule: String,
    marker: char,
    parse: P,
    render: R,
    renderers: Renderers,
    payload: PhantomData<fn() -> T>,
}

impl<T: NodeValue> InlineContainerBuilder<T> {
    pub(in crate::plugin) fn new(plugin: PluginSpec, rule: String) -> Self {
        Self {
            plugin,
            rule,
            marker: '\0',
            parse: (),
            render: (),
            renderers: Renderers::default(),
            payload: PhantomData,
        }
    }
}

impl<T: NodeValue, P, R> InlineContainerBuilder<T, P, R> {
    /// Set the character that may start this container.
    pub fn marker(mut self, marker: char) -> Self {
        self.marker = marker;
        self
    }

    /// Set the parser.
    ///
    /// The callback returns the payload and records parts with
    /// [`InlineContainerState`]. Each part range must lie inside the consumed
    /// bytes and on UTF-8 boundaries; invalid ranges panic. Returning `None`
    /// rolls back local consumption, and a successful parse must consume at
    /// least one byte. The callback may also run during boundary probing, so
    /// keep it free of observable side effects.
    ///
    /// Render parts with [`ContainerHtml::render_part`] or
    /// [`ContainerText::render_part`]; [`render_children`] renders all parts in
    /// order.
    ///
    /// ```
    /// use markdown_it::MarkdownIt;
    /// use markdown_it::parser::inline::helpers::paired_span::{
    ///     EscapePolicy, NestingPolicy, PairOptions,
    /// };
    /// use markdown_it::plugin::PluginSpec;
    ///
    /// #[derive(Debug)]
    /// struct Annotation;
    ///
    /// fn annotation() -> PluginSpec {
    ///     let bracket = PairOptions::new("[", "]")
    ///         .escape(EscapePolicy::Backslash)
    ///         .nesting(NestingPolicy::SamePair);
    ///     let brace = PairOptions::new("{", "}")
    ///         .escape(EscapePolicy::Backslash)
    ///         .nesting(NestingPolicy::SamePair);
    ///
    ///     PluginSpec::new("annotation")
    ///         .inline_container::<Annotation>("annotation")
    ///         .marker('[')
    ///         .parse(move |cx| {
    ///             cx.consume_part(&bracket)?;
    ///             cx.consume_part(&brace)?;
    ///             Some(Annotation)
    ///         })
    ///         .render_html(|_, html| {
    ///             let label = html.render_part(0);
    ///             let note = html.render_part(1);
    ///             html.element("ruby").raw(label).element("rt").raw(note);
    ///         })
    ///         .finish()
    /// }
    ///
    /// let md = MarkdownIt::builder().plugin(annotation()).build();
    /// assert_eq!(
    ///     md.render("[**text**]{*note*}"),
    ///     "<p><ruby><strong>text</strong><rt><em>note</em></rt></ruby></p>\n"
    /// );
    /// assert_eq!(
    ///     md.render("[some `]` text]{note}"),
    ///     "<p><ruby>some <code>]</code> text<rt>note</rt></ruby></p>\n"
    /// );
    /// ```
    ///
    /// [`render_children`]: ContainerHtml::render_children
    pub fn parse<F>(self, parse: F) -> InlineContainerBuilder<T, F, R>
    where
        F: for<'a> Fn(&mut InlineContainerState<'a>) -> Option<T> + Send + Sync + 'static,
    {
        InlineContainerBuilder {
            plugin: self.plugin,
            rule: self.rule,
            marker: self.marker,
            parse,
            render: self.render,
            renderers: self.renderers,
            payload: PhantomData,
        }
    }

    /// Register an HTML callback that can render the container's children.
    ///
    /// Use [`ContainerHtml::render_children`] to render the parsed body.
    /// Repeated registrations for the same format replace earlier ones.
    pub fn render_html<F>(self, render: F) -> InlineContainerBuilder<T, P, WithRenderer>
    where
        F: for<'a, 'b> Fn(&T, &mut ContainerHtml<'a, 'b>) + Send + Sync + 'static,
    {
        self.render_with("html", ContainerHtmlRenderer::<F, T, false>::new(render))
    }

    /// Register a plain-text callback that can render the container's children.
    ///
    /// Use [`ContainerText::render_children`] to render the parsed body.
    pub fn render_text<F>(self, render: F) -> InlineContainerBuilder<T, P, WithRenderer>
    where
        F: for<'a, 'b> Fn(&T, &mut ContainerText<'a, 'b>) + Send + Sync + 'static,
    {
        self.render_with("text", ContainerTextRenderer::<F, T, false>::new(render))
    }

    /// Wrap this container's children in an HTML tag.
    ///
    /// This is a shortcut for [`render_with`](Self::render_with). Existing node
    /// attributes are written on the tag. The last renderer registered for a
    /// payload type and format wins.
    ///
    /// # Panics
    ///
    /// Panics if `tag` is not a valid ASCII HTML tag name.
    pub fn html_tag(self, tag: impl Into<String>) -> InlineContainerBuilder<T, P, WithRenderer> {
        let tag = tag.into();
        validate_tag(&tag);
        self.render_with("html", ContainerTagRenderer::<T>::new(tag))
    }

    /// Register a low-level renderer for an output format.
    ///
    /// May be called repeatedly for different formats. The renderer can use
    /// [`RenderContext::render_children`] to emit the container's parsed
    /// children in the active output format.
    pub fn render_with<N: NodeRenderer<T>>(
        mut self,
        format: impl Into<String>,
        renderer: N,
    ) -> InlineContainerBuilder<T, P, WithRenderer> {
        let format = format.into();
        self.renderers
            .add::<InlineContainerPart, _>(&format, PartRenderer);
        self.renderers.add::<T, _>(format, renderer);
        InlineContainerBuilder {
            plugin: self.plugin,
            rule: self.rule,
            marker: self.marker,
            parse: self.parse,
            render: WithRenderer,
            renderers: self.renderers,
            payload: PhantomData,
        }
    }
}

impl<T: NodeValue, P> InlineContainerBuilder<T, P, WithRenderer>
where
    P: for<'a> Fn(&mut InlineContainerState<'a>) -> Option<T> + Send + Sync + 'static,
{
    /// Add the container to its owning plugin and resume building that plugin.
    pub fn finish(self) -> PluginSpec {
        let installer = ConfiguredInlineContainer::<T> {
            rule: self.rule,
            marker: self.marker,
            parse: Arc::new(self.parse),
        };
        self.plugin.with_installer(self.renderers.attach(installer))
    }
}

/// One independently parsed inline segment of an inline container.
///
/// [`InlineContainerState`] creates one part per recorded range, preserving
/// order and including empty ranges. Parts are parsed in separate inline
/// sessions, so delimiters never pair across them. Container builders register
/// a transparent renderer for this type in every configured output format, and
/// [`ContainerHtml::render_part`] / [`ContainerText::render_part`] render an
/// individual part.
#[derive(Debug)]
pub struct InlineContainerPart;

struct PartRenderer;

impl NodeRenderer<InlineContainerPart> for PartRenderer {
    fn render(
        &self,
        node: &Node,
        _: &InlineContainerPart,
        context: &mut RenderContext<'_>,
        output: &mut Writer,
    ) {
        context.render_children(node.id(), output);
    }
}

struct ConfiguredInlineContainer<T: NodeValue> {
    rule: String,
    marker: char,
    parse: ParseFn<T>,
}

impl<T: NodeValue> ConfiguredInlineContainer<T> {
    fn run(&self, state: &mut InlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        let (value, parts, consumed) = {
            let mut cx = InlineContainerState::new(state);
            let value = (self.parse)(&mut cx)?;
            (value, cx.take_parts(), cx.consumed_bytes())
        };

        assert_progress(&self.rule, state.remaining(), consumed);
        for range in &parts {
            assert_part_range(&self.rule, state.remaining(), range, consumed);
        }

        let node = state.document.create_node(value);
        for range in parts {
            #[cfg(feature = "sourcemap")]
            let map = state.get_map(state.pos + range.start, state.pos + range.end);
            let children = state
                .parse_subrange(range)
                .expect("validated container part range");
            let part = state.document.create_node(InlineContainerPart);
            state.document.node_mut(part).set_transparent(true);
            #[cfg(feature = "sourcemap")]
            state.document.node_mut(part).set_srcmap(map);
            for child in children {
                state.document.push_child(part, child);
            }
            state.document.push_child(node, part);
        }
        Some((Some(node), consumed))
    }

    fn check(&self, state: &mut InlineState<'_>) -> Option<usize> {
        if state.suppress_inline_containers {
            return None;
        }

        let (parts, consumed) = {
            let mut cx = InlineContainerState::new(state);
            (self.parse)(&mut cx)?;
            (cx.take_parts(), cx.consumed_bytes())
        };

        for range in &parts {
            assert_part_range(&self.rule, state.remaining(), range, consumed);
        }
        assert_progress(&self.rule, state.remaining(), consumed);

        Some(consumed)
    }
}

impl<T: NodeValue> PluginInstaller for ConfiguredInlineContainer<T> {
    fn name(&self) -> &str {
        &self.rule
    }

    fn validate(&self) {
        assert!(
            !self.rule.is_empty(),
            "inline container rule name must not be empty"
        );
    }

    fn install(self: Box<Self>, md: &mut MarkdownIt, context: &PluginInstallContext<'_>) {
        let container = Arc::new(*self);
        let mark = crate::plugin::identity::rule_mark("inline", context.name, &container.rule);

        let run_container = Arc::clone(&container);
        let run = Arc::new(move |state: &mut InlineState<'_>| run_container.run(state));

        let check_container = Arc::clone(&container);
        let check = Arc::new(move |state: &mut InlineState<'_>| check_container.check(state));

        md.inline
            .add_dynamic_rule(mark, container.marker, run, check)
            .after_named("text")
            .before_all();
    }
}

pub(super) fn assert_part_range(rule: &str, source: &str, range: &Range<usize>, consumed: usize) {
    assert!(
        range.end <= consumed && source.get(range.clone()).is_some(),
        "inline container rule {rule:?} recorded an invalid part range {range:?} for {consumed} consumed bytes"
    );
}

pub(super) fn assert_progress(rule: &str, source: &str, consumed: usize) {
    assert!(
        consumed > 0,
        "inline container rule {rule:?} succeeded without consuming input"
    );
    debug_assert!(
        source.get(..consumed).is_some(),
        "inline container rule {rule:?} consumed past the source window or split a UTF-8 character"
    );
}
