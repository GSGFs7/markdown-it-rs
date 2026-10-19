#![cfg(feature = "sourcemap")]

use markdown_it::common::sourcemap::SourceWithLineStarts;
use markdown_it::{Document, NodeRef};

#[test]
fn text_fallback_maps_only_existing_source_bytes() {
    use markdown_it::{EditBatch, MarkdownIt, StructuralEvent};

    let md = MarkdownIt::empty();
    type Case = (&'static str, &'static [(usize, usize)], &'static str);
    let cases: &[Case] = &[
        ("", &[], ""),
        ("   ", &[], ""),
        ("hello", &[(0, 5)], "hello\n"),
        ("hello\n", &[(0, 6)], "hello\n"),
        ("雪", &[(0, 3)], "雪\n"),
        ("  雪", &[(2, 5)], "雪\n"),
        ("first\n雪", &[(0, 6), (6, 9)], "first\n雪\n"),
        ("a\r\n雪", &[(0, 2), (3, 6)], "a\n雪\n"),
        ("a\r雪", &[(0, 2), (2, 5)], "a\n雪\n"),
        ("\0", &[(0, 3)], "�\n"),
        ("text", &[(0, 4)], "text\n"),
        (
            "  leading\n\nsecond  ",
            &[(2, 10), (11, 19)],
            "leading\nsecond  \n",
        ),
        (
            "first\r\n\tsecond\rthird",
            &[(0, 6), (8, 15), (15, 20)],
            "first\nsecond\nthird\n",
        ),
        ("nul \0 byte", &[(0, 12)], "nul � byte\n"),
        ("# 雪\n\n*inline*", &[(0, 6), (7, 15)], "# 雪\n*inline*\n"),
    ];

    for &(source, expected_maps, expected_html) in cases {
        let mut document = md.parse(source);
        let maps: Vec<_> = document
            .children(document.root())
            .iter()
            .map(|&id| document.node(id).srcmap().unwrap().get_byte_offsets())
            .collect();
        assert_eq!(maps, expected_maps, "source={source:?}");

        let mut edits = EditBatch::new();
        for event in document.events(document.root()) {
            if matches!(event, StructuralEvent::Exit(_)) {
                continue;
            }
            let node = event.node();
            let map = node.srcmap().unwrap();
            let (start, end) = map.get_byte_offsets();
            assert!(
                document.source().get(start..end).is_some(),
                "source={source:?}"
            );
            edits.set_source_map(node.id(), Some(map));
        }
        edits.commit(&mut document);
        assert_eq!(
            md.render_html(&document),
            expected_html,
            "source={source:?}"
        );
    }
}

fn run(input: &str, f: fn(&Document, SourceWithLineStarts)) {
    let mut md = markdown_it::MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::html::add(&mut md);
    let document = md.parse(input);
    for event in document.events(document.root()) {
        assert!(event.node().srcmap().is_some());
    }
    f(&document, SourceWithLineStarts::new(input));
}
fn getmap(node: NodeRef<'_>, map: &SourceWithLineStarts) -> ((u32, u32), (u32, u32)) {
    node.srcmap().unwrap().get_positions(map)
}

#[test]
fn paragraph() {
    // same as commonmark.js
    run(
        "foo   \n     \n     \n\n  barbaz\n\tquux   \n",
        |node, map| {
            assert_eq!(
                getmap(node.node(node.children(node.root())[0]), &map),
                ((1, 1), (1, 6)),
            );
            assert_eq!(
                getmap(node.node(node.children(node.root())[1]), &map),
                ((5, 3), (6, 8)),
            );
        },
    );
}

#[test]
fn hr() {
    // same as commonmark.js
    run(" ---  \n\n  * * *\n", |node, map| {
        assert_eq!(
            getmap(node.node(node.children(node.root())[0]), &map),
            ((1, 2), (1, 6)),
        );
        assert_eq!(
            getmap(node.node(node.children(node.root())[1]), &map),
            ((3, 3), (3, 7)),
        );
    });
}

#[test]
fn heading() {
    // same as commonmark.js
    run("  \n  ### foo ###  \n\n", |node, map| {
        assert_eq!(
            getmap(node.node(node.children(node.root())[0]), &map),
            ((2, 3), (2, 15)),
        );
    });

    run("  #\n", |node, map| {
        assert_eq!(
            getmap(node.node(node.children(node.root())[0]), &map),
            ((1, 3), (1, 3)),
        );
    });
}

#[test]
fn lheading() {
    // same as commonmark.js
    run("  foo\n bar\n ----\n\n", |node, map| {
        assert_eq!(
            getmap(node.node(node.children(node.root())[0]), &map),
            ((1, 3), (3, 5)),
        );
    });
}

#[test]
fn fence() {
    // same as commonmark.js
    run("  ~~~ foo ~~~\n", |node, map| {
        assert_eq!(
            getmap(node.node(node.children(node.root())[0]), &map),
            ((1, 3), (1, 13)),
        );
    });

    run("  ```\n 12\n", |node, map| {
        assert_eq!(
            getmap(node.node(node.children(node.root())[0]), &map),
            ((1, 3), (2, 3)),
        );
    });

    run("```\n\n\n\n", |node, map| {
        assert_eq!(
            getmap(node.node(node.children(node.root())[0]), &map),
            ((1, 1), (4, 0)),
        );
    });

    run("~~~\na\nb\n~~~  \nc\n", |node, map| {
        assert_eq!(
            getmap(node.node(node.children(node.root())[0]), &map),
            ((1, 1), (4, 5)),
        );
    });
}

#[test]
fn html_block() {
    // same as commonmark.js
    run("  <div>\n", |node, map| {
        assert_eq!(
            getmap(node.node(node.children(node.root())[0]), &map),
            ((1, 3), (1, 7)),
        );
    });

    run("<div>\n</div>  \n", |node, map| {
        assert_eq!(
            getmap(node.node(node.children(node.root())[0]), &map),
            ((1, 1), (2, 8)),
        );
    });
}

#[test]
fn code_block() {
    // this should be (1, 5), (1, 9)
    // for simplicity, we point source maps for block tags to first
    // nonspace character, but it isn't quite correct for code blocks
    run("      foo\n", |node, map| {
        assert_eq!(
            getmap(node.node(node.children(node.root())[0]), &map),
            ((1, 7), (1, 9)),
        );
    });

    run("   a\n    b\n     c\n", |node, map| {
        assert_eq!(
            getmap(node.node(node.children(node.root())[0]), &map),
            ((1, 4), (3, 6)),
        );
    });

    // this I believe to be error in commonmark, code block
    // only have 1 line as per spec, but cmark reports 3 lines
    run("    foobar  \n    \n    \n\nbar\n", |node, map| {
        assert_eq!(
            getmap(node.node(node.children(node.root())[0]), &map),
            ((1, 5), (1, 12)),
        );
    });
}

#[test]
fn blockquotes() {
    // same as commonmark.js
    run("  > foo  \n", |node, map| {
        assert_eq!(
            getmap(node.node(node.children(node.root())[0]), &map),
            ((1, 3), (1, 9)),
        );
    });

    run("> foo\nbar\n\n", |node, map| {
        assert_eq!(
            getmap(node.node(node.children(node.root())[0]), &map),
            ((1, 1), (2, 3)),
        );
    });
}

#[test]
fn lists() {
    // same as commonmark.js
    run(" 1. foo\n 2. bar\n", |node, map| {
        assert_eq!(
            getmap(node.node(node.children(node.root())[0]), &map),
            ((1, 2), (2, 7)),
        );

        assert_eq!(
            getmap(
                node.node(node.children(node.children(node.root())[0])[0]),
                &map
            ),
            ((1, 2), (1, 7)),
        );
    });

    run(" - foo\n\n - bar\n", |node, map| {
        assert_eq!(
            getmap(node.node(node.children(node.root())[0]), &map),
            ((1, 2), (3, 6)),
        );

        assert_eq!(
            getmap(
                node.node(node.children(node.children(node.root())[0])[0]),
                &map
            ),
            ((1, 2), (2, 0)),
        );

        assert_eq!(
            getmap(
                node.node(node.children(node.children(node.root())[0])[1]),
                &map
            ),
            ((3, 2), (3, 6)),
        );
    });
}

#[test]
fn autolinks() {
    run("foo <http://google.com> bar", |node, map| {
        assert_eq!(
            getmap(
                node.node(node.children(node.children(node.root())[0])[1]),
                &map
            ),
            ((1, 5), (1, 23)),
        );

        assert_eq!(
            getmap(
                node.node(node.children(node.children(node.children(node.root())[0])[1])[0]),
                &map
            ),
            ((1, 6), (1, 22)),
        );
    });
}

#[test]
fn emphasis() {
    run("***foo***", |node, map| {
        assert_eq!(
            getmap(
                node.node(node.children(node.children(node.root())[0])[0]),
                &map
            ),
            ((1, 1), (1, 9)),
        );

        assert_eq!(
            getmap(
                node.node(node.children(node.children(node.children(node.root())[0])[0])[0]),
                &map
            ),
            ((1, 2), (1, 8)),
        );
    });

    run("aaa **bb _cc_ dd** eee", |node, map| {
        assert_eq!(
            getmap(
                node.node(node.children(node.children(node.root())[0])[1]),
                &map
            ),
            ((1, 5), (1, 18)),
        );

        assert_eq!(
            getmap(
                node.node(node.children(node.children(node.children(node.root())[0])[1])[1]),
                &map
            ),
            ((1, 10), (1, 13)),
        );
    });
}

#[test]
fn newline() {
    run("foo  \nbar \nbaz\nquux", |node, map| {
        assert_eq!(
            getmap(
                node.node(node.children(node.children(node.root())[0])[1]),
                &map
            ),
            ((1, 4), (2, 0)),
        );

        assert_eq!(
            getmap(
                node.node(node.children(node.children(node.root())[0])[3]),
                &map
            ),
            ((2, 4), (3, 0)),
        );

        assert_eq!(
            getmap(
                node.node(node.children(node.children(node.root())[0])[5]),
                &map
            ),
            ((4, 0), (4, 0)),
        );

        /*let marks : Vec<_> = node.children[0].children.iter().map(|x| getmap(x, &map)).collect();
        assert_eq!(marks, [
            ((1, 1), (1, 5)),
            ((2, 0), (2, 1)),
            ((2, 1), (2, 3)),
            ((3, 0), (3, 0)),
            ((3, 1), (3, 3)),
        ]);*/
    });
}

#[test]
fn escapes() {
    run("foo\\Δ\\*bar", |node, map| {
        assert_eq!(
            getmap(
                node.node(node.children(node.children(node.root())[0])[1]),
                &map
            ),
            ((1, 4), (1, 5)),
        );

        assert_eq!(
            getmap(
                node.node(node.children(node.children(node.root())[0])[2]),
                &map
            ),
            ((1, 6), (1, 7)),
        );
    });

    run("  foo  \\\n  bar  ", |node, map| {
        assert_eq!(
            getmap(
                node.node(node.children(node.children(node.root())[0])[1]),
                &map
            ),
            ((1, 8), (2, 2)),
        );
    });
}

#[test]
fn entities() {
    run("aa &nbsp; bb &#20; cc", |node, map| {
        assert_eq!(
            getmap(
                node.node(node.children(node.children(node.root())[0])[1]),
                &map
            ),
            ((1, 4), (1, 9)),
        );

        assert_eq!(
            getmap(
                node.node(node.children(node.children(node.root())[0])[3]),
                &map
            ),
            ((1, 14), (1, 18)),
        );

        /*let marks : Vec<_> = node.children[0].children.iter().map(|x| getmap(x, &map)).collect();
        assert_eq!(marks, [
            ((1, 1), (1, 5)),
            ((2, 0), (2, 1)),
            ((2, 1), (2, 3)),
            ((3, 0), (3, 0)),
            ((3, 1), (3, 3)),
        ]);*/
    });
}

#[test]
fn html_inline() {
    run("foo <bar> baz", |node, map| {
        assert_eq!(
            getmap(
                node.node(node.children(node.children(node.root())[0])[1]),
                &map
            ),
            ((1, 5), (1, 9)),
        );
    });
}

#[test]
fn backticks() {
    run("foo ```bar``` baz", |node, map| {
        assert_eq!(
            getmap(
                node.node(node.children(node.children(node.root())[0])[1]),
                &map
            ),
            ((1, 5), (1, 13)),
        );

        assert_eq!(
            getmap(
                node.node(node.children(node.children(node.children(node.root())[0])[1])[0]),
                &map
            ),
            ((1, 8), (1, 10)),
        );
    });

    run("foo ` bar ` baz", |node, map| {
        assert_eq!(
            getmap(
                node.node(node.children(node.children(node.root())[0])[1]),
                &map
            ),
            ((1, 5), (1, 11)),
        );

        assert_eq!(
            getmap(
                node.node(node.children(node.children(node.children(node.root())[0])[1])[0]),
                &map
            ),
            ((1, 7), (1, 9)),
        );
    });
}

#[test]
fn imglink() {
    run("foo [bar](baz) quux", |node, map| {
        assert_eq!(
            getmap(
                node.node(node.children(node.children(node.root())[0])[1]),
                &map
            ),
            ((1, 5), (1, 14)),
        );
    });

    run("foo ![bar](baz) quux", |node, map| {
        assert_eq!(
            getmap(
                node.node(node.children(node.children(node.root())[0])[1]),
                &map
            ),
            ((1, 5), (1, 15)),
        );
    });
}
