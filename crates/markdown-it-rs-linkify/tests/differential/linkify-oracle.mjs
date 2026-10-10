import assert from "node:assert/strict";
import { LinkifyIt } from "linkify-it";
import readline from "node:readline";
import { pathToFileURL } from "node:url";
import { versions } from "./versions.mjs";

export { versions };

export function byteOffset(source, offset) {
  assert.ok(Number.isInteger(offset) && offset >= 0 && offset <= source.length);
  // A boundary inside a surrogate pair cannot be represented by a Rust str slice.
  if (offset > 0 && offset < source.length) {
    const left = source.charCodeAt(offset - 1);
    const right = source.charCodeAt(offset);
    assert.ok(!(left >= 0xd800 && left <= 0xdbff && right >= 0xdc00 && right <= 0xdfff));
  }
  return Buffer.byteLength(source.slice(0, offset), "utf8");
}

export function detect({ source, fuzzyLinks = false }) {
  assert.equal(typeof source, "string");
  assert.equal(typeof fuzzyLinks, "boolean");
  assert.equal(Buffer.from(source, "utf8").toString("utf8"), source);
  return (new LinkifyIt({ fuzzyLink: fuzzyLinks }).match(source) ?? []).map((match) => ({
    start: byteOffset(source, match.index),
    end: byteOffset(source, match.lastIndex),
    kind: match.schema === "mailto:" ? "email" : "url",
    raw: match.raw,
  }));
}

// Importable API and JSONL endpoint. Consumers own their fixtures and comparisons.
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  for await (const line of readline.createInterface({ input: process.stdin })) {
    try {
      const matches = detect(JSON.parse(line));
      process.stdout.write(JSON.stringify({ ...versions, matches }) + "\n");
    } catch (error) {
      process.stdout.write(JSON.stringify({ error: String(error) }) + "\n");
    }
  }
}
