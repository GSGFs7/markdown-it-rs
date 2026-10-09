#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RenderOptions {
    /// output `<br />`, `<hr />`, `<img />`...
    pub xhtml_out: bool,
    /// render line breaks as `<br>`
    pub breaks: bool,
    /// cover fenced code class prefixes
    pub lang_prefix: Option<String>,
}
