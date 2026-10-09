//! Persistent JSONL adapter for tests/differential/run.mjs.
use std::io::{self, BufRead, Write};

use markdown_it::{plugins, MarkdownIt};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Options {
    html: Option<bool>,
    linkify: Option<bool>,
    typographer: Option<bool>,
    breaks: Option<bool>,
    xhtml_out: Option<bool>,
    lang_prefix: Option<String>,
    max_nesting: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    source: String,
    preset: String,
    options: Options,
}

fn render(request: Request) -> Result<String, String> {
    let commonmark = match request.preset.as_str() {
        "default" => false,
        "commonmark" => true,
        other => return Err(format!("unsupported preset: {other}")),
    };
    // Build the syntax explicitly so CommonMark's HTML can also be disabled.
    let mut md = MarkdownIt::empty();
    plugins::cmark::add(&mut md);
    if !commonmark {
        plugins::extra::tables::add(&mut md);
        plugins::extra::strikethrough::add(&mut md);
    }
    let options = request.options;
    if options.html.unwrap_or(commonmark) {
        plugins::html::add(&mut md);
    }
    if options.linkify.unwrap_or(false) {
        #[cfg(feature = "linkify")]
        plugins::extra::linkify::add(&mut md);
        #[cfg(not(feature = "linkify"))]
        return Err("linkify requires the default Cargo feature".into());
    }
    if options.typographer.unwrap_or(false) {
        plugins::extra::typographer::add(&mut md);
        plugins::extra::smartquotes::add(&mut md);
    }
    md.max_nesting = options
        .max_nesting
        .unwrap_or(if commonmark { 20 } else { 100 });
    md.render_options.xhtml_out = options.xhtml_out.unwrap_or(commonmark);
    md.render_options.breaks = options.breaks.unwrap_or(false);
    md.render_options.lang_prefix = options.lang_prefix;
    Ok(md.render(&request.source))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let line = line?;
        let response = match serde_json::from_str::<Request>(&line) {
            Ok(request) => match render(request) {
                Ok(html) => serde_json::json!({ "html": html }),
                Err(error) => serde_json::json!({ "error": error }),
            },
            Err(error) => serde_json::json!({ "error": error.to_string() }),
        };
        serde_json::to_writer(&mut stdout, &response)?;
        writeln!(stdout)?;
        stdout.flush()?;
    }
    Ok(())
}
