const regressions = [
  // Valid table headers take precedence over opening code fences.
  ...['```', '~~~'].map((marker) => `${marker}|b|\n|---|---|\n|x|y|`),
  '![</a>\n    \n</a> [</a>](\u0000 "\n")](```\n[\t][id]\n\n[id]: /url "\t"\n```|c*!文; b |\n| --- | --- |\n| &amp;&#10; | x |)',
  // Smartquotes sees raw image labels, without traversing parsed alt children.
  '"![&amp;&#10;](/img)"',
  '"![&#32;x&#32;](/img)"',
  '"![a\nb](/img)"',
  '"![![x](/inner)](/outer)"',
  '"![](/img)"',
  '"![&amp;&#10;][img]"\n\n[img]: /img',
  '| [| https://example.org/a_(b) | b |\n| --- | --- |\n| a b | x |][id]\n\n[id]: /url "![&amp;&#10;](中文é😀)" | b |\n| --- | --- |\n| - > &amp;&#10;\n  中文é😀\n\n  <a href="a b">\n  </a> | x |',

  // Delimiters and smartquotes use Markdown whitespace, not JS regex \s.
  ...['\u0085', '\u2028', '\u2029', '\uFEFF', '\u00A0', '\u2000'].flatMap((ch) => [
    `x${ch}_a_`, `*${ch}a*`, `*a${ch}*`, `_${ch}a_`,
    `**${ch}a**`, `~~${ch}a~~`, `a${ch}'x'`,
  ]),
  '<b\u0085_b____ c____](/url)',
  // Fence languages are split after unescaping, with JavaScript whitespace.
  ...['```', '~~~'].flatMap((marker) => [
    'js\uFEFFextra', '\uFEFFjs\uFEFF', '&#xfeff;js&#xfeff;extra',
    'js&#10;extra', 'js\u0085extra', '\u0085js\u0085', '\uFEFF', '&#xfeff;',
    'js\u200Bextra', 'js\u180Eextra', 'js&amp;x\uFEFFextra',
  ].map((info) => `${marker}${info}\ncode\uFEFF\n${marker}`)),
  // HTML whitespace follows JavaScript's \s (BOM included, NEL excluded).
  ...['\uFEFF', '\u0085'].flatMap((space) => [
    `<b${space}>`, `</b${space}>`, `<b>${space}`,
    `<b${space}title${space}=${space}x>`,
    `before <b${space}>after`,
    `<div${space}?>\n*raw*`, `<script${space}?>\n\n*raw*`,
    `<a${space}>a@example.com</a${space}> b@example.com`,
  ]),
  // Smartquotes must discard openers when leaving sibling emphasis nodes.
  '*a "b* *c" d*',
  // Scan complete explicit reference labels, including nested links.
  '[r]: /url\n[s]: /other\n\n[r][\n[x] [s] end]',
  '[r]: /url\n[s]: /other\n\n![r][\n[x] [s] end]',
  // Invalid inline targets resume reference scanning after the failed delimiter.
  '[x](url z[ref]\n\n[ref]: /uri',
  '[x](url 中[ref]\n\n[ref]: /uri',
  // Closing HTML anchors must not expose Markdown link labels to core linkify.
  '[</a>b@e[</a>b@example.com](/url)',
  // Entity-decoded newlines must not swallow the following softbreak.
  'foo&#10;\nbar',
  'foo&#10;&#10;\nbar',
  // A short unmatched label exercises the nesting cutoff; bulk cases live in
  // tests/pathological.rs.
  '[[[[le]](rl)',
  '![____&_|\t0___b_____ ___](/img)',
  '![*****|\r|c*_a ***\u0000_**b***** c*****](/img)\r;`&a|\n',
  '雪 ![____&_|\t0___b_____ ___](/img)',
  '![![____&_|\t0___b_____ ___](/inner)](/outer)',
  '![[____&_|\t0___b_____ ___](/link)](/img)',
  'b@example.com</a>',
  'mailto:b@example.com</a>',
  'b@example.com</a></a>',
  '*b@example.com*</A >',
  '`a`+b@example.oé</a>',
  'b@example.com</a> c@example.com',
  '<a>b@example.com</a> c@example.com',
  '</a>b@example.com',
  '*a*+b@example.com',
  '[</a>b@example.com](/url)',
  '![![\\)]()]()',
  '![![\\`]()]()',
  '![![x\\)y &amp; z]()]()',
  '![![`x` *y* <a@b.co>]()]()',
  '>|\n:-',
  '> a |\n---|\n',
  '> a |\n> ---|\n',
  '`a<`+b@example.oé! é[#\t&',
  '`a`b@example.com',
  '`a<b@example.com`+c@example.com',
  '`a`+b@example.com `c`+d@example.com',
  '`a`+b@example.com <a>+c@example.com</a>',
  '[`a`+b@example.com](/url)',
  '\\+b@example.com',
  '&amp;+b@example.com',
  '[foo]: <bar>(\naz)\n\n[foo]\no]\n',
  ...[['(', ')'], ["'", "'"], ['"', '"']].flatMap(([open, close]) => [
    `[foo]: <bar>${open}a\nb${close}\n\n[foo]\n`,
    `[foo]: <bar>${open}ab${close}\n\n[foo]\n`,
  ]),
  '<a>https://example.org',
  '<a>a@b.co //example.org',
  '<a>*https://example.org*</a',
  '<a>https://example.org</a> <a>https://example.org',
  '[<a>https://example.org](https://example.org)',
  '1. a\n    <!--',
  '- a\n  <!-- comment -->',
  '1. a\n    <div>',
  '1. a\n    <?x?>',
  '- <div>\n- foo\n',
  '1. a\n\n    <!--',
  '1. i&9文\t\n    <!--\n!c😀;    a\n\n    b\n    -->\n    c\n',
  '<https://example.org/"x">',
  "<foo'bar@example.org>",
  '"foo &quot;"',
  '"foo \\""',
  'foo&quot;"bar"',
  '"foo"&#32;bar',
  '"one `"inner"` two"',
  '!["alt" can\'t](/img) "outside"',
  '"one\n\ntwo"',
  '- "one\n- two"',
];

// Probe either side of the configured 3 / 20 / 100 nesting limits.
const nesting = [2, 3, 4, 19, 20, 21, 99, 100, 101].flatMap((depth) => [
  '> '.repeat(depth) + '*leaf* [x](/url)\n',
  Array.from({ length: depth }, (_, i) => '  '.repeat(i) + '- item').join('\n'),
  Array.from({ length: depth }, (_, i) => '> '.repeat(i) + '- > item').join('\n'),
  '*a **b '.repeat(depth) + 'leaf' + ' b** a*'.repeat(depth),
  '['.repeat(depth) + 'leaf' + ']'.repeat(depth) + '(/url)',
  '!['.repeat(depth) + 'leaf' + '](/img)'.repeat(depth),
]);

const blocks = [
  '- item\n\n  | a | b |\n  | --- | --- |\n  | **x** | `y` |',
  '> | a | b |\n> | --- | --- |\n> | x | y |',
  '- item\n\n  ```js\n  x()\n  ```\n\n  continuation',
  '> - item\n>\n>   ~~~\n>   code\n>   ~~~',
  '> paragraph\nlazy continuation\n\nnext',
  '- paragraph\nlazy continuation\n- next',
  '- tight\n- item\n\n  second paragraph\n\n- next',
  'paragraph\n---\n\nparagraph\n===',
  '- heading\n  ---\n- next',
  '> <div>\n> *inside*\n> </div>\n\n*outside*',
  '- item\n\n  [id]: /url "title"\n\n  [id]',
  '[id]\n\n> [id]: /url\n\n[id]',
  '[A B]\n\n[a\tb]: /first\n[A B]: /second',
  '[Σ]\n\n[σ]: /url\n\n[ς]',
  '[x](javascript:alert(1)) ![x](data:image/png;base64,AA==)',
  '[x](a_(b_(c))) [x](<a b>) [x](/url "a &quot; b")',
];

const delimiters = [1, 2, 3, 4, 5, 6, 9].flatMap((length) => {
  const tick = '`'.repeat(length);
  return [
    ...['*', '_', '~'].flatMap((marker) => {
      const run = marker.repeat(length);
      const text = `${run}a ${run}b${run} c${run}`;
      return [text, `[${text}](/url)`, `![${text}](/img)`, text.slice(0, -1)];
    }),
    `${tick} a\n\`b ${tick}`,
    `${tick}a${'`'.repeat(length + 1)}b${tick}`,
    `| a |\n| --- |\n| ${tick}x\\|y${tick} |`,
    `${tick}\ncode\n${'`'.repeat(length + 1)}`,
  ];
});

const unicode = ['\u00a0', '\u3000', '\u200b', '\u0301', '。', '—', '😀'].flatMap((char) => [
  `a${char}*b*${char}c`,
  `${char}_b_${char} **中文${char}é**`,
  `"${char}word${char}" can't users'`,
  `[a${char}b]\n\n[A${char}B]: /url`,
]);

const urls = [
  'https://例子.测试/a_(b)?x=1&y=2',
  "https://example.org/a'b",
  'https://example.org/a%20b%ZZ',
  'a+b@example.org',
  '//example.org/a',
].flatMap((url) => [
  `${url}).`, `*${url}*`, `\`${url}\``, `<${url}>`,
  `[${url}](/target)`, `<a>${url}</a>`, `<a>${url}`,
  `"${url}" &quot;${url}&quot;`,
]);

// Small versions of tests/pathological.rs inputs; compare HTML, not just crashes.
const pathological = [8, 32, 128, 512].flatMap((size) => [
  '*'.repeat(size) + 'a' + '*'.repeat(size),
  '[a'.repeat(size),
  '[a](<b'.repeat(size),
  '``\\'.repeat(size),
  '*a_ b* '.repeat(size),
  `</${'<!--'.repeat(size)}`,
  `${'x|'.repeat(size)}\n${'-|'.repeat(size)}\nx|\nx|`,
  Array.from({ length: size }, (_, i) => `[r${i}]: /url/${i}`).join('\n') +
  '\n\n' + Array.from({ length: size }, (_, i) => `[r${i}]`).join(' '),
]);

function variants(source) {
  const lf = source.replace(/\r\n?/g, '\n').replace(/\n$/, '');
  return [
    source, lf, lf + '\n',
    (lf + '\n').replace(/\n/g, '\r\n'),
    (lf + '\n').replace(/\n/g, '\r'),
    lf.replace(/\n\n/g, '\n \t\n'),
    lf.replace(/^( +)/gm, (spaces) => '\t'.repeat(Math.floor(spaces.length / 4)) +
      ' '.repeat(spaces.length % 4)),
  ];
}

const additionalCorpus = [...new Set([
  ...regressions, ...nesting, ...blocks, ...delimiters, ...unicode, ...urls,
  ...pathological,
].flatMap(variants))];

// Preserve the original seed order for reproducible mutation runs.
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
  ...additionalCorpus,
];
