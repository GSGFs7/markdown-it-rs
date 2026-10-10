import assert from "node:assert/strict";
import test from "node:test";
import { byteOffset, detect } from "./linkify-oracle.mjs";

test("oracle converts UTF-16 boundaries across astral and combining characters", () => {
  const source = "中😀e\u0301 https://example.org";
  assert.equal(byteOffset(source, source.indexOf("https:")), 11);
  assert.throws(() => byteOffset(source, 2));
  assert.throws(() => byteOffset(source, -1));
  assert.throws(() => byteOffset(source, source.length + 1));
});

test("oracle rejects requests Rust strings cannot represent", () => {
  assert.throws(() => detect({ source: "\ud800" }));
  assert.throws(() => detect({ source: 42 }));
  assert.throws(() => detect({ source: "", fuzzyLinks: "true" }));
});
