export { corpus } from "./corpus.mjs";

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

// Keep the mutation alphabet stable so old mutation seeds remain reproducible.
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

export const strategies = ["mixed", "mutation", "structured", "splice", "random", "unicode"];

const pick = (values, rng) => values[Math.floor(rng() * values.length)];
const limit = (source) => Array.from(source).slice(0, 4096).join("");
const syntaxAlphabet = [...alphabet, ...Array.from("\"'=+-./%?@$0123456789\u00a0\u200b\u0301\uFEFF")];

// Mostly short inputs, with occasional long runs to exercise scanner boundaries.
function randomText(rng, unicode = false) {
  const max = pick([8, 32, 128, 4096], rng);
  const length = Math.floor(rng() * (max + 1));
  return Array.from({ length }, () => {
    if (!unicode) return pick(syntaxAlphabet, rng);
    // Sample Unicode scalar values, skipping the surrogate range.
    const scalar = Math.floor(rng() * (0x110000 - 0x800));
    return String.fromCodePoint(scalar < 0xd800 ? scalar : scalar + 0x800);
  }).join("");
}

const leaves = ["x", "a b", "中文é😀", "\u0000", "\t", "\n", '"word"',
  "&amp;&#10;", "https://example.org/a_(b)", "a+b@example.com", "</a>"];

function structured(rng, depth = 0) {
  if (depth >= 3 || rng() < 0.2) return pick(leaves, rng);
  const a = structured(rng, depth + 1);
  const b = structured(rng, depth + 1);
  const marker = pick(["*", "_", "~~", "`", "**"], rng);
  switch (Math.floor(rng() * 12)) {
    case 0: return `${marker}${a}${marker}${b}`;
    case 1: return `[${a}](${b} "${pick(leaves, rng)}")`;
    case 2: return `![${a}](${b})`;
    case 3: return `[${a}][id]\n\n[id]: /url "${b}"`;
    case 4: return `<a href="${b}">${a}</a>`;
    case 5: return `> ${a.replace(/\n/g, "\n> ")}\n${b}`;
    case 6: return `- ${a.replace(/\n/g, "\n  ")}\n\n  ${b.replace(/\n/g, "\n  ")}`;
    case 7: return `| ${a} | b |\n| --- | --- |\n| ${b} | x |`;
    case 8: return `\`\`\`${pick(["", "js", "`"], rng)}\n${a}\n\`\`\`\n${b}`;
    case 9: return `${a}\n${pick(["---", "===", "", "    "], rng)}\n${b}`;
    case 10: return `<div>\n${a}\n</div>\n\n${b}`;
    case 11: return `${a}${pick(["", " ", "\n", "\r\n", "\\"], rng)}${b}`;
  }
}

function splice(sources, rng) {
  const a = Array.from(pick(sources, rng)).slice(0, 2048);
  const b = Array.from(pick(sources, rng)).slice(0, 2048);
  const left = a.slice(0, Math.floor(rng() * (a.length + 1))).join("");
  const right = b.slice(Math.floor(rng() * (b.length + 1))).join("");
  return pick([
    () => left + right,
    () => `${left}\n\n${right}`,
    () => `[${left}](${right})`,
    () => `![${left}](${right})`,
    () => `> ${left.replace(/\n/g, "\n> ")}\n${right}`,
    () => `<a>${left}</a>${right}`,
  ], rng)();
}

// Return the chosen strategy too, so failures identify their input distribution.
export function generate(sources, rng, strategy = "mixed") {
  if (!strategies.includes(strategy)) throw new Error(`unknown strategy: ${strategy}`);
  if (sources.length === 0) throw new Error("empty source corpus");
  if (strategy === "mixed") {
    const roll = rng();
    strategy = roll < 0.5 ? "mutation" : roll < 0.75 ? "structured" :
      roll < 0.9 ? "splice" : roll < 0.98 ? "random" : "unicode";
  }
  let source;
  switch (strategy) {
    case "mutation": {
      source = pick(sources, rng);
      const rounds = 1 + Math.floor(rng() * 4);
      for (let i = 0; i < rounds; i++) source = mutate(source, rng);
      break;
    }
    case "structured":
      source = structured(rng);
      // Include damaged delimiters and indentation as well as complete structures.
      if (rng() < 0.5) source = mutate(source, rng);
      break;
    case "splice": source = splice(sources, rng); break;
    case "random": source = randomText(rng); break;
    case "unicode": source = randomText(rng, true); break;
  }
  return { source: limit(source), strategy };
}
