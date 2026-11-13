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

type DatePart = "day" | "month" | "year";

const LONE_DAYS = /^\+?(\d{1,3})\s*[\p{L}.]*$/u;
const ISO_LIKE = /^(\d{4})\D+(\d{1,2})\D+(\d{1,2})$/;
const MAX_DAYS_TYPED = 3650;

/** The order a locale writes day, month and year in (`ru`: day, month, year). */
export function dateOrder(locale: string): DatePart[] {
  return new Intl.DateTimeFormat(locale, {
    day: "2-digit",
    month: "2-digit",
    year: "numeric",
  })
    .formatToParts(new Date(2026, 10, 12))
    .map((part) => part.type)
    .filter(
      (type): type is DatePart =>
        type === "day" || type === "month" || type === "year",
    );
}

/** A `YYYY-MM-DD` value the way the locale writes it in full: `03.11.2026`. */
export function formatDateField(value: string, locale: string): string {
  const expiry = expiryFromDateInput(value);
  if (expiry === null) return "";
  return new Intl.DateTimeFormat(locale, {
    day: "2-digit",
    month: "2-digit",
    year: "numeric",
  }).format(new Date(expiry * 1000));
}

const isoIfReal = (year: number, month: number, day: number): string | null => {
  const value = `${year}-${pad(month)}-${pad(day)}`;
  return expiryFromDateInput(value) === null ? null : value;
};

/**
 * What a person typed into the expiry field, as `YYYY-MM-DD`: a date in the
 * locale's order (`3.11.2026`, `3.11.26`, `3.11`, the next such day when the
 * year is left out), `2026-11-03`, or a number of days from today (`30`,
 * `30 дн.`). "" for an empty field, null for anything else.
 */
export function parseDateField(
  text: string,
  locale: string,
  now = new Date(),
): string | null {
  const typed = text.trim();
  if (!typed) return "";
  const days = LONE_DAYS.exec(typed);
  if (days) {
    const count = Number(days[1]);
    return count <= MAX_DAYS_TYPED ? dateInputDaysFromToday(count, now) : null;
  }
  const iso = ISO_LIKE.exec(typed);
  if (iso) return isoIfReal(Number(iso[1]), Number(iso[2]), Number(iso[3]));

  const numbers = typed.split(/\D+/).filter(Boolean);
  if (numbers.length !== 2 && numbers.length !== 3) return null;
  const order = dateOrder(locale).filter(
    (part) => numbers.length === 3 || part !== "year",
  );
  const value: Record<DatePart, number> = { day: 0, month: 0, year: 0 };
  order.forEach((part, index) => {
    value[part] = Number(numbers[index]);
  });
  if (numbers.length === 3) {
    const year = value.year < 100 ? 2000 + value.year : value.year;
    return isoIfReal(year, value.month, value.day);
  }
  const thisYear = isoIfReal(now.getFullYear(), value.month, value.day);
  if (thisYear === null) return null;
  const passed = (expiryFromDateInput(thisYear) ?? 0) * 1000 < startOfDay(now);
  return passed
    ? isoIfReal(now.getFullYear() + 1, value.month, value.day)
    : thisYear;
}

/** The weekday a locale's weeks start on, as `Date.getDay()` (Monday = 1). */
export function weekStart(locale: string): number {
  try {
    const info = new Intl.Locale(locale) as Intl.Locale & {
      getWeekInfo?: () => { firstDay: number };
      weekInfo?: { firstDay: number };
    };
    const firstDay = info.getWeekInfo?.().firstDay ?? info.weekInfo?.firstDay;
    return typeof firstDay === "number" ? firstDay % 7 : 1;
  } catch {
    return 1;
  }
}

export interface CalendarDay {
  /** `YYYY-MM-DD`. */
  value: string;
  day: number;
  inMonth: boolean;
}

/** Six weeks of days around a month (`month` 0-11), for a calendar grid. */
export function calendarDays(
  year: number,
  month: number,
  firstWeekday: number,
): CalendarDay[] {
  const first = new Date(year, month, 1);
  const lead = (first.getDay() - firstWeekday + 7) % 7;
  return Array.from({ length: 42 }, (_, index) => {
    const date = new Date(year, month, 1 - lead + index);
    return {
      value: toDateInputValue(date),
      day: date.getDate(),
      inMonth: date.getMonth() === month,
    };
  });
}

/** The `YYYY-MM-DD` `days` days from another such value. */
export function shiftDateInput(value: string, days: number): string {
  const [year, month, day] = value.split("-").map(Number);
  return toDateInputValue(new Date(year, month - 1, day + days));
}

/** The same day `months` months later, clamped to that month's last day. */
export function shiftDateInputMonths(value: string, months: number): string {
  const [year, month, day] = value.split("-").map(Number);
  const last = new Date(year, month - 1 + months + 1, 0).getDate();
  return toDateInputValue(
    new Date(year, month - 1 + months, Math.min(day, last)),
  );
}

const MIN_MASKED_DIGITS = 4;
const MAX_DATE_DIGITS = 8;

/** The separator a locale writes between day, month and year. */
function dateSeparator(locale: string): string {
  const literal = new Intl.DateTimeFormat(locale, {
    day: "2-digit",
    month: "2-digit",
    year: "numeric",
  })
    .formatToParts(new Date(2026, 10, 12))
    .find((part) => part.type === "literal");
  return literal?.value.trim().charAt(0) || ".";
}

/** Bare digits grouped into the locale's day, month and year order. */
function groupDateDigits(digits: string, locale: string): string {
  const widths = dateOrder(locale).map((part) => (part === "year" ? 4 : 2));
  const groups: string[] = [];
  let rest = digits;
  for (const width of widths) {
    if (!rest) break;
    groups.push(rest.slice(0, width));
    rest = rest.slice(width);
  }
  return groups.join(dateSeparator(locale));
}

/**
 * Separators put in while a date is typed as bare digits: `3112` reads
 * `31.12`, `31122026` reads `31.12.2026` (the locale's order and separator).
 * Up to three digits stay as they are, since that is a number of days. Text
 * the person separated themselves (`3.11.2026`, `2026-12-31`) is left alone;
 * only a digit typed after the field's own grouping is grouped again.
 */
export function maskDateDigits(text: string, locale: string): string {
  const digits = text.replace(/\D/g, "");
  if (digits.length < MIN_MASKED_DIGITS) return text;
  const bare = /^\d+$/.test(text);
  const typedAfterMask =
    !bare &&
    text === groupDateDigits(digits.slice(0, -1), locale) + digits.slice(-1);
  if (!bare && !typedAfterMask) return text;
  return groupDateDigits(digits.slice(0, MAX_DATE_DIGITS), locale);
}

/** An example date (30 days ahead) the way the locale writes it. */
export function expiryExample(locale: string, now = new Date()): string {
  return formatDateField(dateInputDaysFromToday(30, now), locale);
}

/** What the selection bar's "Extend by" menu offers. */
export const PROXY_EXPIRY_EXTEND_DAYS = [7, 30, 90] as const;

/**
 * The new expiry after extending by `days`: counted from the current date, or
 * from today when the proxy has none or has already expired. Valid through the
 * end of that day, like every stored expiry.
 */
export function extendedExpiry(
  expiresAt: number | null | undefined,
  days: number,
  now = new Date(),
): number {
  const status = proxyExpiryStatus(expiresAt, now);
  const base = status && status.level !== "expired" ? status.date : now;
  const target = new Date(
    base.getFullYear(),
    base.getMonth(),
    base.getDate() + days,
  );
  return (
    expiryFromDateInput(toDateInputValue(target)) ??
    Math.floor(target.getTime() / 1000)
  );
}

/** Soonest first, proxies without an expiry last. */
export function compareExpiry(
  a: number | null | undefined,
  b: number | null | undefined,
): number {
  const hasA = typeof a === "number";
  const hasB = typeof b === "number";
  if (hasA && hasB) return a - b;
  if (hasA) return -1;
  if (hasB) return 1;
  return 0;
}

/** Whether the plan has ended or ends within `days` days. */
export function expiresWithin(
  expiresAt: number | null | undefined,
  days: number,
  now = new Date(),
): boolean {
  const status = proxyExpiryStatus(expiresAt, now);
  return status !== null && status.daysLeft <= days;
}
