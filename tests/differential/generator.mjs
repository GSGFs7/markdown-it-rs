export const corpus = [
  "",
  "plain",
  "# heading",
  "**bold** and *em*",
  "**x [a*b**c*](d)",
  "- one\n  - two\n\n    continuation",
  "> quote\n>\n> - item",
  "| a | b |\n| :- | -: |\n| `x\\|y` | **z** |",
  "```rust\nfn main() {}\n```",
  '[a][id]\n\n[id]: /url "title"',
  "![alt *em*](image.png)",
  "<div>\n\n*text*\n</div>",
  '<a href="/">https://example.org</a>',
  "&amp; &#0; &#x1F600;",
  "https://例子.测试/a_(b)?x=1&y=2",
  "“中文”**强调** é\u0301 😀",
  '"quotes" ... (c) -- ---',
  "a  \nb\t",
  "\u0000\uFEFFtext",
  "a\rb\r\nc\n",
  "trailing  ",
  "trailing\t",
];

export const configs = [
  { preset: "default", options: {} },
  { preset: "commonmark", options: {} },
  {
    preset: "default",
    options: { html: true, linkify: true, typographer: true },
  },
  {
    preset: "default",
    options: { breaks: true, xhtmlOut: true, langPrefix: "" },
  },
  {
    preset: "commonmark",
    options: { html: false, breaks: true, xhtmlOut: false },
  },
  { preset: "default", options: { maxNesting: 3, html: true } },
];

// Specified 32-bit PRNG; runs reproduce across Node versions.
export function random(seed) {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let value = Math.imul(state ^ (state >>> 15), 1 | state);
    value ^= value + Math.imul(value ^ (value >>> 7), 61 | value);
    return ((value ^ (value >>> 14)) >>> 0) / 4294967296;
  };
}

const alphabet = Array.from("*_~`[]()!<>\\&;:#|\n\r\t abc09中文é😀\u0000");

export function mutate(source, rng) {
  // Work in Unicode code points: don't accidentally create lone surrogates
  // which Rust's UTF-8 String cannot represent.
  const chars = Array.from(source).slice(0, 4096);
  const index = Math.floor(rng() * (chars.length + 1));
  const count = 1 + Math.floor(rng() * 8);
  const inserted = Array.from(
    { length: count },
    () => alphabet[Math.floor(rng() * alphabet.length)],
  );

  switch (Math.floor(rng() * 5)) {
    case 0:
      chars.splice(index, count);
      break;
    case 1:
      chars.splice(index, 0, ...inserted);
      break;
    case 2:
      chars.splice(index, count, ...inserted);
      break;
    case 3:
      chars.splice(index, 0, ...chars.slice(index, index + count));
      break;
    case 4:
      chars.length = index;
      break;
  }

  return chars.slice(0, 4096).join("");
}
