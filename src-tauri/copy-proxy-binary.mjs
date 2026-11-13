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

// Determine source directory
let srcDir;
if (TARGET === HOST_TARGET || TARGET === "unknown") {
  srcDir = join(
    MANIFEST_DIR,
    "target",
    PROFILE === "release" ? "release" : "debug",
  );
} else {
  srcDir = join(
    MANIFEST_DIR,
    "target",
    TARGET,
    PROFILE === "release" ? "release" : "debug",
  );
}

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
