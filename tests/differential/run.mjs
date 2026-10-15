import { spawn, spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import { readFile, mkdir, writeFile, readdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import testgen from "markdown-it-testgen";
import { corpus, configs, random, generate, strategies } from "./generator.mjs";
import { lines } from "./jsonl.mjs";

const directory = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(directory, "../..");
const require = createRequire(import.meta.url);
const version = require("markdown-it/package.json").version;

const flags = new Map();
const argv = process.argv.slice(2);
for (let i = 0; i < argv.length; i++) {
  const flag = argv[i];
  if (["--fixtures", "--no-build", "--random-only"].includes(flag)) {
    flags.set(flag, true);
  } else if (
    [
      "--seed",
      "--iterations",
      "--timeout",
      "--replay",
      "--rust-bin",
      "--config",
      "--strategy",
    ].includes(flag)
  ) {
    if (!argv[i + 1] || argv[i + 1].startsWith("--")) {
      throw new Error(`missing value for ${flag}`);
    }
    flags.set(flag, argv[++i]);
  } else {
    throw new Error(`unknown argument: ${flag}`);
  }
}

function integer(flag, fallback, min, max) {
  const value = Number(flags.get(flag) ?? fallback);
  if (!Number.isSafeInteger(value) || value < min || value > max) {
    throw new Error(`invalid ${flag}`);
  }
  return value;
}

const seed = integer("--seed", 1, 0, 0xffffffff);
const iterations = integer("--iterations", 200, 0, 1_000_000);
const timeout = integer("--timeout", 5000, 1, 3_600_000);
const strategy = flags.get("--strategy") ?? "mixed";
if (!strategies.includes(strategy)) {
  throw new Error(`invalid --strategy: expected ${strategies.join(", ")}`);
}
const selectedConfigs = flags.has("--config")
  ? [configs[integer("--config", 0, 0, configs.length - 1)]]
  : configs;
const rustBin = path.resolve(
  flags.get("--rust-bin") ??
    path.join(root, "target/debug/examples/differential"),
);

if (!flags.has("--no-build")) {
  const build = spawnSync(
    "cargo",
    ["build", "-p", "markdown-it-rs", "--example", "differential", "--locked"],
    { cwd: root, stdio: "inherit" },
  );
  if (build.error) throw build.error;
  if (build.status !== 0) process.exit(build.status ?? 1);
}

function worker(command, args, label) {
  const child = spawn(command, args, {
    cwd: root,
    stdio: ["pipe", "pipe", "inherit"],
  });

  let pending;
  let failure;
  const fail = (error) => {
    failure = error;
    pending?.reject(error);
    pending = undefined;
  };

  child.on("error", fail);
  child.stdin.on("error", fail);
  child.on("exit", (code, signal) =>
    fail(new Error(`${label} exited: ${code ?? signal}`)),
  );

  const receive = async () => {
    for await (const line of lines(child.stdout)) {
      if (!pending) return fail(new Error(`${label}: unsolicited response`));
      const current = pending;
      pending = undefined;
      try {
        const result = JSON.parse(line);
        if (typeof result.html !== "string") {
          throw new Error(`${label}: ${result.error ?? "invalid response"}`);
        }
        current.resolve(result.html);
      } catch (error) {
        current.reject(error);
      }
    }
  };
  receive().catch(fail);

  return {
    request(value) {
      if (failure) return Promise.reject(failure);
      return new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
          fail(new Error(`${label}: exceeded ${timeout}ms`));
          child.kill("SIGKILL");
        }, timeout);
        pending = {
          resolve: (value) => {
            clearTimeout(timer);
            resolve(value);
          },
          reject: (error) => {
            clearTimeout(timer);
            reject(error);
          },
        };
        child.stdin.write(JSON.stringify(value) + "\n");
      });
    },
    close() {
      child.kill("SIGKILL");
    },
  };
}

const sources = [...corpus];
if (flags.has("--fixtures")) {
  for (const folder of ["commonmark", "markdown-it"]) {
    const fixtureDir = path.join(root, "tests/fixtures", folder);
    for (const filename of (await readdir(fixtureDir)).sort()) {
      if (!filename.endsWith(".txt")) continue;
      testgen.load(path.join(fixtureDir, filename), (data) => {
        for (const fixture of data.fixtures) {
          sources.push(fixture.first.text);
        }
      });
    }
  }
}

function* cases() {
  if (!flags.has("--random-only")) {
    for (const source of sources) {
      for (const config of selectedConfigs) {
        yield { request: { ...config, source }, strategy: "corpus" };
      }
    }
  }

  const rng = random(seed);
  for (let i = 0; i < iterations; i++) {
    const generated = generate(sources, rng, strategy);
    yield {
      request: {
        ...selectedConfigs[Math.floor(rng() * selectedConfigs.length)],
        source: generated.source,
      },
      strategy: generated.strategy,
    };
  }
}

// Replay stores the entire request, so reproducing does not depend on corpus order.
const replay = flags.has("--replay")
  ? JSON.parse(await readFile(path.resolve(flags.get("--replay")), "utf8"))
  : null;
if (replay && replay.markdownItVersion !== version) {
  throw new Error(
    `replay requires markdown-it ${replay.markdownItVersion}; installed ${version}`,
  );
}

const js = worker(process.execPath, [path.join(directory, "oracle.mjs")], "JS");
const rust = worker(rustBin, [], "Rust");
let count = 0;

try {
  for (const entry of replay ? [replay] : cases()) {
    const { request } = entry;
    let jsHtml, rustHtml, error;
    try {
      [jsHtml, rustHtml] = await Promise.all([
        js.request(request),
        rust.request(request),
      ]);
    } catch (cause) {
      error = String(cause);
    }
    count++;

    if (error || jsHtml !== rustHtml) {
      const record = {
        markdownItVersion: version,
        seed,
        strategy: entry.strategy,
        caseIndex: count - 1,
        request,
        jsHtml,
        rustHtml,
        error,
      };
      const failures = path.join(directory, "failures");
      await mkdir(failures, { recursive: true });
      const filename = path.join(
        failures,
        `${Date.now()}-${process.pid}-${seed}-${count}.json`,
      );
      await writeFile(filename, JSON.stringify(record, null, 2) + "\n");

      let offset = 0;
      if (!error) {
        while (
          offset < Math.min(jsHtml.length, rustHtml.length) &&
          jsHtml[offset] === rustHtml[offset]
        ) {
          offset++;
        }
      }
      console.error(error ?? `HTML mismatch at UTF-16 offset ${offset}`);
      console.error(JSON.stringify(record, null, 2));
      console.error(
        `Replay: node tests/differential/run.mjs --replay ${JSON.stringify(filename)}`,
      );
      process.exitCode = 1;
      break;
    }
  }

  if (!process.exitCode) {
    console.log(`PASS: ${count} cases; seed=${seed}; strategy=${replay ? "replay" : strategy}; markdown-it=${version}`);
  }
} finally {
  js.close();
  rust.close();
}
