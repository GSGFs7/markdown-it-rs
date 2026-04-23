use crate::parser::block::{BlockRule, BlockState};
use crate::parser::extset::{MarkdownItExt, RootExt};
use crate::{MarkdownIt, Node};

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

impl RootExt for FrontMatter {}

#[derive(Debug, Clone, Copy)]
struct FrontMatterSettings {
    max_lines: usize,
}

impl MarkdownItExt for FrontMatterSettings {}

pub struct FrontMatterScanner;

impl BlockRule for FrontMatterScanner {
    fn run(state: &mut BlockState) -> Option<(Node, usize)> {
        if state.line != 0 {
            return None;
        }
        if state.line_indent(state.line) != 0 {
            return None;
        }

        let opener = state.get_line(0).trim_end();
        let (kind, closer) = match opener {
            "---" => (FrontMatterKind::Yaml, "---"),
            "+++" => (FrontMatterKind::Toml, "+++"),
            _ => return None,
        };

        let max_lines = state
            .md
            .ext
            .get::<FrontMatterSettings>()
            .map(|settings| settings.max_lines)
            .unwrap_or(DEFAULT_MAX_LINES);

        let line_limit = state.line_max.min(max_lines);
        let mut end_line = 1;
        while end_line < line_limit {
            if state.line_indent(end_line) == 0 && state.get_line(end_line).trim_end() == closer {
                let (raw, _) = state.get_lines(1, end_line, 0, false);
                state.root_ext.insert(FrontMatter {
                    kind,
                    raw,
                    start_line: 0,
                    end_line,
                });
                return Some((Node::default(), end_line + 1));
            }

            end_line += 1;
        }

        None
    }
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
