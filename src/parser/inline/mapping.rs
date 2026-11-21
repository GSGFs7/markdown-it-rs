/// Mapping from inline byte offsets to document byte offsets.
///
/// Entries are `(inline_offset, source_offset)` pairs. The first inline offset
/// must be zero, and inline offsets must be in nondecreasing order. An empty
/// mapping or one that fails these checks yields no inline source maps.
///
/// An individual range yields no map if either endpoint cannot be mapped,
/// offset arithmetic overflows, or the mapped start exceeds the mapped end.
/// Mapped ranges are hints and may fall outside the document source or split
/// a UTF-8 character. To read the source text, use
/// `document.source().get(start..end)`.
#[cfg(feature = "sourcemap")]
pub type InlineMapping = Vec<(usize, usize)>;

/// Zero-sized mapping carrier used when source mapping is disabled.
///
/// Custom block rules can use `InlineMapping::default()` in this configuration.
#[cfg(not(feature = "sourcemap"))]
#[derive(Debug, Default, Clone, Copy)]
pub struct InlineMapping;
