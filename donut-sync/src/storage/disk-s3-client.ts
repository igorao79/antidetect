import { createHmac, randomBytes, timingSafeEqual } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {
  CreateBucketCommand,
  DeleteObjectCommand,
  DeleteObjectsCommand,
  GetObjectCommand,
  HeadBucketCommand,
  HeadObjectCommand,
  ListObjectsV2Command,
  PutObjectCommand,
} from "@aws-sdk/client-s3";
import { DiskObjectStore } from "./disk-object-store.js";

/** The error S3 gives for a missing object, in the shape the service checks. */
function notFound(): Error {
  return Object.assign(new Error("NotFound"), {
    name: "NotFound",
    $metadata: { httpStatusCode: 404 },
  });
}

type AnyCommand = { input: Record<string, unknown> };

/**
 * Stands in for the S3 client when the server stores objects on its own disk.
 * It answers the commands `SyncService` sends, with the same input and output
 * shapes, so the service's logic is the same for both kinds of storage.
 */
export class DiskS3Client {
  constructor(readonly store: DiskObjectStore) {}

  async send(command: AnyCommand): Promise<any> {
    const input = command.input;
    const key = input.Key as string;
    if (command instanceof HeadBucketCommand) {
      await this.store.ensureReady();
      return {};
    }
    if (command instanceof CreateBucketCommand) {
      await this.store.ensureReady();
      return {};
    }
    if (command instanceof PutObjectCommand) {
      const etag = await this.store.put(
        key,
        input.Body as string | Buffer,
        input.ContentType as string | undefined,
        input.Metadata as Record<string, string> | undefined,
      );
      return { ETag: etag };
    }
    if (command instanceof HeadObjectCommand) {
      const head = await this.store.head(key);
      if (!head) throw notFound();
      return {
        LastModified: head.lastModified,
        ContentLength: head.size,
        ETag: head.etag,
        ContentType: head.contentType,
        Metadata: head.metadata ?? {},
      };
    }
    if (command instanceof GetObjectCommand) {
      const opened = await this.store.open(key);
      if (!opened) throw notFound();
      return {
        Body: opened.stream,
        LastModified: opened.head.lastModified,
        ContentLength: opened.head.size,
        ETag: opened.head.etag,
        Metadata: opened.head.metadata ?? {},
      };
    }
    if (command instanceof DeleteObjectCommand) {
      await this.store.delete(key);
      return {};
    }
    if (command instanceof DeleteObjectsCommand) {
      const objects =
        (input.Delete as { Objects?: Array<{ Key?: string }> })?.Objects ?? [];
      const deleted: Array<{ Key: string }> = [];
      for (const object of objects) {
        if (!object.Key) continue;
        await this.store.delete(object.Key);
        deleted.push({ Key: object.Key });
      }
      return { Deleted: deleted };
    }
    if (command instanceof ListObjectsV2Command) {
      const result = await this.store.list(
        (input.Prefix as string | undefined) ?? "",
        (input.MaxKeys as number | undefined) ?? 1000,
        input.ContinuationToken as string | undefined,
        input.Delimiter as string | undefined,
      );
      return {
        Contents: result.objects.map((object) => ({
          Key: object.key,
          Size: object.size,
          LastModified: object.lastModified,
        })),
        CommonPrefixes: result.commonPrefixes.map((prefix) => ({
          Prefix: prefix,
        })),
        KeyCount: result.objects.length + result.commonPrefixes.length,
        IsTruncated: result.isTruncated,
        NextContinuationToken: result.nextToken,
      };
    }
    throw new Error(
      `Disk storage does not support ${command.constructor.name}`,
    );
  }
}

export type BlobOperation = "get" | "put";

/**
 * Signs and checks the URLs that stand in for presigned S3 URLs in disk mode:
 * `<public url>/v1/blob/<key>?exp=<unix seconds>&sig=<hex>&op=get|put`. The
 * secret is generated on first start and kept in the data directory, so a
 * restart does not invalidate URLs already handed out.
 */
export class BlobSigner {
  private readonly secret: Buffer;

  constructor(dataDir: string) {
    mkdirSync(dataDir, { recursive: true });
    const path = join(dataDir, "blob-secret");
    if (!existsSync(path)) {
      writeFileSync(path, randomBytes(32).toString("hex"), { mode: 0o600 });
    }
    this.secret = Buffer.from(readFileSync(path, "utf8").trim(), "hex");
  }

  private signature(op: BlobOperation, key: string, exp: number): string {
    return createHmac("sha256", this.secret)
      .update(`${op}\n${key}\n${exp}`)
      .digest("hex");
  }

  url(
    publicUrl: string,
    op: BlobOperation,
    key: string,
    expiresIn: number,
  ): string {
    const exp = Math.floor(Date.now() / 1000) + expiresIn;
    const path = key.split("/").map(encodeURIComponent).join("/");
    const query = new URLSearchParams({
      op,
      exp: String(exp),
      sig: this.signature(op, key, exp),
    });
    return `${publicUrl.replace(/\/+$/, "")}/v1/blob/${path}?${query}`;
  }

  verify(
    op: BlobOperation,
    key: string,
    exp: string | undefined,
    sig: string | undefined,
  ): boolean {
    const expires = Number(exp);
    if (!Number.isFinite(expires) || expires < Date.now() / 1000 || !sig) {
      return false;
    }
    const expected = Buffer.from(this.signature(op, key, expires));
    const given = Buffer.from(sig);
    return expected.length === given.length && timingSafeEqual(expected, given);
  }
}
