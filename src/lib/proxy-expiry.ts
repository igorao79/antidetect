/**
 * The optional date a proxy's plan ends (`StoredProxy.expires_at`, unix
 * seconds). The form picks a calendar day and the proxy counts as valid
 * through the end of that local day.
 *
 * Kept free of runtime imports so `proxy-expiry.test.mjs` can load it directly.
 */

export const PROXY_EXPIRY_PRESET_DAYS = [7, 14, 30, 60, 90] as const;

/** At or below this many days left, profiles show a warning mark. */
export const PROXY_EXPIRY_WARNING_DAYS = 3;

const MS_PER_DAY = 86_400_000;
const DATE_INPUT = /^(\d{4})-(\d{2})-(\d{2})$/;

export type ProxyExpiryLevel = "ok" | "soon" | "expired";

export interface ProxyExpiryStatus {
  level: ProxyExpiryLevel;
  /** Calendar days from today to the expiry day; negative once it passed. */
  daysLeft: number;
  /** The expiry moment, for formatting. */
  date: Date;
}

const startOfDay = (date: Date): number =>
  new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();

const pad = (value: number): string => String(value).padStart(2, "0");

/** `YYYY-MM-DD` of a local date, the value an `<input type="date">` takes. */
export function toDateInputValue(date: Date): string {
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/** Unix seconds at the end of the picked local day, or null when not a date. */
export function expiryFromDateInput(value: string): number | null {
  const match = DATE_INPUT.exec(value.trim());
  if (!match) return null;
  const [year, month, day] = match.slice(1).map(Number);
  const end = new Date(year, month - 1, day, 23, 59, 59);
  if (end.getMonth() !== month - 1 || end.getDate() !== day) return null;
  return Math.floor(end.getTime() / 1000);
}

/** The date input's value for a stored expiry. */
export function dateInputFromExpiry(expiresAt: number): string {
  return toDateInputValue(new Date(expiresAt * 1000));
}

/** The date input's value `days` days after today. */
export function dateInputDaysFromToday(days: number, now = new Date()): string {
  return toDateInputValue(
    new Date(now.getFullYear(), now.getMonth(), now.getDate() + days),
  );
}

/** Where a proxy stands against its expiry; null when it has none. */
export function proxyExpiryStatus(
  expiresAt: number | null | undefined,
  now = new Date(),
): ProxyExpiryStatus | null {
  if (typeof expiresAt !== "number" || !Number.isFinite(expiresAt)) {
    return null;
  }
  const date = new Date(expiresAt * 1000);
  // Rounded, so a day that is 23 or 25 hours long around a clock change
  // still counts as one.
  const daysLeft = Math.round(
    (startOfDay(date) - startOfDay(now)) / MS_PER_DAY,
  );
  const level: ProxyExpiryLevel =
    date.getTime() <= now.getTime()
      ? "expired"
      : daysLeft <= PROXY_EXPIRY_WARNING_DAYS
        ? "soon"
        : "ok";
  return { level, daysLeft, date };
}

/** `12.11` (or the locale's order), with the year only when it is not this one. */
export function formatExpiryDate(
  date: Date,
  locale: string,
  now = new Date(),
): string {
  return new Intl.DateTimeFormat(locale, {
    day: "2-digit",
    month: "2-digit",
    ...(date.getFullYear() === now.getFullYear() ? {} : { year: "numeric" }),
  }).format(date);
}
