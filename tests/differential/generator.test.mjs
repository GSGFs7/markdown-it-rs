import assert from "node:assert/strict";
import test from "node:test";
import { random, mutate, corpus } from "./generator.mjs";

test("mutation runs reproduce and preserve valid Unicode", () => {
  function generate(seed) {
    const rng = random(seed);
    return Array.from({ length: 1000 }, (_, i) =>
      mutate(corpus[i % corpus.length], rng),
    );
  }

  const values = generate(123);
  assert.deepEqual(values, generate(123));
  assert.notDeepEqual(values, generate(124));
  for (const value of values) {
    assert.equal(Buffer.from(value, "utf8").toString("utf8"), value);
    assert.ok(Array.from(value).length <= 4096);
  }
});
