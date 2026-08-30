// run it like this:
// cargo test --test pathological --jobs 1 -- --nocapture --test-threads=1
use std::hint::black_box;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use markdown_it::MarkdownIt;

static MD: LazyLock<MarkdownIt> = LazyLock::new(|| {
    let mut parser = markdown_it::MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut parser);
    markdown_it::plugins::html::add(&mut parser);
    markdown_it::plugins::extra::add(&mut parser);
    parser
});

const CASE_BUDGET: Duration = Duration::from_secs(10);

#[track_caller]
fn assert_within_budget(start: Instant, phase: &str, input_len: usize) {
    let elapsed = start.elapsed();
    eprintln!("{phase} completed for {input_len} bytes in {elapsed:?}");
    assert!(
        elapsed <= CASE_BUDGET,
        "pathological {phase} exceeded {CASE_BUDGET:?}: {input_len} bytes took {elapsed:?}"
    );
}

#[track_caller]
fn run(src: &str) {
    let start = Instant::now();
    let ast = MD.parse(src);
    black_box(&ast);
    assert_within_budget(start, "parse", src.len());
}

#[track_caller]
fn run_render(src: &str) {
    let output_limit = src.len().saturating_mul(64).saturating_add(1024);
    run_render_with_output_limit(src, output_limit);
}

#[track_caller]
fn run_render_with_output_limit(src: &str, output_limit: usize) {
    let start = Instant::now();
    let output = MD.parse(src).render();
    black_box(&output);
    assert_within_budget(start, "render", src.len());

    assert!(
        output.len() <= output_limit,
        "pathological render amplified {} input bytes into {} output bytes (limit: {})",
        src.len(),
        output.len(),
        output_limit,
    );
}

mod commonmark {
    // Ported from cmark, https://github.com/commonmark/cmark/blob/master/test/pathological_tests.py
    use super::{MD, run, run_render};

    #[test]
    fn nested_inlines() {
        run(&format!(
            "{}{}{}",
            "*".repeat(100000),
            "a",
            "*".repeat(100000)
        ));
    }

    #[test]
    fn render_nested_inlines() {
        run_render(&format!(
            "{}{}{}",
            "*".repeat(100000),
            "a",
            "*".repeat(100000)
        ));
    }

    #[test]
    fn nested_strong_emph() {
        run(&format!(
            "{}{}{}",
            "*a **a ".repeat(5_000),
            "b",
            " a** a*".repeat(5_000)
        ));
    }

    #[test]
    fn many_emph_closers_with_no_openers() {
        run(&"a_ ".repeat(100000));
    }

    #[test]
    fn many_emph_openers_with_no_closers() {
        run(&"_a ".repeat(100000));
    }

    #[test]
    fn many_link_closers_with_no_openers() {
        run(&"a]".repeat(100000));
    }

    #[test]
    fn many_link_openers_with_no_closers() {
        run(&"[a".repeat(50000));
    }

    #[test]
    fn mismatched_openers_and_closers() {
        // most probably a bug
        run(&"*a_ ".repeat(50000));
    }

    #[test]
    fn commonmark_cmark_389() {
        run(&format!(
            "{}{}",
            "*a ".repeat(20_000),
            "_a*_ ".repeat(20_000)
        ));
    }

    #[test]
    fn hard_link_emph_case() {
        assert_eq!(
            MD.render("**x [a*b**c*](d)"),
            "<p>**x <a href=\"d\">a<em>b**c</em></a></p>\n"
        );
    }

    #[test]
    fn openers_and_closers_multiple_of_3() {
        run(&format!("{}{}", "a**b", "c* ".repeat(50000)));
    }

    #[test]
    fn link_openers_and_emph_closers() {
        run(&"[ a_".repeat(50000));
    }

    #[test]
    fn link_pattern_repeated() {
        run(&"[ (](".repeat(100000));
    }

    #[test]
    fn image_link_pattern_repeated() {
        run(&"![[]()".repeat(160000));
    }

    #[test]
    fn nested_brackets() {
        run(&format!(
            "{}{}{}",
            "[".repeat(50000),
            "a",
            "]".repeat(50000)
        ));
    }

    #[test]
    fn nested_block_quotes() {
        run(&format!("{}{}", "> ".repeat(50000), "a"));
    }

    #[test]
    fn render_nested_block_quotes() {
        run_render(&format!("{}{}", "> ".repeat(50000), "a"));
    }

    #[test]
    fn deeply_nested_lists() {
        let src = (0..5000)
            .map(|x| format!("{}{}", "  ".repeat(x), "* a\n"))
            .collect::<Vec<_>>()
            .join("");
        run(&src);
    }

    #[test]
    fn backticks() {
        let src = (0..1000)
            .map(|x| format!("{}{}", "e", "`".repeat(x)))
            .collect::<Vec<_>>()
            .join("");
        run(&src);
    }

    #[test]
    fn unclosed_links_a() {
        run(&"[a](<b".repeat(30000));
    }

    #[test]
    fn unclosed_links_b() {
        run(&"[a](b".repeat(30000));
    }

    #[test]
    fn unclosed_html_comments() {
        run(&format!("</{}", "<!--".repeat(300000)));
    }

    #[test]
    fn empty_lines_in_deeply_nested_lists() {
        let src = format!("{}x{}", "- ".repeat(30_000), "\n".repeat(30_000),);
        run_render(&src);
    }

    #[test]
    fn empty_lines_in_deeply_nested_lists_in_blockquote() {
        let src = format!("> {}x\n{}", "- ".repeat(30_000), ">\n".repeat(30_000),);
        run_render(&src);
    }

    #[test]
    fn emphasis_in_deep_blockquote() {
        let src = format!("{}{}", ">".repeat(100_000), "a*".repeat(100_000),);
        run_render(&src);
    }

    #[test]
    fn many_references() {
        use std::fmt::Write;

        let count = 25_000;
        let mut src = String::with_capacity(count * 48);

        for i in 0..count {
            writeln!(&mut src, "[ref{i}]: /url/{i}").unwrap();
        }

        src.push('\n');

        for i in 0..count {
            write!(&mut src, "[ref{i}] ").unwrap();
        }

        run_render(&src);
    }

    #[test]
    fn multiline_reference_title() {
        let src = format!("[foo]: /url '\n{}'\n\n[foo]", "line\n".repeat(40_000),);

        run_render(&src);
    }

    #[test]
    fn nul_bytes_in_input() {
        run_render(&"abc\0de\0".repeat(100_000));
    }
}

mod markdownit {
    // Ported from markdown-it.js
    use super::{run, run_render, run_render_with_output_limit};

    #[test]
    fn table_autocompleted_cells() {
        let size = 1000;
        let src = format!(
            "{}\n{}\n{}",
            "x|".repeat(size),
            "-|".repeat(size),
            "x|\n".repeat(size),
        );

        // Without a cap on synthesized empty cells this produces roughly
        // 10 MB of HTML and grows quadratically with `size`.
        run_render_with_output_limit(&src, 1024 * 1024);
    }

    #[test]
    fn emphasis_pattern() {
        run(&"**_* ".repeat(50_000));
    }

    #[test]
    fn backtick_pattern() {
        run(&"``\\".repeat(50000));
    }

    #[test]
    fn autolinks_pattern() {
        run(&format!("{}{}", "<".repeat(100000), ">"));
    }

    #[test]
    fn hardbreak_whitespaces_pattern() {
        run(&format!("{}{}{}", "x", " ".repeat(100000), "x  \nx"));
    }

    #[test]
    fn linkify_emails_separated_by_softbreaks() {
        run(&"ping a@b.co ok\n".repeat(30000));
    }

    #[test]
    fn linkify_unregistered_schemes() {
        run(&"a://".repeat(70000));
    }

    #[test]
    fn many_smartquotes_in_single_block() {
        run(&"\"".repeat(70000));
    }

    #[cfg(feature = "linkify")]
    #[test]
    fn linkify_trailing_asterisks() {
        let src = format!("https://test.com?{}a", "*".repeat(70_000),);

        run_render(&src);
    }
}
