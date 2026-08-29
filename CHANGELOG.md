# Changelog

## Unreleased

### Breaking changes

- required custom `NodeValue` implementations to be `Send + Sync`
- removed `syntect` from the default features; enable it explicitly when needed

### Added

- added parser presets via `MarkdownIt::with_preset`
- added GFM-style alerts and CJK-friendly emphasis handling
- expanded `syntect` syntax and theme support via `two-face`

### Changed

- updated CommonMark compliance to spec 0.31.2
- extracted link detection into the `markdown-it-rs-linkify` crate

### Fixed

- aligned linkification and URL normalization with markdown-it.js 15.0.1
- fixed a panic when linkifying short email addresses with link beautification
- prevented pathological linkification of unregistered schemes from taking
  superlinear time

## 0.7.0 - 2026-08-05

### Breaking changes

- changed parser rule identifiers from `TypeKey` to `RuleMark`, which supports
  both Rust types (`RuleMark::of::<T>()`) and string names
  (`RuleMark::named("name")`). This enables Python and other dynamic plugins
  to locate and order parser rules. Integrations that depend on the concrete
  identifier type of parser rule chains must migrate to `RuleMark`; existing
  typed `RuleBuilder` ordering methods remain available

### Added

- added an opt-in directives plugin at `plugins::directives`, with text, leaf,
  and container directives plus custom renderers; directive attributes are
  HTML-escaped but are not sanitized
- added footnote, task list, and mark plugins to `plugins::extra`
- added string aliases through the `NAMES` constant on core, block, and inline
  rules, plus the `before_named`, `after_named`, `alias_named`, and
  `require_named` rule-builder methods
- expanded the Python bindings with `.use()`, mutable AST and node access,
  Python core-rule callbacks, ordered postprocessors, and bindings for
  directives, task lists, footnotes, mark, and configurable heading anchors
- added an initial Kotlin/JVM binding built with UniFFI
- added WebAssembly/npm bindings

### Changed

- improved heading anchor slugification and uniqueness, with configurable slug
  strategies, existing-ID behavior, empty-slug handling, and prefixes
- upgraded the Python bindings to PyO3 0.29
- rebuilt the browser demo with Vite and pnpm on top of the WebAssembly binding

### Fixed

- fixed block math rendering so block expressions use display mode
- fixed Python constructor options for directives, task lists, and footnotes
- fixed the Rust test suite when running with `--no-default-features`
- deferred syntax highlighting theme loading until it is needed

## 0.6.2 - 2026-04-25

### Added

- added markdown math support
- added front matter support
- added Python bindings
- added syntect class-based highlighting mode, highlighted line support, and demo
- added `markdown-it-rs-url` compatibility crate to replace `mdurl`

### Changed

- renamed the published crate to `markdown-it-rs`
- moved package manifests to Rust 2024 edition
- replaced `argparse` with `clap`
- removed the unmaintained `derivative` dependency
- updated dependencies for newer toolchains

### Fixed

- fixed syntect custom prefix handling in inline mode
- fixed syntect doctest coverage

## 0.6.1 - 2024-07-07

### Fixed

- fixed panic on malformed input found by fuzzing
  (https://github.com/markdown-it-rust/markdown-it/issues/40)

## 0.6.0 - 2023-08-03

### Added

- added link reference definition as AST node (renders as empty) for roundtripping
  (https://github.com/rlidwka/markdown-it.rs/pull/22)

### Changed

- only set max indent=4 if `code` blocks plugin is enabled
  (https://github.com/rlidwka/markdown-it.rs/pull/20)

### Fixed

- fixed ambiguity between tables and setext headings
  (https://github.com/rlidwka/markdown-it.rs/pull/27)

## 0.5.1 - 2023-07-05

### Fixed

- fixed panics in smartquotes (https://github.com/rlidwka/markdown-it.rs/issues/26)
- fixed entity code unescaping (https://github.com/rlidwka/markdown-it.rs/issues/23)
- multiple other minor bugfixes

## 0.5.0 - 2023-05-13

### Added

- typographer plugin (https://github.com/rlidwka/markdown-it.rs/pull/4)
- smartquotes plugin (https://github.com/rlidwka/markdown-it.rs/pull/5)
- headings with ids plugin (https://github.com/rlidwka/markdown-it.rs/pull/18)

### Changed

- reference map changed from a HashMap to a trait object, allowing user to override it
  (https://github.com/rlidwka/markdown-it.rs/pull/17)

## 0.0.0 - 0.4.0 (2022-05-21 - 2022-10-03)

Initial commits. Software was not stabilized yet, so changes weren't documented at that point.
