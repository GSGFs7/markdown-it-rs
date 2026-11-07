/// Cursor passed to an inline leaf parser.
///
/// The cursor is local to one callback. Consumption is committed only when the
/// callback returns `Some`, so a failed match rolls back automatically.
#[derive(Debug)]
pub struct InlinePluginState<'a> {
    source: &'a str,
    consumed: usize,
}

impl<'a> InlinePluginState<'a> {
    pub(super) fn new(source: &'a str) -> Self {
        Self {
            source,
            consumed: 0,
        }
    }

    /// Unconsumed source available to this rule.
    pub fn remaining(&self) -> &'a str {
        &self.source[self.consumed..]
    }

    /// The complete source window visible to this rule.
    pub fn source(&self) -> &'a str {
        self.source
    }

    /// Number of bytes consumed so far.
    pub fn consumed(&self) -> usize {
        self.consumed
    }

    /// Whether the unconsumed source starts with `literal`.
    pub fn starts_with(&self, literal: &str) -> bool {
        self.remaining().starts_with(literal)
    }

    /// Consume `literal` if it is next, returning whether it matched.
    pub fn consume(&mut self, literal: &str) -> bool {
        if self.remaining().starts_with(literal) {
            self.consumed += literal.len();
            true
        } else {
            false
        }
    }

    /// Consume one character, returning it when one is available.
    pub fn consume_char(&mut self) -> Option<char> {
        let ch = self.remaining().chars().next()?;
        self.consumed += ch.len_utf8();
        Some(ch)
    }

    /// Consume exactly `bytes` bytes.
    ///
    /// Returns `false` without moving the cursor when the range is outside the
    /// source or not on a UTF-8 boundary.
    pub fn advance(&mut self, bytes: usize) -> bool {
        if self.remaining().get(bytes..).is_none() {
            return false;
        }
        self.consumed += bytes;
        true
    }
}
