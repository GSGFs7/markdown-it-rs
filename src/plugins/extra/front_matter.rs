use crate::parser::block::{BlockRule, DocumentBlockState};
use crate::{MarkdownIt, NodeId};

/// Default maximum number of document lines searched for the closing delimiter.
pub const DEFAULT_MAX_LINES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontMatterKind {
    Yaml,
    Toml,
}

#[derive(Debug, Clone)]
pub struct FrontMatter {
    pub kind: FrontMatterKind,
    pub raw: String,
    pub start_line: usize,
    pub end_line: usize,
}

impl FrontMatter {
    pub fn parse_with<T, E>(
        &self,
        parser: impl FnOnce(FrontMatterKind, &str) -> Result<T, E>,
    ) -> Result<T, E> {
        parser(self.kind, &self.raw)
    }
}

#[derive(Debug, Clone, Copy)]
struct FrontMatterSettings {
    max_lines: usize,
}

pub struct FrontMatterScanner;

impl BlockRule for FrontMatterScanner {
    const MARKERS: &'static [char] = &['-', '+'];
    const NAMES: &'static [&'static str] = &["front_matter", "frontmatter"];

    fn run(state: &mut DocumentBlockState<'_>) -> Option<(Option<NodeId>, usize)> {
        let (kind, end_line) = scan_front_matter(state.md, state.line, state.line_max, |line| {
            (state.get_line(line), state.line_indent(line))
        })?;
        let (raw, _) = state.get_lines(1, end_line, 0, false);
        state.root_ext.insert(FrontMatter {
            kind,
            raw,
            start_line: 0,
            end_line,
        });
        // root extensions only, no rendered node
        Some((None, end_line + 1))
    }
}

fn scan_front_matter<'a>(
    md: &MarkdownIt,
    line: usize,
    line_max: usize,
    get_line: impl Fn(usize) -> (&'a str, i32),
) -> Option<(FrontMatterKind, usize)> {
    if line != 0 || get_line(0).1 != 0 {
        return None;
    }
    let (kind, closer) = match get_line(0).0.trim_end() {
        "---" => (FrontMatterKind::Yaml, "---"),
        "+++" => (FrontMatterKind::Toml, "+++"),
        _ => return None,
    };
    let max_lines = md
        .ext
        .get::<FrontMatterSettings>()
        .map(|settings| settings.max_lines)
        .unwrap_or(DEFAULT_MAX_LINES);
    let end_line = (1..line_max.min(max_lines)).find(|&line| {
        let (text, indent) = get_line(line);
        indent == 0 && text.trim_end() == closer
    })?;
    Some((kind, end_line))
}

pub fn add(md: &mut MarkdownIt) {
    add_with_max_lines(md, DEFAULT_MAX_LINES);
}

pub fn add_with_max_lines(md: &mut MarkdownIt, max_lines: usize) {
    md.ext.insert(FrontMatterSettings { max_lines });
    md.block.add_rule::<FrontMatterScanner>().before_all();
}

pub fn set_max_lines(md: &mut MarkdownIt, max_lines: usize) {
    md.ext.insert(FrontMatterSettings { max_lines });
}

#[cfg(test)]
mod tests {
    use crate as markdown_it;

    #[test]
    fn front_matter_extracts_yaml_and_does_not_render() {
        use markdown_it::Root;
        use markdown_it::plugins::extra::front_matter::{FrontMatter, FrontMatterKind};

        let md = &mut markdown_it::MarkdownIt::empty();
        markdown_it::plugins::extra::front_matter::add(md);
        markdown_it::plugins::cmark::add(md);

        let ast = md.parse_document("---\ntitle: Hello\ntags:\n  - rust\n---\n# Post\n");
        let root = ast.node(ast.root()).cast::<Root>().unwrap();
        let front_matter = root.ext.get::<FrontMatter>().unwrap();

        assert_eq!(front_matter.kind, FrontMatterKind::Yaml);
        assert_eq!(front_matter.raw, "title: Hello\ntags:\n  - rust");
        assert_eq!(front_matter.start_line, 0);
        assert_eq!(front_matter.end_line, 4);
        assert_eq!(md.render_document(&ast), "<h1>Post</h1>\n");
    }

    #[test]
    fn front_matter_extracts_toml() {
        use markdown_it::Root;
        use markdown_it::plugins::extra::front_matter::{FrontMatter, FrontMatterKind};

        let md = &mut markdown_it::MarkdownIt::empty();
        markdown_it::plugins::extra::front_matter::add(md);
        markdown_it::plugins::cmark::add(md);

        let ast = md.parse_document("+++\ntitle = \"Hello\"\n+++\nBody");
        let root = ast.node(ast.root()).cast::<Root>().unwrap();
        let front_matter = root.ext.get::<FrontMatter>().unwrap();

        assert_eq!(front_matter.kind, FrontMatterKind::Toml);
        assert_eq!(front_matter.raw, "title = \"Hello\"");
        assert_eq!(md.render_document(&ast), "<p>Body</p>\n");
    }

    #[test]
    fn front_matter_can_be_parsed_by_user_callback() {
        use markdown_it::Root;
        use markdown_it::plugins::extra::front_matter::{FrontMatter, FrontMatterKind};

        #[derive(Debug, PartialEq, Eq)]
        struct Metadata {
            title: String,
        }

        let md = &mut markdown_it::MarkdownIt::empty();
        markdown_it::plugins::extra::front_matter::add(md);
        markdown_it::plugins::cmark::add(md);

        let ast = md.parse_document("---\ntitle: Hello\n---\nBody");
        let root = ast.node(ast.root()).cast::<Root>().unwrap();
        let front_matter = root.ext.get::<FrontMatter>().unwrap();

        let metadata = front_matter
            .parse_with(|kind, raw| match kind {
                FrontMatterKind::Yaml => raw
                    .strip_prefix("title: ")
                    .map(|title| Metadata {
                        title: title.to_owned(),
                    })
                    .ok_or("missing title"),
                FrontMatterKind::Toml => Err("unsupported front matter format"),
            })
            .unwrap();

        assert_eq!(
            metadata,
            Metadata {
                title: "Hello".to_owned(),
            }
        );
    }

    #[test]
    fn front_matter_respects_max_line_limit() {
        use markdown_it::Root;
        use markdown_it::plugins::extra::front_matter::FrontMatter;

        let md = &mut markdown_it::MarkdownIt::empty();
        markdown_it::plugins::extra::front_matter::add_with_max_lines(md, 3);
        markdown_it::plugins::cmark::add(md);

        let ast = md.parse_document("---\ntitle: Hello\nstill: metadata\n---\nBody");
        let root = ast.node(ast.root()).cast::<Root>().unwrap();

        assert!(root.ext.get::<FrontMatter>().is_none());
        assert_eq!(
            md.render_document(&ast),
            "<hr>\n<h2>title: Hello\nstill: metadata</h2>\n<p>Body</p>\n"
        );
    }
}
