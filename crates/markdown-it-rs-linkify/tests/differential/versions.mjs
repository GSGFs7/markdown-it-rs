import assert from "node:assert/strict";
import { createRequire } from "node:module";
import pins from "./package.json" with { type: "json" };

const require = createRequire(import.meta.url);
const markdownRequire = createRequire(require.resolve("markdown-it"));
const linkifyRequire = createRequire(require.resolve("linkify-it"));

// Check the actual transitive oracle dependencies as well as the direct imports.
export const versions = {
  markdownItVersion: require("markdown-it/package.json").version,
  linkifyItVersion: markdownRequire("linkify-it/package.json").version,
  ucMicroVersion: linkifyRequire("uc.micro/package.json").version,
};

assert.equal(versions.markdownItVersion, pins.dependencies["markdown-it"]);
assert.equal(versions.linkifyItVersion, pins.dependencies["linkify-it"]);
assert.equal(require("linkify-it/package.json").version, versions.linkifyItVersion);
assert.equal(versions.ucMicroVersion, pins.dependencies["uc.micro"]);
assert.equal(
  createRequire(markdownRequire.resolve("linkify-it"))("uc.micro/package.json").version,
  versions.ucMicroVersion,
);
