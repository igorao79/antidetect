/**
 * Pinned ("favourite") profiles. A pin is a preference of this device, not a
 * fact about the profile: it is kept in local storage and never synced.
 */

export const PINNED_PROFILES_KEY = "owl.pinnedProfiles";

/** Ids from a stored value; anything that is not a list of strings is empty. */
export function parsePinned(
  raw: string | null | undefined,
): ReadonlySet<string> {
  if (!raw) return new Set();
  try {
    const value: unknown = JSON.parse(raw);
    if (!Array.isArray(value)) return new Set();
    return new Set(value.filter((id): id is string => typeof id === "string"));
  } catch {
    return new Set();
  }
}

export function serializePinned(pinned: ReadonlySet<string>): string {
  return JSON.stringify([...pinned]);
}

/** A new set with every id pinned or unpinned; the given set is untouched. */
export function withPinned(
  pinned: ReadonlySet<string>,
  ids: readonly string[],
  value: boolean,
): ReadonlySet<string> {
  const next = new Set(pinned);
  for (const id of ids) {
    if (value) next.add(id);
    else next.delete(id);
  }
  return next.size === pinned.size &&
    ids.every((id) => pinned.has(id) === value)
    ? pinned
    : next;
}

/** True when every id is pinned, so a bulk action should unpin. */
export function allPinned(
  pinned: ReadonlySet<string>,
  ids: readonly string[],
): boolean {
  return ids.length > 0 && ids.every((id) => pinned.has(id));
}

/**
 * Pinned rows first, each group keeping the order it came in, so a pin rides
 * above whatever sort the table has.
 */
export function pinnedFirst<T>(
  rows: readonly T[],
  isPinned: (row: T) => boolean,
): T[] {
  const pinned: T[] = [];
  const rest: T[] = [];
  for (const row of rows) (isPinned(row) ? pinned : rest).push(row);
  return pinned.length === 0 ? [...rows] : [...pinned, ...rest];
}

type Listener = () => void;

/** One shared store, so the table, its menu and its filter agree. */
function createPinnedStore() {
  const listeners = new Set<Listener>();
  let snapshot: ReadonlySet<string> | null = null;

  const read = (): ReadonlySet<string> => {
    try {
      return parsePinned(window.localStorage.getItem(PINNED_PROFILES_KEY));
    } catch {
      return new Set();
    }
  };
  const emit = () => {
    for (const listener of listeners) listener();
  };

  return {
    getSnapshot(): ReadonlySet<string> {
      snapshot ??= read();
      return snapshot;
    },
    subscribe(listener: Listener): () => void {
      listeners.add(listener);
      const onStorage = (event: StorageEvent) => {
        if (event.key !== PINNED_PROFILES_KEY) return;
        snapshot = read();
        emit();
      };
      window.addEventListener("storage", onStorage);
      return () => {
        listeners.delete(listener);
        window.removeEventListener("storage", onStorage);
      };
    },
    set(ids: readonly string[], value: boolean) {
      const current = this.getSnapshot();
      const next = withPinned(current, ids, value);
      if (next === current) return;
      snapshot = next;
      try {
        window.localStorage.setItem(PINNED_PROFILES_KEY, serializePinned(next));
      } catch {
        // The pin lasts for this session when storage is unavailable.
      }
      emit();
    },
  };
}

export const pinnedProfilesStore = createPinnedStore();
