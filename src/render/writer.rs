//! Output buffer handed to `NodeRenderer` implementations.

use std::fmt;

use crate::common::utils::escape_html;

/// Output buffer supplied to [`NodeRenderer`](super::NodeRenderer) implementations.
///
/// The inherent [`write_str`](Self::write_str), [`write_char`](Self::write_char),
/// and [`write_fmt`](Self::write_fmt) methods return `()` and panic on a
/// formatting error. The type also implements [`fmt::Write`], whose methods
/// return [`fmt::Result`], for interoperability with generic code.
#[derive(Debug, Default)]
pub struct Writer {
    output: String,
}

impl Writer {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(super) fn cr(&mut self) {
        if !self.output.is_empty() && !self.output.ends_with('\n') {
            self.output.push('\n');
        }
    }

    pub(crate) fn finish(self) -> String {
        self.output
    }

    /// Append a string to the output buffer.
    pub fn write_str(&mut self, value: &str) {
        self.output.push_str(value);
    }

    /// Append a char to the output buffer.
    pub fn write_char(&mut self, value: char) {
        self.output.push(value);
    }

    /// Avoid scalar entity detection for long text that needs no escaping.
    #[inline]
    pub(crate) fn write_escaped_html(&mut self, value: &str) {
        if value.len() >= 32
            && memchr::memchr3(b'&', b'<', b'>', value.as_bytes()).is_none()
            && memchr::memchr(b'"', value.as_bytes()).is_none()
        {
            self.output.push_str(value);
        } else {
            self.output.push_str(&escape_html(value));
        }
    }

    /// Append formatted output.
    ///
    /// # Panics
    /// Panic if a formatting implementation returns an error.
    pub fn write_fmt(&mut self, args: fmt::Arguments<'_>) {
        fmt::write(&mut self.output, args)
            .expect("a formatting implementation failed while rendering");
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.output.push_str(s);
        Ok(())
    }
}
