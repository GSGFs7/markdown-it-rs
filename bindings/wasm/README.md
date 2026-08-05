# WASM binding for markdown-it-rs

Rust compiled to WebAssembly, usable from JavaScript/TypeScript.
A live demo is available at <https://gsgfs7.github.io/markdown-it-rs/>.

## Install

```bash
npm install @gsgfs/markdown-it-rs-wasm
```

## Usage

```js
import { MarkdownIt } from '@gsgfs/markdown-it-rs-wasm'

const md = new MarkdownIt()
const html = md.render('# Hello, world!')
console.log(html) // <h1>Hello, world!</h1>
```

### Two build variants

The package ships two entry points:

- **`@gsgfs/markdown-it-rs-wasm`** (default / `./basic`) — CommonMark syntax.
- **`@gsgfs/markdown-it-rs-wasm/full`** — CommonMark + extras (`linkify`, `syntect` syntax highlighting, `katex` math, and other `extra` plugins).

```js
// basic build — CommonMark
import { MarkdownIt } from '@gsgfs/markdown-it-rs-wasm'

// full build — extras enabled
import { MarkdownIt as MarkdownItFull } from '@gsgfs/markdown-it-rs-wasm/full'
```

## API

### `new MarkdownIt(options?)`

Creates a parser. It always registers the CommonMark plugins, and (in the `full` build) automatically registers the `extra` plugins.

```js
const md = new MarkdownIt({ allowHtml: true })
console.log(md.render('Hello <strong>world</strong>!'))
// <p>Hello <strong>world</strong>!</p>
```

`allowHtml` defaults to `false`.

### `render(source: string) => string`

Parses Markdown and returns the rendered HTML string.

```js
const md = new MarkdownIt()
console.log(md.render('Hello **world**!'))
// <p>Hello <strong>world</strong>!</p>
```

## Security

The WASM binding does **not** sanitize or filter the generated HTML.

## Development

The WASM package is built with [wasm-pack](https://rustwasm.github.io/wasm-pack/).

```bash
cd bindings/wasm
npm run build
npm test
```
