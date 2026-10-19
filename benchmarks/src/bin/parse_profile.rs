use std::hint::black_box;

use markdown_it_benchmarks::{corpus, parser};

fn usage() -> ! {
    eprintln!(
        "usage: parse_profile \
         <configuration> <corpus> \
         <markdown-it-rs> \
         [iterations]"
    );
    std::process::exit(2);
}

fn main() {
    let mut args = std::env::args().skip(1);
    let configuration_name = args.next().unwrap_or_else(|| usage());
    let corpus_name = args.next().unwrap_or_else(|| usage());
    let path = args.next().unwrap_or_else(|| usage());
    let iterations = args
        .next()
        .map(|value| value.parse::<usize>().unwrap_or_else(|_| usage()))
        .unwrap_or(1_000);

    if args.next().is_some() || iterations == 0 {
        usage();
    }

    let configuration = parser::parse_configurations()
        .into_iter()
        .find(|configuration| configuration.name == configuration_name)
        .unwrap_or_else(|| usage());
    let corpus = corpus::parser_emphasis_checkpoint()
        .into_iter()
        .find(|corpus| corpus.name == corpus_name)
        .unwrap_or_else(|| usage());

    let md = configuration.parser;
    let source = corpus.source();

    // Keep correctness checks and lazy parser initialization outside the
    // repeated profile region.
    md.render(source);

    match path.as_str() {
        "markdown-it-rs" => {
            for _ in 0..iterations {
                black_box(md.parse(black_box(source)));
            }
        }
        _ => usage(),
    }

    eprintln!(
        "profiled {path} with {configuration_name} on {corpus_name} \
         for {iterations} iterations"
    );
}
