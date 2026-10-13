const regressions = [
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
