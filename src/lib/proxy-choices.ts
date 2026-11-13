export interface NamedChoice {
  id: string;
  name: string;
}

/**
 * The picker order: the route the profile already uses first, the rest
 * alphabetically. Returns a new list; the input is never reordered.
 */
export function orderProxyChoices<T extends NamedChoice>(
  choices: readonly T[],
  currentId: string | null | undefined,
): T[] {
  const collator = new Intl.Collator(undefined, {
    numeric: true,
    sensitivity: "base",
  });
  const byName = (a: T, b: T) => collator.compare(a.name, b.name);
  const current = currentId
    ? choices.filter((choice) => choice.id === currentId)
    : [];
  const rest = choices.filter((choice) => choice.id !== currentId);
  return [...current, ...[...rest].sort(byName)];
}

/** The proxy every listed profile shares, or null when they differ or have none. */
export function sharedProxyId(
  proxyIds: readonly (string | null | undefined)[],
): string | null {
  const first = proxyIds[0];
  if (!first) return null;
  return proxyIds.every((id) => id === first) ? first : null;
}

/**
 * The main language of an exit country, from the locale data the platform
 * ships ("DE" -> "de"). Null when the country code is not a region.
 */
export function likelyLanguage(
  countryCode: string | undefined | null,
): string | null {
  if (!countryCode || !/^[A-Za-z]{2}$/.test(countryCode)) return null;
  try {
    const locale = new Intl.Locale(`und-${countryCode.toUpperCase()}`);
    return locale.maximize().language;
  } catch {
    return null;
  }
}
