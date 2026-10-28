#!/usr/bin/env node
// The version the next release ships as. The release workflow pipes in the
// tags already released (one per line); when the app's version is among them,
// the patch number after the highest of them is written to every file that
// carries the version, and the workflow commits it.
//
//   gh release list --json tagName -q '.[].tagName' | node scripts/next-version.mjs
//
// Prints `version=<x.y.z>` and `bumped=true|false`, for $GITHUB_OUTPUT.

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const file = (path) => resolve(root, path);

const parse = (version) => version.replace(/^v/, "").split(".").map(Number);
const newer = (a, b) => {
  for (let i = 0; i < 3; i++) if (a[i] !== b[i]) return a[i] > b[i];
  return false;
};

const current = JSON.parse(
  readFileSync(file("src-tauri/tauri.conf.json"), "utf8"),
).version;
const released = readFileSync(0, "utf8")
  .split(/\s+/)
  .filter((tag) => /^v\d+\.\d+\.\d+$/.test(tag))
  .map(parse);

if (!released.some((tag) => tag.join(".") === current)) {
  console.log(`version=${current}\nbumped=false`);
  process.exit(0);
}

let highest = parse(current);
for (const tag of released) if (newer(tag, highest)) highest = tag;
const next = `${highest[0]}.${highest[1]}.${highest[2] + 1}`;

// Each pattern matches the app's own version line and nothing else.
const edits = [
  ["package.json", /^(\s*"version": ")[^"]+(")/m],
  ["src-tauri/tauri.conf.json", /^(\s*"version": ")[^"]+(")/m],
  ["src-tauri/Cargo.toml", /^(version = ")[^"]+(")/m],
  ["src-tauri/Cargo.lock", /(name = "donutbrowser"\nversion = ")[^"]+(")/],
];
for (const [path, pattern] of edits) {
  const text = readFileSync(file(path), "utf8");
  if (!pattern.test(text)) throw new Error(`no version line in ${path}`);
  writeFileSync(file(path), text.replace(pattern, `$1${next}$2`));
}
console.log(`version=${next}\nbumped=true`);
