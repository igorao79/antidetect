import { createHash, randomUUID } from "node:crypto";
import { createReadStream, createWriteStream } from "node:fs";
import {
  mkdir,
  readdir,
  readFile,
  rename,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { dirname, join } from "node:path";
import type { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";

/** What a stored object says about itself, as S3's HeadObject would. */
export interface DiskObjectHead {
  key: string;
  size: number;
  lastModified: Date;
  etag: string;
  contentType?: string;
  metadata?: Record<string, string>;
}

interface Sidecar {
  etag: string;
  contentType?: string;
  metadata?: Record<string, string>;
}

export interface DiskListResult {
  objects: Array<{ key: string; size: number; lastModified: Date }>;
  commonPrefixes: string[];
  isTruncated: boolean;
  nextToken?: string;
}

/** Thrown for a key that is not a plain relative path. */
export class InvalidKeyError extends Error {}

/**
 * Objects on the server's own disk, for a self-hosted server that runs without
 * an S3 service. `<root>/objects/<key>` holds the bytes and
 * `<root>/meta/<key>.json` the ETag, content type and user metadata.
 * Writes go to a temporary file first and are renamed into place, so a reader
 * never sees half an object.
 */
export class DiskObjectStore {
  private readonly objectsDir: string;
  private readonly metaDir: string;
  private readonly tmpDir: string;

  constructor(root: string) {
    this.objectsDir = join(root, "objects");
    this.metaDir = join(root, "meta");
    this.tmpDir = join(root, "tmp");
  }

  async ensureReady(): Promise<void> {
    await mkdir(this.objectsDir, { recursive: true });
    await mkdir(this.metaDir, { recursive: true });
    await mkdir(this.tmpDir, { recursive: true });
  }

  /** The key's segments, refusing anything that could leave the store. */
  static segments(key: string): string[] {
    if (
      typeof key !== "string" ||
      key.length === 0 ||
      key.length > 1024 ||
      key.includes("\0") ||
      key.includes("\\") ||
      key.startsWith("/")
    ) {
      throw new InvalidKeyError(`Invalid key: ${JSON.stringify(key)}`);
    }
    const parts = key.split("/");
    if (parts.some((part) => part === "" || part === "." || part === "..")) {
      throw new InvalidKeyError(`Invalid key: ${JSON.stringify(key)}`);
    }
    return parts;
  }

  private objectPath(key: string): string {
    return join(this.objectsDir, ...DiskObjectStore.segments(key));
  }

  private metaPath(key: string): string {
    const parts = DiskObjectStore.segments(key);
    parts[parts.length - 1] = `${parts[parts.length - 1]}.json`;
    return join(this.metaDir, ...parts);
  }

  private async readSidecar(key: string): Promise<Sidecar | null> {
    try {
      return JSON.parse(await readFile(this.metaPath(key), "utf8")) as Sidecar;
    } catch {
      return null;
    }
  }

  async put(
    key: string,
    body: Buffer | string | Readable,
    contentType?: string,
    metadata?: Record<string, string>,
  ): Promise<string> {
    const target = this.objectPath(key);
    const tmp = join(this.tmpDir, randomUUID());
    const hash = createHash("md5");
    if (typeof body === "string" || Buffer.isBuffer(body)) {
      const bytes = typeof body === "string" ? Buffer.from(body) : body;
      hash.update(bytes);
      await writeFile(tmp, bytes);
    } else {
      body.on("data", (chunk: Buffer) => hash.update(chunk));
      await pipeline(body, createWriteStream(tmp));
    }
    const etag = `"${hash.digest("hex")}"`;
    await mkdir(dirname(target), { recursive: true });
    await rename(tmp, target);
    const sidecar: Sidecar = { etag, contentType, metadata };
    const metaTarget = this.metaPath(key);
    await mkdir(dirname(metaTarget), { recursive: true });
    const metaTmp = join(this.tmpDir, randomUUID());
    await writeFile(metaTmp, JSON.stringify(sidecar));
    await rename(metaTmp, metaTarget);
    return etag;
  }

  async head(key: string): Promise<DiskObjectHead | null> {
    let info: Awaited<ReturnType<typeof stat>>;
    try {
      info = await stat(this.objectPath(key));
    } catch {
      return null;
    }
    if (!info.isFile()) return null;
    const sidecar = await this.readSidecar(key);
    return {
      key,
      size: info.size,
      lastModified: info.mtime,
      etag: sidecar?.etag ?? `"${info.size}-${info.mtimeMs}"`,
      contentType: sidecar?.contentType,
      metadata: sidecar?.metadata,
    };
  }

  /** The object's bytes as a stream, with its head; null when absent. */
  async open(
    key: string,
  ): Promise<{ head: DiskObjectHead; stream: Readable } | null> {
    const head = await this.head(key);
    if (!head) return null;
    return { head, stream: createReadStream(this.objectPath(key)) };
  }

  async delete(key: string): Promise<boolean> {
    const head = await this.head(key);
    await rm(this.objectPath(key), { force: true });
    await rm(this.metaPath(key), { force: true });
    return head !== null;
  }

  /** Every key under the store, sorted the way S3 lists them. */
  private async allKeys(): Promise<string[]> {
    const keys: string[] = [];
    const walk = async (dir: string, prefix: string) => {
      let entries: import("node:fs").Dirent[];
      try {
        entries = await readdir(dir, { withFileTypes: true });
      } catch {
        return;
      }
      for (const entry of entries) {
        const key = prefix ? `${prefix}/${entry.name}` : entry.name;
        if (entry.isDirectory()) {
          await walk(join(dir, entry.name), key);
        } else if (entry.isFile()) {
          keys.push(key);
        }
      }
    };
    await walk(this.objectsDir, "");
    return keys.sort();
  }

  async list(
    prefix: string,
    maxKeys = 1000,
    continuationToken?: string,
    delimiter?: string,
  ): Promise<DiskListResult> {
    const after = continuationToken
      ? Buffer.from(continuationToken, "base64url").toString("utf8")
      : undefined;
    const matching = (await this.allKeys()).filter(
      (key) => key.startsWith(prefix) && (after === undefined || key > after),
    );
    const objects: DiskListResult["objects"] = [];
    const commonPrefixes = new Set<string>();
    let last: string | undefined;
    let isTruncated = false;
    for (const key of matching) {
      if (objects.length + commonPrefixes.size >= maxKeys) {
        isTruncated = true;
        break;
      }
      last = key;
      if (delimiter) {
        const rest = key.slice(prefix.length);
        const cut = rest.indexOf(delimiter);
        if (cut >= 0) {
          commonPrefixes.add(prefix + rest.slice(0, cut + delimiter.length));
          continue;
        }
      }
      const head = await this.head(key);
      if (head) {
        objects.push({ key, size: head.size, lastModified: head.lastModified });
      }
    }
    return {
      objects,
      commonPrefixes: [...commonPrefixes],
      isTruncated,
      nextToken:
        isTruncated && last
          ? Buffer.from(last, "utf8").toString("base64url")
          : undefined,
    };
  }
}
