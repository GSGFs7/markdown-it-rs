use std::io::{Read, Write};

use clap::{Arg, ArgAction, Command};
use markdown_it::document::{Text, TextSpecial};

#[cfg(not(tarpaulin_include))]
fn main() {
    let cli = Command::new("markdown-it")
        .version(env!("CARGO_PKG_VERSION"))
        .disable_version_flag(true)
        .arg(
            Arg::new("version")
                .short('v')
                .long("version")
                .help("Show version")
                .action(ArgAction::Version),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .help("File to write")
                .default_value("-"),
        )
        .arg(
            Arg::new("sourcepos")
                .long("sourcepos")
                .help("Include source mappings in HTML attributes")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no-html")
                .long("no-html")
                .help("Disable embedded HTML")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("typographer")
                .short('t')
                .long("typographer")
                .help("Enable smartquotes and other typographic replacements")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("tree")
                .long("tree")
                .help("Print syntax tree for debugging")
                .action(ArgAction::SetTrue),
        )
        .arg(Arg::new("file").help("File to read").default_value("-"));

    #[cfg(feature = "linkify")]
    let cli = cli.arg(
        Arg::new("linkify")
            .short('l')
            .long("linkify")
            .help("Autolink text")
            .action(ArgAction::SetTrue),
    );

    let matches = cli.get_matches();
    let input = matches
        .get_one::<String>("file")
        .expect("file has a default");
    let output = matches
        .get_one::<String>("output")
        .expect("output has a default");
    let no_html = matches.get_flag("no-html");
    #[cfg(feature = "linkify")]
    let linkify = matches.get_flag("linkify");
    let typographer = matches.get_flag("typographer");
    let sourcepos = matches.get_flag("sourcepos");
    let show_tree = matches.get_flag("tree");

    let vec = if input == "-" {
        let mut vec = Vec::new();
        std::io::stdin().read_to_end(&mut vec).unwrap();
        vec
    } else {
        std::fs::read(input).unwrap()
    };

    let source = String::from_utf8_lossy(&vec);
    let md = &mut markdown_it::MarkdownIt::empty();
    markdown_it::plugins::cmark::add(md);
    #[cfg(feature = "syntect")]
    markdown_it::plugins::extra::syntect::add(md);
    markdown_it::plugins::extra::tables::add(md);
    markdown_it::plugins::extra::strikethrough::add(md);
    markdown_it::plugins::extra::beautify_links::add(md);
    if !no_html {
        markdown_it::plugins::html::add(md);
    }
    if sourcepos {
        markdown_it::plugins::sourcepos::add(md);
    }
    #[cfg(feature = "linkify")]
    if linkify {
        markdown_it::plugins::extra::linkify::add(md);
    }
    if typographer {
        markdown_it::plugins::extra::smartquotes::add(md);
        markdown_it::plugins::extra::typographer::add(md);
    }

    let ast = md.parse_document(&source);

    if show_tree {
        let mut depth = 0usize;
        for event in ast.events(ast.root()) {
            if matches!(event, markdown_it::StructuralEvent::Exit(_)) {
                depth -= 1;
                continue;
            }
            let node = event.node();
            let name = node.name().rsplit("::").next().unwrap();
            print!("{}", "    ".repeat(depth));
            if let Some(text) = node.cast::<Text>() {
                println!("{name}: {:?}", text.content);
            } else if let Some(text) = node.cast::<TextSpecial>() {
                println!("{name}: {:?}", text.content);
            } else {
                println!("{name}");
            }
            if matches!(event, markdown_it::StructuralEvent::Enter(_)) {
                depth += 1;
            }
        }
        return;
    }

    let result = md.render_document(&ast);

    if output == "-" {
        std::io::stdout().write_all(result.as_bytes()).unwrap();
    } else {
        std::fs::write(output, &result).unwrap();
    }
}
