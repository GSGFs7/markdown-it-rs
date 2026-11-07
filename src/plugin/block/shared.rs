//! Shared cursors, registration, and dispatch for block plugins.
//!
//! One static [`BlockRule`] adapter dispatches to all configured block leaves
//! and containers. Callbacks live in [`MarkdownIt::ext`], so the parser core
//! needs no dynamic block-rule representation.

use std::fmt;
use std::sync::Arc;

use crate::MarkdownIt;
use crate::document::NodeId;
use crate::parser::block::{BlockRule, BlockState, LineOffset};

/// Mutable cursor passed to block leaf parsers.
///
/// A leaf commits consumption only when its callback returns `Some`, so a
/// failed match rolls back automatically. Line offsets are relative to the
/// current cursor, starting at the rule's first line. Block containers use the
/// read-only [`BlockContainerState`] instead.
#[derive(Debug)]
pub struct BlockPluginState<'a> {
    source: &'a str,
    line_offsets: &'a [LineOffset],
    start: usize,
    line_max: usize,
    blk_indent: usize,
    consumed: usize,
    markdown_it: &'a MarkdownIt,
}

impl<'a> BlockPluginState<'a> {
    pub(super) fn new(
        source: &'a str,
        line_offsets: &'a [LineOffset],
        start: usize,
        line_max: usize,
        blk_indent: usize,
        markdown_it: &'a MarkdownIt,
    ) -> Self {
        Self {
            source,
            line_offsets,
            start,
            line_max,
            blk_indent,
            consumed: 0,
            markdown_it,
        }
    }

    fn index(&self, offset: usize) -> Option<usize> {
        let index = self.start.checked_add(self.consumed)?.checked_add(offset)?;
        (index < self.line_max).then_some(index)
    }

    /// Return a line by offset, or `None` past the block window.
    ///
    /// Leading indentation and the line ending are stripped.
    pub fn line(&self, offset: usize) -> Option<&'a str> {
        let index = self.index(offset)?;
        let offsets = self.line_offsets.get(index)?;
        Some(&self.source[offsets.first_nonspace..offsets.line_end])
    }

    /// Return a line's indentation relative to the active block container.
    ///
    /// Returns `None` past the block window. The value can be negative when a
    /// line outdents the current list item.
    pub fn line_indent(&self, offset: usize) -> Option<i32> {
        let index = self.index(offset)?;
        let offsets = self.line_offsets.get(index)?;
        Some(offsets.indent_nonspace - self.blk_indent as i32)
    }

    /// Return whether a line is empty, or `None` past the block window.
    pub fn is_empty(&self, offset: usize) -> Option<bool> {
        let index = self.index(offset)?;
        Some(self.line_offsets[index].first_nonspace >= self.line_offsets[index].line_end)
    }

    /// The complete Markdown source visible to the block parser.
    pub fn source(&self) -> &'a str {
        self.source
    }

    /// Parser instance that owns the active block rule.
    pub fn markdown_it(&self) -> &'a MarkdownIt {
        self.markdown_it
    }

    /// Number of lines consumed so far.
    pub fn consumed_lines(&self) -> usize {
        self.consumed
    }

    /// Number of lines left in the current block window.
    pub fn remaining_lines(&self) -> usize {
        self.line_max
            .saturating_sub(self.start)
            .saturating_sub(self.consumed)
    }

    /// Consume `lines` lines.
    ///
    /// Returns `false` without moving the cursor if they fall outside the
    /// current block window.
    pub fn advance_lines(&mut self, lines: usize) -> bool {
        let Some(consumed) = self.consumed.checked_add(lines) else {
            return false;
        };
        if self
            .start
            .checked_add(consumed)
            .is_none_or(|end| end > self.line_max)
        {
            return false;
        }
        self.consumed = consumed;
        true
    }
}

/// Read-only line view passed to block container parsers.
///
/// Consumption is defined solely by the returned
/// [`BlockContainerParse`](crate::plugin::BlockContainerParse) line counts;
/// offsets are relative to the container's first line.
#[derive(Debug, Clone, Copy)]
pub struct BlockContainerState<'a> {
    source: &'a str,
    line_offsets: &'a [LineOffset],
    start: usize,
    line_max: usize,
    blk_indent: usize,
    markdown_it: &'a MarkdownIt,
}

impl<'a> BlockContainerState<'a> {
    pub(super) fn new(
        source: &'a str,
        line_offsets: &'a [LineOffset],
        start: usize,
        line_max: usize,
        blk_indent: usize,
        markdown_it: &'a MarkdownIt,
    ) -> Self {
        Self {
            source,
            line_offsets,
            start,
            line_max,
            blk_indent,
            markdown_it,
        }
    }

    fn index(&self, offset: usize) -> Option<usize> {
        let index = self.start.checked_add(offset)?;
        (index < self.line_max).then_some(index)
    }

    /// Return a line by offset, or `None` past the block window.
    ///
    /// Leading indentation and the line ending are stripped.
    pub fn line(&self, offset: usize) -> Option<&'a str> {
        let index = self.index(offset)?;
        let offsets = self.line_offsets.get(index)?;
        Some(&self.source[offsets.first_nonspace..offsets.line_end])
    }

    /// Return a line's indentation relative to the active block container.
    ///
    /// Returns `None` past the block window. The value can be negative when a
    /// line outdents the current list item.
    pub fn line_indent(&self, offset: usize) -> Option<i32> {
        let index = self.index(offset)?;
        let offsets = self.line_offsets.get(index)?;
        Some(offsets.indent_nonspace - self.blk_indent as i32)
    }

    /// Return whether a line is empty, or `None` past the block window.
    pub fn is_empty(&self, offset: usize) -> Option<bool> {
        let index = self.index(offset)?;
        Some(self.line_offsets[index].first_nonspace >= self.line_offsets[index].line_end)
    }

    /// The complete Markdown source visible to the block parser.
    pub fn source(&self) -> &'a str {
        self.source
    }

    /// Parser instance that owns the active block rule.
    pub fn markdown_it(&self) -> &'a MarkdownIt {
        self.markdown_it
    }

    /// Number of lines left in the current block window.
    pub fn remaining_lines(&self) -> usize {
        self.line_max.saturating_sub(self.start)
    }
}

/// Type-erased block plugin rule stored in the parser extension set.
pub(in crate::plugin) trait ErasedBlockPlugin: fmt::Debug + Send + Sync {
    fn check(&self, state: &BlockState<'_>) -> Option<()>;
    fn run(&self, state: &mut BlockState<'_>) -> Option<(Option<NodeId>, usize)>;
}

/// All configured block plugins share one static [`BlockRule`] adapter, keeping
/// the parser core unaware of dynamic block rules.
#[derive(Debug, Default)]
struct BlockPluginRegistry {
    plugins: Vec<Arc<dyn ErasedBlockPlugin>>,
}

#[derive(Debug)]
struct BlockPluginRule;

impl BlockRule for BlockPluginRule {
    const NAMES: &'static [&'static str] = &["plugin_block"];

    fn check(state: &mut BlockState<'_>) -> Option<()> {
        let registry = state.md.ext.get::<BlockPluginRegistry>()?;
        registry
            .plugins
            .iter()
            .find_map(|plugin| plugin.check(state))
    }

    fn run(state: &mut BlockState<'_>) -> Option<(Option<NodeId>, usize)> {
        let registry = state.md.ext.get::<BlockPluginRegistry>()?;
        registry.plugins.iter().find_map(|plugin| plugin.run(state))
    }
}

pub(in crate::plugin) fn install_block_plugin(
    md: &mut MarkdownIt,
    plugin: Arc<dyn ErasedBlockPlugin>,
) {
    md.ext
        .get_or_insert_default::<BlockPluginRegistry>()
        .plugins
        .push(plugin);

    if !md.block.has_rule::<BlockPluginRule>() {
        md.block
            .add_rule::<BlockPluginRule>()
            .before_named("paragraph");
    }
}

pub(in crate::plugin) fn block_window_end(state: &BlockState<'_>) -> usize {
    // End the window before the first non-empty line that outdents past the
    // current container (relevant inside list items).
    let mut window_end = state.line + 1;
    while window_end < state.line_max {
        if !state.is_empty(window_end) && state.line_indent(window_end) < 0 {
            break;
        }
        window_end += 1;
    }
    window_end
}
