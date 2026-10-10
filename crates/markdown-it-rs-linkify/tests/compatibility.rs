//! Strict detection snapshots; the live JS oracle is opt-in so ordinary tests need no Node.js.
use std::collections::HashSet;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use markdown_it_rs_linkify::{LinkKind, Linkify};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Request {
    source: String,
    fuzzy_links: bool,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Match {
    start: usize,
    end: usize,
    kind: String,
    raw: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Fixture {
    id: String,
    request: Request,
    expected: Vec<Match>,
}

fn fixtures() -> Vec<Fixture> {
    serde_json::from_str(include_str!("differential/linkify-fixtures.json")).unwrap()
}

fn detect(request: &Request) -> Vec<Match> {
    Linkify::new()
        .links_with_fuzzy(&request.source, request.fuzzy_links)
        .into_iter()
        .map(|link| Match {
            start: link.start(),
            end: link.end(),
            kind: match link.kind() {
                LinkKind::Url => "url",
                LinkKind::Email => "email",
            }
            .into(),
            raw: link.as_str(&request.source).into(),
        })
        .collect()
}

#[test]
fn pinned_detection_snapshots() {
    let fixtures = fixtures();
    assert!(!fixtures.is_empty());
    let mut ids = HashSet::new();
    for fixture in &fixtures {
        assert!(ids.insert(&fixture.id), "duplicate fixture: {}", fixture.id);
        let actual = detect(&fixture.request);
        let expected = &fixture.expected;
        assert_eq!(
            &actual, expected,
            "{}: detection snapshot changed",
            fixture.id
        );
        for matches in [&fixture.expected, expected] {
            let mut previous_end = 0;
            for found in matches {
                assert!(found.start >= previous_end && found.end > found.start);
                assert_eq!(
                    fixture.request.source.get(found.start..found.end),
                    Some(found.raw.as_str()),
                    "{}: invalid UTF-8 match range",
                    fixture.id
                );
                previous_end = found.end;
            }
        }
    }
}

#[derive(Deserialize)]
struct OracleResponse {
    matches: Vec<Match>,
}

#[test]
#[ignore = "requires Node.js and the crate's pinned JS oracle dependencies"]
fn pinned_linkify_it_oracle() {
    let fixtures = fixtures();
    let oracle =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/differential/linkify-oracle.mjs");
    let mut child = Command::new("node")
        .arg(oracle)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("install Node.js before running the live oracle test");
    {
        let mut stdin = child.stdin.take().unwrap();
        for fixture in &fixtures {
            serde_json::to_writer(&mut stdin, &fixture.request).unwrap();
            writeln!(stdin).unwrap();
        }
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "JS oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let responses: Vec<OracleResponse> = stdout
        .lines()
        .map(|line| serde_json::from_str(line).expect("invalid JS oracle response"))
        .collect();
    assert_eq!(responses.len(), fixtures.len(), "wrong JS response count");
    for (fixture, response) in fixtures.iter().zip(responses) {
        assert_eq!(
            response.matches, fixture.expected,
            "{}: JS oracle changed",
            fixture.id
        );
        assert_eq!(
            detect(&fixture.request),
            response.matches,
            "{}: Rust differs from JS",
            fixture.id
        );
    }
}
