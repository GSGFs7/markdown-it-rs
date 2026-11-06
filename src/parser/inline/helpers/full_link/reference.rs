use crate::plugins::cmark::block::reference::ReferenceMap;

pub(super) struct ParseLinkResult {
    pub(super) label_start: usize,
    pub(super) label_end: usize,
    pub(super) href: Option<String>,
    pub(super) title: Option<String>,
    pub(super) end: usize,
}

pub(super) fn resolve_reference_link(
    source: &str,
    label_start: usize,
    label_end: usize,
    reference_end: Option<(usize, usize)>,
    references: &ReferenceMap,
) -> Option<ParseLinkResult> {
    let (label, end) = match reference_end {
        Some((start, close)) => {
            let explicit_label = &source[start..close];
            let label = if explicit_label.is_empty() {
                &source[label_start..label_end]
            } else {
                explicit_label
            };
            (label, close + 1)
        }
        None => (&source[label_start..label_end], label_end + 1),
    };

    let (destination, title) = references.get(label)?;
    Some(ParseLinkResult {
        label_start,
        label_end,
        href: Some(destination.to_owned()),
        title: title.map(str::to_owned),
        end,
    })
}
