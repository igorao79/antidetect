/**
 * The proxy generator form's rules (fork): lifetime choices and the request
 * the backend's `proxy_generator_generate` takes. Pure, so `node --test`
 * checks it without Tauri.
 */

export type CountriesMode = "any" | "only" | "exclude";
export type GeneratorProtocol = "socks5" | "http";
export type TtlUnit = "minutes" | "hours" | "days";

export interface ProxyGeneratorOptions {
  enabled: boolean;
  countries: string[];
  popular_countries: string[];
  protocols: string[];
  ttl_hours: number[];
  ttl_minutes_min: number;
  ttl_minutes_max: number;
  bulk_max: number;
  owned_max: number;
}

export type TtlChoice =
  | { kind: "forever" }
  | { kind: "preset"; hours: number }
  | { kind: "custom"; amount: string; unit: TtlUnit };

export interface GeneratorFormState {
  count: number;
  countriesMode: CountriesMode;
  countries: string[];
  ttl: TtlChoice;
  name: string;
  note: string;
  protocol: GeneratorProtocol;
}

export interface GenerateProxiesRequest {
  count: number;
  countriesMode: CountriesMode;
  countries: string[];
  ttlMinutes: number | null;
  name: string | null;
  note: string | null;
  protocol: GeneratorProtocol;
}

export const TEXT_MAX = 120;
const MINUTES_IN: Record<TtlUnit, number> = {
  minutes: 1,
  hours: 60,
  days: 1440,
};

export const DEFAULT_FORM: GeneratorFormState = {
  count: 1,
  countriesMode: "any",
  countries: [],
  ttl: { kind: "forever" },
  name: "",
  note: "",
  protocol: "socks5",
};

/** The lifetime in minutes, `null` for none, `NaN` for an unreadable one. */
export function ttlMinutesOf(ttl: TtlChoice): number | null {
  if (ttl.kind === "forever") return null;
  if (ttl.kind === "preset") return ttl.hours * MINUTES_IN.hours;
  const amount = Number(ttl.amount.trim());
  if (!Number.isInteger(amount) || amount <= 0) return Number.NaN;
  return amount * MINUTES_IN[ttl.unit];
}

/** What is wrong with the form, as a translation key, or `null`. */
export function formProblem(
  form: GeneratorFormState,
  options: ProxyGeneratorOptions,
): string | null {
  if (
    !Number.isInteger(form.count) ||
    form.count < 1 ||
    form.count > options.bulk_max
  ) {
    return "proxyGenerator.errors.count";
  }
  if (form.countriesMode !== "any" && form.countries.length === 0) {
    return "proxyGenerator.errors.countries";
  }
  const minutes = ttlMinutesOf(form.ttl);
  if (
    minutes !== null &&
    (Number.isNaN(minutes) ||
      minutes < options.ttl_minutes_min ||
      minutes > options.ttl_minutes_max)
  ) {
    return "proxyGenerator.errors.ttl";
  }
  if (form.name.trim().length > TEXT_MAX || form.note.trim().length > TEXT_MAX)
    return "proxyGenerator.errors.textTooLong";
  return null;
}

export function toRequest(form: GeneratorFormState): GenerateProxiesRequest {
  const name = form.name.trim();
  const note = form.note.trim();
  return {
    count: form.count,
    countriesMode: form.countriesMode,
    countries: form.countriesMode === "any" ? [] : form.countries,
    ttlMinutes: ttlMinutesOf(form.ttl),
    name: name || null,
    note: note || null,
    protocol: form.protocol,
  };
}

/** Adds the country when it is not picked, removes it when it is. */
export function toggleCountry(picked: string[], code: string): string[] {
  return picked.includes(code)
    ? picked.filter((c) => c !== code)
    : [...picked, code];
}

/** Countries whose code or name holds the query, popular ones first. */
export function searchCountries(
  all: string[],
  popular: string[],
  query: string,
  nameOf: (code: string) => string,
): string[] {
  const q = query.trim().toLowerCase();
  const matches = all.filter(
    (code) => !q || code.includes(q) || nameOf(code).toLowerCase().includes(q),
  );
  const first = matches.filter((code) => popular.includes(code));
  const rest = matches
    .filter((code) => !popular.includes(code))
    .sort((a, b) => nameOf(a).localeCompare(nameOf(b)));
  return [...first, ...rest];
}
