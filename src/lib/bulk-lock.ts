/** Profiles flagged `bulk_locked` stay out of bulk actions. */
interface LockableProfile {
  id: string;
  bulk_locked?: boolean;
}

export interface BulkLockSplit {
  /** The ids a bulk action may touch, in the order given. */
  allowed: string[];
  /** How many ids it must leave alone. */
  skipped: number;
}

export function splitBulkLocked(
  profiles: readonly LockableProfile[],
  ids: readonly string[],
): BulkLockSplit {
  const locked = new Set(
    profiles
      .filter((profile) => profile.bulk_locked)
      .map((profile) => profile.id),
  );
  const allowed = ids.filter((id) => !locked.has(id));
  return { allowed, skipped: ids.length - allowed.length };
}
