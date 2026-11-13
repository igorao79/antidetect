/** How many proxies a bulk check tries at once. */
export const BULK_PROXY_CHECK_CONCURRENCY = 4;

export interface BulkCheckSummary {
  ok: number;
  failed: number;
  /** Proxies a check was already running for. */
  busy: number;
}

/**
 * Runs `work` over `items`, at most `limit` at a time, and resolves when all
 * are done. `work` never rejects: the caller records its own outcomes.
 */
export async function runPool<T>(
  items: readonly T[],
  limit: number,
  work: (item: T) => Promise<void>,
): Promise<void> {
  let next = 0;
  const lane = async () => {
    while (next < items.length) {
      const item = items[next];
      next += 1;
      await work(item);
    }
  };
  const lanes = Math.max(1, Math.min(limit, items.length));
  await Promise.all(Array.from({ length: lanes }, lane));
}

/** One distinct proxy per id, for the profiles that use one without a VPN. */
export function proxiesToCheck<
  P extends { proxy_id?: string | null; vpn_id?: string | null },
  S extends { id: string },
>(profiles: readonly P[], stored: readonly S[]): S[] {
  const wanted = new Set(
    profiles
      .filter((profile) => profile.proxy_id && !profile.vpn_id)
      .map((profile) => profile.proxy_id),
  );
  return stored.filter((proxy) => wanted.has(proxy.id));
}
