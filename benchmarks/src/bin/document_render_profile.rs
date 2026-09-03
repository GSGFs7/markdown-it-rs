use std::hint::black_box;

use markdown_it::MarkdownIt;
use markdown_it_benchmarks::corpus;

fn parser() -> MarkdownIt {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::html::add(&mut md);
    md
}

fn usage() -> ! {
    eprintln!(
        "usage: document_render_profile <corpus> <arena-direct|legacy-tree> [iterations]"
    );
    std::process::exit(2);
}

fn main() {
    let mut args = std::env::args().skip(1);
    let corpus_name = args.next().unwrap_or_else(|| usage());
    let path = args.next().unwrap_or_else(|| usage());
    let iterations = args
        .next()
        .map(|value| value.parse::<usize>().unwrap_or_else(|_| usage()))
        .unwrap_or(1_000);
    if args.next().is_some() || iterations == 0 {
        usage();
    }

    let corpus = corpus::standard()
        .into_iter()
        .find(|corpus| corpus.name == corpus_name)
        .unwrap_or_else(|| usage());
    let md = parser();
    let source = corpus.source();

    match path.as_str() {
        "arena-direct" => {
            let document = md.parse_document(source);
            for _ in 0..iterations {
                black_box(md.render_document(black_box(&document)).unwrap());
            }
        }
        "legacy-tree" => {
            let root = md.parse(source);
            for _ in 0..iterations {
                black_box(root.render());
            }
        }
        _ => usage(),
    }

    eprintln!("profiled {path} on {corpus_name} for {iterations} iterations");
}
