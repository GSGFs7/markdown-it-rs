# Kotlin binding for markdown-it-rs

This directory contains the first Kotlin/JVM binding layer for `markdown-it-rs`.
The Rust crate exposes a deliberately small UniFFI API, and Gradle generates the
low-level JNA binding before compiling the public Kotlin wrapper.

## Current API

- reusable `MarkdownParser`
- typed `MarkdownOptions`
- HTML rendering
- front matter metadata
- optional built-in plugins
- optional syntax theme discovery and CSS

The Rust AST and foreign-language plugin callbacks are intentionally not part
of this first version.

Directive attributes are HTML-escaped but are not sanitized. Enable directives
only for trusted Markdown, or sanitize the final HTML with an appropriate
policy before displaying it.

## Development

JDK 17 or newer and the Rust toolchain are required. Run both the Rust and
Kotlin/JVM tests:

```bash
./gradlew check
```

Run only the Rust facade tests:

```bash
cargo test -p markdown-it-rs-kt
```

Enable additional native features for a Gradle build:

```bash
./gradlew test -PrustFeatures=syntect,katex
```

Generated Kotlin is written to `build/generated/uniffi` and must not be edited
or committed. UniFFI and its bundled `uniffi-bindgen` executable are pinned to
the same version in `Cargo.toml`.

## Packaging status

The development build compiles and loads the native library for the current
host through `jna.library.path`. Publishing still requires native libraries for
each supported desktop target. Android AAR packaging and `cargo-ndk` target
builds are the next packaging milestone; they are not implied by the JVM JAR.
