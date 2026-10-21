import { join } from "node:path";

/**
 * The self-hosted server's own settings, read from the environment each time
 * so a test can set them after the module has started.
 */

/** Where the database, the blob secret and disk-stored objects live. */
export function dataDir(): string {
  return process.env.DATA_DIR || join(process.cwd(), "data");
}

/** The address members' apps reach this server at. */
export function publicUrl(): string {
  const port = process.env.PORT ?? "12342";
  return (process.env.PUBLIC_URL || `http://localhost:${port}`).replace(
    /\/+$/,
    "",
  );
}

/** Whether PUBLIC_URL was set, or the local fallback is in use. */
export function publicUrlConfigured(): boolean {
  return Boolean(process.env.PUBLIC_URL);
}

/**
 * `disk` keeps objects in the data directory, `s3` in an S3 service. Unset,
 * a server configured with an S3 endpoint keeps using it.
 */
export function storageMode(): "disk" | "s3" {
  const mode = process.env.STORAGE?.toLowerCase();
  if (mode === "disk" || mode === "s3") return mode;
  return process.env.S3_ENDPOINT ? "s3" : "disk";
}

/** The name members see for this server. */
export function workspaceName(): string {
  return process.env.WORKSPACE_NAME?.trim() || "Owl";
}

/** The invite link a member's app opens. */
export function inviteLink(code: string): string {
  const query = new URLSearchParams({ server: publicUrl(), code });
  return `owl://join?${query}`;
}
