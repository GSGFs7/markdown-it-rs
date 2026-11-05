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

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use super::Writer;
    use crate::render::tests::{FailingDisplay, panic_message};

    #[test]
    fn escaped_writes_match_legacy_encoding_across_fast_path_boundary() {
        for len in [0, 1, 31, 32, 33, 64, 1024] {
            let plain = "a".repeat(len);
            for marker in ['&', '<', '>', '"', '\'', '/', '\0', '雪', '🦀'] {
                for pos in [0, len / 2, len] {
                    let mut input = plain.clone();
                    input.insert(pos, marker);
                    let mut output = Writer::new();
                    output.write_str("prefix:");
                    output.write_escaped_html(&input);
                    output.write_escaped_html("&\"");
                    output.write_str(":suffix");
                    assert_eq!(
                        output.finish(),
                        format!(
                            "prefix:{}&amp;&quot;:suffix",
                            crate::common::utils::escape_html(&input),
                        ),
                        "escaping differs for {input:?}",
                    );
                }
            }
        }
    }

    #[test]
    fn writer_supports_inherent_string_char_and_format_writes() {
        let mut output = Writer::new();
        output.write_str("hé");
        output.write_char('🦀');
        write!(output, "{}", 42);
        let suffix = "end";
        writeln!(output, "-{suffix}");

        assert_eq!(output.finish(), "hé🦀42-end\n");
    }

    #[test]
    fn inherent_write_fmt_panics_on_formatting_error() {
        let mut output = Writer::new();

        let panic = catch_unwind(AssertUnwindSafe(|| {
            write!(output, "{}", FailingDisplay);
        }))
        .expect_err("inherent write_fmt must panic when formatting fails");
        let message = panic_message(panic);

        assert!(message.contains("failed"), "{message}");
    }

    fn write_through_generic<W: std::fmt::Write>(writer: &mut W) {
        writer.write_str("generic").unwrap();
        write!(writer, "-{}", 7).unwrap();
    }

    #[test]
    fn writer_interoperates_with_generic_and_trait_object_fmt_write() {
        let mut output = Writer::new();
        write_through_generic(&mut output);
        output.write_char('|');

        let dyn_writer: &mut dyn std::fmt::Write = &mut output;
        dyn_writer.write_str("dyn").unwrap();
        write!(dyn_writer, "-{}", 9).unwrap();

        assert_eq!(output.finish(), "generic-7|dyn-9");
    }

    #[test]
    fn trait_write_fmt_propagates_formatting_error() {
        let mut output = Writer::new();

        let result =
            <Writer as std::fmt::Write>::write_fmt(&mut output, format_args!("{}", FailingDisplay));

        assert!(result.is_err());
    }
}
