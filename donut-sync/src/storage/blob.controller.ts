import type { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";
import { Controller, Get, Put, Req, Res } from "@nestjs/common";
import type { Request, Response } from "express";
import { SyncService } from "../sync/sync.service.js";
import { InvalidKeyError } from "./disk-object-store.js";
import type { BlobOperation } from "./disk-s3-client.js";

const PREFIX = "/v1/blob/";

/** The object key in a blob URL's path. */
function keyOf(req: Request): string | null {
  const path = req.path;
  if (!path.startsWith(PREFIX)) return null;
  try {
    return path
      .slice(PREFIX.length)
      .split("/")
      .map((part) => decodeURIComponent(part))
      .join("/");
  } catch {
    return null;
  }
}

function query(req: Request, name: string): string | undefined {
  const value = req.query[name];
  return typeof value === "string" ? value : undefined;
}

/**
 * Object transfers on a server that stores objects on its own disk. The URLs
 * are the ones `presign-upload` and `presign-download` hand out, signed by the
 * server, so a transfer needs no token of its own, as with S3.
 */
@Controller("v1/blob")
export class BlobController {
  constructor(private readonly sync: SyncService) {}

  private authorize(
    req: Request,
    res: Response,
    op: BlobOperation,
  ): string | null {
    const disk = this.sync.disk;
    const key = keyOf(req);
    if (
      !disk ||
      !key ||
      query(req, "op") !== op ||
      !disk.signer.verify(op, key, query(req, "exp"), query(req, "sig"))
    ) {
      res.status(403).end();
      return null;
    }
    return key;
  }

  @Put("*path")
  async put(@Req() req: Request, @Res() res: Response): Promise<void> {
    const key = this.authorize(req, res, "put");
    if (!key) return;
    const disk = this.sync.disk;
    if (!disk) return;
    // The JSON parser never reads blob transfers (`configureBodyParsing`), so
    // the body is still the request stream.
    const body: Readable = req;
    const updatedAt = req.header("x-amz-meta-updated-at");
    try {
      const etag = await disk.store.put(
        key,
        body,
        req.header("content-type") ?? undefined,
        updatedAt ? { "updated-at": updatedAt } : undefined,
      );
      res.setHeader("ETag", etag);
      res.status(200).end();
    } catch (error) {
      res.status(error instanceof InvalidKeyError ? 400 : 500).end();
    }
  }

  @Get("*path")
  async get(@Req() req: Request, @Res() res: Response): Promise<void> {
    const key = this.authorize(req, res, "get");
    if (!key) return;
    const disk = this.sync.disk;
    if (!disk) return;
    let opened: Awaited<ReturnType<typeof disk.store.open>>;
    try {
      opened = await disk.store.open(key);
    } catch {
      res.status(400).end();
      return;
    }
    if (!opened) {
      res.status(404).end();
      return;
    }
    const { head, stream } = opened;
    res.setHeader(
      "Content-Type",
      head.contentType ?? "application/octet-stream",
    );
    res.setHeader("Content-Length", String(head.size));
    res.setHeader("ETag", head.etag);
    res.setHeader("Last-Modified", head.lastModified.toUTCString());
    const updatedAt = head.metadata?.["updated-at"];
    if (updatedAt) res.setHeader("x-amz-meta-updated-at", updatedAt);
    res.status(200);
    await pipeline(stream, res).catch(() => res.destroy());
  }
}
