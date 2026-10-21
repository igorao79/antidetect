import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Readable } from "node:stream";
import { DiskObjectStore, InvalidKeyError } from "./disk-object-store.js";
import { BlobSigner } from "./disk-s3-client.js";

describe("DiskObjectStore", () => {
  let dir: string;
  let store: DiskObjectStore;

  beforeEach(async () => {
    dir = mkdtempSync(join(tmpdir(), "owl-disk-"));
    store = new DiskObjectStore(dir);
    await store.ensureReady();
  });

  afterEach(() => {
    rmSync(dir, { recursive: true, force: true });
  });

  it("keeps an object's bytes, metadata and a changing ETag", async () => {
    const first = await store.put("profiles/p1/a.txt", "hello", "text/plain", {
      "updated-at": "100",
    });
    const head = await store.head("profiles/p1/a.txt");
    expect(head).toMatchObject({
      size: 5,
      etag: first,
      contentType: "text/plain",
      metadata: { "updated-at": "100" },
    });
    const second = await store.put("profiles/p1/a.txt", "hello again");
    expect(second).not.toBe(first);
  });

  it("stores a stream as it arrives", async () => {
    await store.put("big/file.bin", Readable.from([Buffer.from("ab"), "cd"]));
    const opened = await store.open("big/file.bin");
    const chunks: Buffer[] = [];
    for await (const chunk of opened?.stream ?? []) chunks.push(chunk);
    expect(Buffer.concat(chunks).toString()).toBe("abcd");
  });

  it("lists in key order, by page, and folds common prefixes", async () => {
    for (const key of ["profiles/b/1", "profiles/a/2", "profiles/a/1", "x/1"]) {
      await store.put(key, key);
    }
    const page1 = await store.list("profiles/", 2);
    expect(page1.objects.map((o) => o.key)).toEqual([
      "profiles/a/1",
      "profiles/a/2",
    ]);
    expect(page1.isTruncated).toBe(true);
    const page2 = await store.list("profiles/", 2, page1.nextToken);
    expect(page2.objects.map((o) => o.key)).toEqual(["profiles/b/1"]);
    expect(page2.isTruncated).toBe(false);

    const folded = await store.list("profiles/", 1000, undefined, "/");
    expect(folded.commonPrefixes).toEqual(["profiles/a/", "profiles/b/"]);
  });

  it("deletes an object and says whether it was there", async () => {
    await store.put("a/b", "x");
    expect(await store.delete("a/b")).toBe(true);
    expect(await store.head("a/b")).toBeNull();
    expect(await store.delete("a/b")).toBe(false);
  });

  it("refuses keys that would leave the store", async () => {
    for (const key of ["", "/etc/passwd", "a/../b", "a//b", "a\\b", "./a"]) {
      await expect(store.put(key, "x")).rejects.toBeInstanceOf(InvalidKeyError);
    }
  });
});

describe("BlobSigner", () => {
  it("accepts its own URLs for their operation and key until they expire", () => {
    const dir = mkdtempSync(join(tmpdir(), "owl-signer-"));
    try {
      const signer = new BlobSigner(dir);
      const url = new URL(
        signer.url("http://owl.test:12342/", "put", "profiles/p 1/a.txt", 60),
      );
      expect(url.pathname).toBe("/v1/blob/profiles/p%201/a.txt");
      const exp = url.searchParams.get("exp") ?? undefined;
      const sig = url.searchParams.get("sig") ?? undefined;
      expect(signer.verify("put", "profiles/p 1/a.txt", exp, sig)).toBe(true);
      expect(signer.verify("get", "profiles/p 1/a.txt", exp, sig)).toBe(false);
      expect(signer.verify("put", "profiles/p2/a.txt", exp, sig)).toBe(false);
      expect(signer.verify("put", "profiles/p 1/a.txt", "1", sig)).toBe(false);
      // The secret survives a restart: earlier URLs stay valid.
      expect(
        new BlobSigner(dir).verify("put", "profiles/p 1/a.txt", exp, sig),
      ).toBe(true);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
