pub(super) struct ParseLinkResult {
    pub(super) label_start: usize,
    pub(super) label_end: usize,
    pub(super) href: Option<String>,
    pub(super) title: Option<String>,
    pub(super) end: usize,
}
