import assert from "node:assert/strict";
import test from "node:test";

import { MarkdownIt as BasicMarkdownIt } from "../pkg/basic/basic.js";
import { MarkdownIt as FullMarkdownIt } from "../pkg/full/full.js";

const source = "<span>Ciallo</span>";

test("basic escapes HTML by default", () => {
  const md = new BasicMarkdownIt();
  assert.equal(md.render(source), "<p>&lt;span&gt;Ciallo&lt;/span&gt;</p>\n");
});

test("basic enables HTML explicitly", () => {
  const md = new BasicMarkdownIt({ allowHtml: true });
  assert.equal(md.render(source), "<p><span>Ciallo</span></p>\n");
});

test("allowHtml false keeps HTML escaped", () => {
  const md = new BasicMarkdownIt({ allowHtml: false });
  assert.equal(md.render(source), "<p>&lt;span&gt;Ciallo&lt;/span&gt;</p>\n");
});

test("full build uses the same HTML opt-in behavior", () => {
  const md = new FullMarkdownIt({ allowHtml: true });
  assert.equal(md.render(source), "<p><span>Ciallo</span></p>\n");
});

const syntaxCases = [
  {
    name: "heading",
    input: "# Hello",
    expected: "<h1>Hello</h1>\n",
  },
  {
    name: "emphasis",
    input: "**bold** and *italic*",
    expected: "<p><strong>bold</strong> and <em>italic</em></p>\n",
  },
  {
    name: "link",
    input: "[Rust](https://www.rust-lang.org)",
    expected: '<p><a href="https://www.rust-lang.org">Rust</a></p>\n',
  },
  {
    name: "unordered list",
    input: "- one\n- two",
    expected: "<ul>\n<li>one</li>\n<li>two</li>\n</ul>\n",
  },
  {
    name: "blockquote",
    input: "> quote",
    expected: "<blockquote>\n<p>quote</p>\n</blockquote>\n",
  },
  {
    name: "fenced code",
    input: "```rust\nfn main() {}\n```",
    expected: '<pre><code class="language-rust">fn main() {}\n</code></pre>\n',
  },
  {
    name: "inline code",
    input: "`code`",
    expected: "<p><code>code</code></p>\n",
  },
  {
    name: "hard break",
    input: "a  \nb",
    expected: "<p>a<br>\nb</p>\n",
  },
  {
    name: "entity",
    input: "&amp;",
    expected: "<p>&amp;</p>\n",
  },
];

for (const { name, input, expected } of syntaxCases) {
  test(`basic syntax: ${name}`, () => {
    const md = new BasicMarkdownIt();
    assert.equal(md.render(input), expected);
  });
}
