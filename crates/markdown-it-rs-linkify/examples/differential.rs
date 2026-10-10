//! JSONL detection adapter; deliberately bypasses Markdown parsing and URL formatting.
use std::io::{self, BufRead, Write};

use markdown_it_rs_linkify::{LinkKind, Linkify};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    source: String,
    #[serde(default)]
    fuzzy_links: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let response = match serde_json::from_str::<Request>(&line?) {
            Ok(request) => {
                let matches: Vec<_> = Linkify::new()
                    .links_with_fuzzy(&request.source, request.fuzzy_links)
                    .into_iter()
                    .map(|link| {
                        serde_json::json!({
                            "start": link.start(),
                            "end": link.end(),
                            "kind": match link.kind() {
                                LinkKind::Url => "url",
                                LinkKind::Email => "email",
                            },
                            "raw": link.as_str(&request.source),
                        })
                    })
                    .collect();
                serde_json::json!({ "matches": matches })
            }
            Err(error) => serde_json::json!({ "error": error.to_string() }),
        };
        serde_json::to_writer(&mut stdout, &response)?;
        writeln!(stdout)?;
        stdout.flush()?;
    }
    Ok(())
}
