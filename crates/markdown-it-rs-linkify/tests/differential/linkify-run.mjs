import { spawnSync } from "node:child_process";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { random, mutate } from "./generator.mjs";
import { detect, versions } from "./linkify-oracle.mjs";
import { compare } from "./compare.mjs";
import { rustWorker } from "./linkify-worker.mjs";

const directory = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(directory, "../../../..");
const fixtures = JSON.parse(await readFile(path.join(directory, "linkify-fixtures.json"), "utf8"));
const flags = new Map();
const args = process.argv.slice(2);
for (let i = 0; i < args.length; i++) {
  const flag = args[i];
  if (["--baseline", "--no-build", "--random-only"].includes(flag)) {
    flags.set(flag, true);
  } else if (["--seed", "--iterations", "--replay", "--rust-bin"].includes(flag)) {
    if (!args[i + 1] || args[i + 1].startsWith("--")) throw new Error(`missing value for ${flag}`);
    flags.set(flag, args[++i]);
  } else {
    throw new Error(`unknown argument: ${flag}`);
  }
}

function integer(flag, fallback, max) {
  const value = Number(flags.get(flag) ?? fallback);
  if (!Number.isSafeInteger(value) || value < 0 || value > max) throw new Error(`invalid ${flag}`);
  return value;
}

const seed = integer("--seed", 1, 0xffffffff);
const iterations = integer("--iterations", 0, 1_000_000);
const baseline = flags.has("--baseline");
if (baseline && (iterations || flags.has("--replay") || flags.has("--random-only"))) {
  throw new Error("--baseline only accepts the reviewed fixtures, without mutations or replay");
}
const replay = flags.has("--replay")
  ? JSON.parse(await readFile(path.resolve(flags.get("--replay")), "utf8"))
  : null;
if (replay) {
  if (replay.mode !== "linkify") throw new Error("not a linkify detection replay");
  for (const [key, version] of Object.entries(versions)) {
    if (replay[key] !== version) throw new Error(`replay requires ${key} ${replay[key]}; installed ${version}`);
  }
}

if (!flags.has("--no-build")) {
  const build = spawnSync("cargo", ["build", "-p", "markdown-it-rs-linkify", "--example", "linkify_differential", "--locked"], {
    cwd: root,
    stdio: "inherit",
  });
  if (build.error) throw build.error;
  if (build.status !== 0) process.exit(build.status ?? 1);
}
const rustBin = path.resolve(flags.get("--rust-bin") ?? path.join(root, "target/debug/examples/linkify_differential"));

function* cases() {
  if (replay) {
    yield { id: replay.id, request: replay.request };
    return;
  }
  if (!flags.has("--random-only")) yield* fixtures;
  const rng = random(seed);
  for (let i = 0; i < iterations; i++) {
    const fixture = fixtures[Math.floor(rng() * fixtures.length)];
    let source = fixture.request.source;
    const rounds = 1 + Math.floor(rng() * 4);
    for (let j = 0; j < rounds; j++) source = mutate(source, rng);
    yield { id: `mutation-${i}`, request: { source, fuzzyLinks: rng() < 0.5 } };
  }
}

let count = 0;
let known = 0;
const rust = rustWorker(rustBin, root);
try {
  for (const fixture of cases()) {
    const jsMatches = detect(fixture.request);
    let rustMatches, error;
    try {
      rustMatches = await rust.request(fixture.request);
      error = compare(fixture, jsMatches, rustMatches, baseline);
    } catch (cause) {
      error = String(cause);
    }
    const caseIndex = count++;
    if (error) {
      const record = {
        mode: "linkify",
        ...versions,
        seed: replay?.seed ?? seed,
        caseIndex: replay?.caseIndex ?? caseIndex,
        id: fixture.id,
        request: fixture.request,
        jsMatches,
        rustMatches,
        error,
      };
      const failures = path.join(directory, "failures");
      await mkdir(failures, { recursive: true });
      const filename = path.join(failures, `linkify-${Date.now()}-${process.pid}-${seed}-${caseIndex}.json`);
      await writeFile(filename, JSON.stringify(record, null, 2) + "\n");
      console.error(JSON.stringify(record, null, 2));
      console.error(`Replay: node crates/markdown-it-rs-linkify/tests/differential/linkify-run.mjs --replay ${JSON.stringify(filename)}`);
      process.exitCode = 1;
      break;
    }
    if (baseline && fixture.knownDifference) {
      known++;
      console.log(`KNOWN: ${fixture.id}: ${fixture.knownDifference.reason}`);
    }
  }
} finally {
  rust.close();
}
if (!process.exitCode) {
  console.log(`PASS: ${count} cases; ${known} known differences; seed=${seed}; linkify-it=${versions.linkifyItVersion}; uc.micro=${versions.ucMicroVersion}`);
}
