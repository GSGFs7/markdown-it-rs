import assert from "node:assert/strict";
import test from "node:test";
import fixtures from "./linkify-fixtures.json" with { type: "json" };
import { detect } from "./linkify-oracle.mjs";
import { compare } from "./compare.mjs";
import { random, mutate } from "./generator.mjs";
import { structuredCases } from "./corpus.mjs";

test("systematic corpus is deterministic and uses representable Unicode", () => {
  const corpus = Array.from(structuredCases());
  assert.deepEqual(corpus, Array.from(structuredCases()));
  assert.equal(new Set(corpus.map(c => c.id)).size, corpus.length);
  assert.ok(corpus.some(c => c.request.source.includes("65536")));
  assert.ok(corpus.some(c => c.request.source.includes("[::ffff:")));
  assert.ok(corpus.some(c => c.request.source.length > 10000));
  for (const { request } of corpus) {
    assert.equal(Buffer.from(request.source).toString("utf8"), request.source);
  }
});

test("crate-owned mutations reproduce and preserve valid Unicode", () => {
  function generate(seed) {
    const rng = random(seed);
    return Array.from({ length: 1000 }, (_, i) => mutate(fixtures[i % fixtures.length].request.source, rng));
  }
  const values = generate(1145);
  assert.deepEqual(values, generate(1145));
  assert.notDeepEqual(values, generate(1146));
  for (const source of values) {
    assert.equal(Buffer.from(source, "utf8").toString("utf8"), source);
    assert.ok(Array.from(source).length <= 4096);
  }
});

test("fixture IDs are unique and known differences pin unequal outputs", () => {
  assert.equal(new Set(fixtures.map((fixture) => fixture.id)).size, fixtures.length);
  for (const fixture of fixtures) {
    assert.ok(fixture.id);
    assert.ok(Array.isArray(fixture.expected));
    if (fixture.knownDifference) {
      assert.ok(fixture.knownDifference.reason);
      assert.ok(Array.isArray(fixture.knownDifference.rust));
      assert.notDeepEqual(fixture.expected, fixture.knownDifference.rust);
    }
  }
});

for (const fixture of fixtures) {
  test(`pinned JS detection: ${fixture.id}`, () => {
    assert.deepEqual(detect(fixture.request), fixture.expected);
    const bytes = Buffer.from(fixture.request.source, "utf8");
    let previousEnd = 0;
    for (const match of fixture.expected) {
      assert.ok(Number.isInteger(match.start) && Number.isInteger(match.end));
      assert.ok(match.start >= previousEnd && match.end > match.start && match.end <= bytes.length);
      assert.equal(bytes.subarray(match.start, match.end).toString("utf8"), match.raw);
      previousEnd = match.end;
    }
  });
}

// Synthetic old regression: keep testing the comparison tool without allowing
// compatibility exceptions in the actual native scanner's fixture corpus.
const known = {
  ...fixtures.find((fixture) => fixture.id === "unmatched-parenthesis-unicode"),
  knownDifference: {
    reason: "Historical unmatched-parenthesis regression",
    rust: [{ start: 0, end: 26, kind: "url", raw: "https://例子.测试/a_(b" }],
  },
};

test("native detection corpus has no compatibility exceptions", () => {
  assert.ok(fixtures.every((fixture) => !fixture.knownDifference));
});

test("strict comparison fails the seed 1145 detection regression", () => {
  assert.equal(compare(known, known.expected, known.knownDifference.rust, false), "detection mismatch");
});

test("baseline accepts only the exact recorded known difference", () => {
  assert.equal(compare(known, known.expected, known.knownDifference.rust, true), null);
  assert.equal(compare(known, known.expected, [], true), "known Rust difference changed");
  assert.equal(compare(known, [], known.knownDifference.rust, true), "JS oracle changed");
});

test("baseline requires removal of resolved differences and rejects new differences", () => {
  assert.equal(compare(known, known.expected, known.expected, true), "known difference resolved; remove its baseline");
  const passing = fixtures.find((fixture) => fixture.id === "explicit-http");
  assert.equal(compare(passing, passing.expected, [], true), "detection mismatch");
});
