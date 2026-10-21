import type { SyncService } from "../sync/sync.service.js";
import { LOCK_TTL_MS, LocksService } from "./locks.service.js";

describe("LocksService", () => {
  const announced: string[] = [];
  const sync = {
    emitChange: (event: { key?: string }) => announced.push(event.key ?? ""),
  } as unknown as SyncService;

  beforeEach(() => {
    announced.length = 0;
    jest.useFakeTimers();
  });

  afterEach(() => {
    jest.useRealTimers();
  });

  it("gives a profile to one member at a time", () => {
    const locks = new LocksService(sync);
    expect(locks.acquire("p1", "m1", "Ivan")).toEqual({ success: true });
    expect(locks.acquire("p1", "m2", "Olga")).toEqual({
      success: false,
      lockedBy: "m1",
      lockedByEmail: "Ivan",
    });
    // The holder taking it again only renews it.
    expect(locks.acquire("p1", "m1", "Ivan")).toEqual({ success: true });
    expect(announced).toEqual(["profile-locks/p1"]);
  });

  it("is renewed and released only by its holder", () => {
    const locks = new LocksService(sync);
    locks.acquire("p1", "m1", "Ivan");
    expect(locks.heartbeat("p1", "m2")).toBe(false);
    expect(locks.heartbeat("p1", "m1")).toBe(true);
    expect(locks.release("p1", "m2")).toBe(false);
    expect(locks.release("p1", "m1")).toBe(true);
    expect(locks.list()).toEqual([]);
  });

  it("lapses when its app stops renewing it", () => {
    const locks = new LocksService(sync);
    locks.acquire("p1", "m1", "Ivan");
    jest.advanceTimersByTime(LOCK_TTL_MS + 1);
    expect(locks.list()).toEqual([]);
    expect(locks.acquire("p1", "m2", "Olga")).toEqual({ success: true });
  });

  it("drops what a removed member held", () => {
    const locks = new LocksService(sync);
    locks.acquire("p1", "m1", "Ivan");
    locks.acquire("p2", "m2", "Olga");
    locks.releaseAllOf("m1");
    expect(locks.list().map((lock) => lock.profileId)).toEqual(["p2"]);
  });
});
