import assert from "node:assert/strict";
import test from "node:test";
import { Readable } from "node:stream";
import { lines } from "./jsonl.mjs";
import { random, mutate, corpus, generate, strategies, configs } from "./generator.mjs";

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

for (const strategy of strategies) {
  test(`${strategy} generation reproduces bounded valid Unicode`, () => {
    const run = (seed) => {
      const rng = random(seed);
      return Array.from({ length: 500 }, () => generate(corpus, rng, strategy));
    };
    const values = run(123);
    assert.deepEqual(values, run(123));
    assert.notDeepEqual(values, run(124));
    for (const { source, strategy: chosen } of values) {
      assert.equal(Buffer.from(source, "utf8").toString("utf8"), source);
      assert.ok(Array.from(source).length <= 4096);
      assert.ok(strategies.includes(chosen) && chosen !== "mixed");
      if (strategy !== "mixed") assert.equal(chosen, strategy);
    }
    if (strategy === "mixed") {
      assert.deepEqual(new Set(values.map((value) => value.strategy)),
        new Set(strategies.filter((value) => value !== "mixed")));
    }
  });
}

test("mutation strategy preserves the historical input and config sequence", () => {
  const oldRng = random(42);
  const newRng = random(42);
  for (let i = 0; i < 1000; i++) {
    let source = corpus[Math.floor(oldRng() * corpus.length)];
    const rounds = 1 + Math.floor(oldRng() * 4);
    for (let j = 0; j < rounds; j++) source = mutate(source, oldRng);
    const config = configs[Math.floor(oldRng() * configs.length)];
    assert.equal(generate(corpus, newRng, "mutation").source, source);
    assert.equal(configs[Math.floor(newRng() * configs.length)], config);
  }
});

test("JSONL frames only on LF and preserves split multibyte characters", async () => {
  const values = [{ source: "a\u2028b\u2029c\u0085😀\r\n" }, { html: "中文" }];
  const bytes = Buffer.from(values.map((value) => JSON.stringify(value) + "\n").join(""));
  const stream = Readable.from(Array.from(bytes, (byte) => Buffer.from([byte])));
  const actual = [];
  for await (const line of lines(stream)) actual.push(JSON.parse(line));
  assert.deepEqual(actual, values);
  await assert.rejects(async () => {
    for await (const line of lines(Readable.from(['{"html":"x"}']))) {
      assert.fail(`unexpected incomplete line: ${line}`);
    }
  }, /incomplete JSONL/);
});
