/// Mapping from inline byte offsets to document byte offsets.
///
/// Without the `sourcemap` feature this is a zero-sized carrier; custom block
/// rules can use `InlineMapping::default()` when source mapping is disabled.
#[cfg(feature = "sourcemap")]
pub type InlineMapping = Vec<(usize, usize)>;

/// Zero-sized mapping carrier used when source mapping is disabled.
#[cfg(not(feature = "sourcemap"))]
#[derive(Debug, Default, Clone, Copy)]
pub struct InlineMapping;
