/**
 * Which running profiles have a proxy that stopped answering, from the
 * `proxy-health-changed` event, and what the banner and the system
 * notification say about them. Pure, so `node --test` reads it directly.
 */

export type HealthStatus = "ok" | "degraded" | "down";

export type ProxyHealthEvent =
  | { profile_id: string; status: HealthStatus; at: number; code?: string }
  | { profile_id: string; cleared: true };

export interface ProxyOutage {
  profileId: string;
  /** A backend error code, never an English sentence. */
  code?: string;
  /** Unix seconds of the first "down" verdict of this outage. */
  since: number;
  /**
   * The backend cut the session's worker (the "Block launches without a
   * proxy or VPN" switch): the browser runs on, but nothing reconnects it.
   */
  cut: boolean;
}

export type Outages = ReadonlyMap<string, ProxyOutage>;

export interface OutageRow {
  profileId: string;
  name: string;
  code?: string;
  cut: boolean;
}

/** How long a notification waits for more profiles of the same outage. */
export const NOTIFY_GROUP_WINDOW_MS = 3000;
/** Names a grouped notification spells out before "and N more". */
export const NOTIFY_MAX_NAMES = 3;

function without(outages: Outages, profileId: string): Outages {
  const next = new Map(outages);
  next.delete(profileId);
  return next;
}

function withOutage(outages: Outages, outage: ProxyOutage): Outages {
  const next = new Map(outages);
  next.set(outage.profileId, outage);
  return next;
}

/**
 * The outages after one health event. "ok" and "degraded" end an outage. A
 * `cleared` health ends it when the profile stopped; while the browser still
 * runs it means the backend cut the session, which stays an outage.
 * Returns the same map when nothing changed.
 */
export function applyHealthEvent(
  outages: Outages,
  event: ProxyHealthEvent,
  isRunning: (profileId: string) => boolean,
): Outages {
  const id = event.profile_id;
  const current = outages.get(id);
  if ("cleared" in event) {
    if (!current) return outages;
    if (!isRunning(id)) return without(outages, id);
    return current.cut
      ? outages
      : withOutage(outages, { ...current, cut: true });
  }
  if (event.status !== "down") {
    return current ? without(outages, id) : outages;
  }
  if (current) {
    if (current.code === event.code && !current.cut) return outages;
    return withOutage(outages, { ...current, code: event.code, cut: false });
  }
  return withOutage(outages, {
    profileId: id,
    code: event.code,
    since: event.at,
    cut: false,
  });
}

/** Drops the outages of profiles that were running and no longer are. */
export function pruneStopped(
  outages: Outages,
  previousRunning: ReadonlySet<string>,
  running: ReadonlySet<string>,
): Outages {
  const stopped = [...outages.keys()].filter(
    (id) => previousRunning.has(id) && !running.has(id),
  );
  if (stopped.length === 0) return outages;
  const next = new Map(outages);
  for (const id of stopped) next.delete(id);
  return next;
}

/**
 * The banner's rows: outages of profiles the app knows, oldest first. A
 * deleted profile has no row.
 */
export function outageRows(
  outages: Outages,
  profiles: readonly { id: string; name: string }[],
): OutageRow[] {
  const names = new Map(profiles.map((profile) => [profile.id, profile.name]));
  return [...outages.values()]
    .filter((outage) => names.has(outage.profileId))
    .sort(
      (a, b) =>
        a.since - b.since ||
        (names.get(a.profileId) ?? "").localeCompare(
          names.get(b.profileId) ?? "",
        ),
    )
    .map((outage) => ({
      profileId: outage.profileId,
      name: names.get(outage.profileId) ?? "",
      code: outage.code,
      cut: outage.cut,
    }));
}

export type OutageNotice =
  | { kind: "one"; name: string; code?: string }
  | {
      kind: "many";
      count: number;
      names: string[];
      more: number;
      /** The reason, when every profile has the same one. */
      code?: string;
    };

/**
 * One notice for the profiles that went down within one grouping window:
 * a single profile by name, several (one proxy behind many profiles) as one
 * grouped notice. `null` when all of them recovered or stopped meanwhile.
 */
export function outageNotice(
  batch: ReadonlySet<string>,
  rows: readonly OutageRow[],
): OutageNotice | null {
  const fresh = rows.filter((row) => batch.has(row.profileId));
  if (fresh.length === 0) return null;
  if (fresh.length === 1) {
    return { kind: "one", name: fresh[0].name, code: fresh[0].code };
  }
  const codes = new Set(fresh.map((row) => row.code));
  return {
    kind: "many",
    count: fresh.length,
    names: fresh.slice(0, NOTIFY_MAX_NAMES).map((row) => row.name),
    more: Math.max(0, fresh.length - NOTIFY_MAX_NAMES),
    code: codes.size === 1 ? fresh[0].code : undefined,
  };
}
