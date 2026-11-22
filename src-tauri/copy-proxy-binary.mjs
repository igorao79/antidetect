import { execFileSync, execSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
} from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { downloadXray } from "./download-xray.mjs";

const MANIFEST_DIR = dirname(fileURLToPath(import.meta.url));
const PROFILE =
  process.argv.includes("--release") || process.env.PROFILE === "release"
    ? "release"
    : "debug";

function getTarget() {
  if (process.env.TARGET) return process.env.TARGET;
  // Tauri names the triple of `tauri build --target <triple>` to its
  // beforeBuildCommand, so an Intel build gets an Intel sidecar and Xray.
  if (process.env.TAURI_ENV_TARGET_TRIPLE) {
    return process.env.TAURI_ENV_TARGET_TRIPLE;
  }
  try {
    const output = execSync("rustc -vV", { encoding: "utf-8" });
    const match = output.match(/host:\s*(.+)/);
    if (match) return match[1].trim();
  } catch {}
  return "unknown";
}

function getHostTarget() {
  try {
    const output = execSync("rustc -vV", { encoding: "utf-8" });
    const match = output.match(/host:\s*(.+)/);
    if (match) return match[1].trim();
  } catch {}
  return "unknown";
}

const TARGET = getTarget();
const HOST_TARGET = getHostTarget();
const isWindows = TARGET.includes("windows");

// Determine source directory (CARGO_TARGET_DIR: the Windows build from macOS
// keeps its target on an external disk, scripts/build-windows.sh).
const targetRoot = process.env.CARGO_TARGET_DIR || join(MANIFEST_DIR, "target");
const profileDir = PROFILE === "release" ? "release" : "debug";
const srcDir =
  TARGET === HOST_TARGET || TARGET === "unknown"
    ? join(targetRoot, profileDir)
    : join(targetRoot, TARGET, profileDir);

const destDir = join(MANIFEST_DIR, "binaries");
mkdirSync(destDir, { recursive: true });

/**
 * The environment the app's own `tauri build` compiles with. Tauri sets
 * MACOSX_DEPLOYMENT_TARGET from `bundle.macOS.minimumSystemVersion`; a
 * sidecar built without it gets other fingerprints for every crate with C
 * code (objc2, ring, wry...), so the app build compiled them all again.
 */
function cargoEnv() {
  // Only for the release build `tauri build` runs right after; local cargo
  // test and clippy runs have no deployment target, and mixing the two in
  // the debug target dir would rebuild those crates back and forth.
  if (
    PROFILE !== "release" ||
    process.platform !== "darwin" ||
    process.env.MACOSX_DEPLOYMENT_TARGET
  ) {
    return process.env;
  }
  try {
    const config = JSON.parse(
      readFileSync(join(MANIFEST_DIR, "tauri.conf.json"), "utf-8"),
    );
    const minimum = config?.bundle?.macOS?.minimumSystemVersion;
    return minimum
      ? { ...process.env, MACOSX_DEPLOYMENT_TARGET: minimum }
      : process.env;
  } catch {
    return process.env;
  }
}

function copyBinary(baseName) {
  const binName = isWindows ? `${baseName}.exe` : baseName;
  const source = join(srcDir, binName);

  let destName = `${baseName}-${TARGET}`;
  if (isWindows) destName += ".exe";
  const dest = join(destDir, destName);

  const buildArgs = ["build", "--bin", baseName];
  if (PROFILE === "release") buildArgs.push("--release");
  if (TARGET !== "unknown" && TARGET !== HOST_TARGET) {
    buildArgs.push("--target", TARGET);
  }
  execFileSync("cargo", buildArgs, {
    cwd: MANIFEST_DIR,
    env: cargoEnv(),
    stdio: "inherit",
  });

  if (!existsSync(source)) {
    console.error(`Error: Failed to build ${baseName} binary`);
    process.exit(1);
  }
  // macOS kills an executable that was overwritten in place.
  rmSync(dest, { force: true });
  copyFileSync(source, dest);
  console.log(`Built and copied ${binName} to ${dest}`);
}

copyBinary("donut-proxy");
await downloadXray(TARGET);
