import type {
  BrowserProfile,
  ProfileGroup,
  ProfileStatus,
  StoredProxy,
} from "@/types";

export const PROFILE_CSV_COLUMNS = [
  "name",
  "group",
  "tags",
  "note",
  "status",
  "proxyName",
  "proxyAddress",
  "engine",
  "created",
  "lastLaunched",
] as const;

export type ProfileCsvColumn = (typeof PROFILE_CSV_COLUMNS)[number];

export interface ProfilesCsvLookups {
  groups: readonly Pick<ProfileGroup, "id" | "name">[];
  statuses: readonly Pick<ProfileStatus, "id" | "name">[];
  proxies: readonly Pick<StoredProxy, "id" | "name" | "proxy_settings">[];
  vpns: readonly { id: string; name: string }[];
}

const ENGINE_NAMES: Record<string, string> = {
  camoufox: "Camoufox",
  cloakbrowser: "CloakBrowser",
  wayfern: "Wayfern",
};

/** A spreadsheet runs a cell that starts with one of these as a formula. */
const FORMULA_START = /^[=+\-@\t\r]/;

const pad = (value: number): string => String(value).padStart(2, "0");

/** One CSV field: quoted when needed, and defused when a sheet would run it. */
export function csvField(value: string): string {
  const safe = FORMULA_START.test(value) ? `'${value}` : value;
  return /[",\n\r;]/.test(safe) ? `"${safe.replaceAll('"', '""')}"` : safe;
}

/** `YYYY-MM-DD HH:MM` in local time; empty when the profile never had one. */
export function csvDate(seconds: number | undefined): string {
  if (!seconds) return "";
  const date = new Date(seconds * 1000);
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

function proxyCells(
  profile: BrowserProfile,
  lookups: ProfilesCsvLookups,
): { name: string; address: string } {
  const proxy = profile.proxy_id
    ? lookups.proxies.find((item) => item.id === profile.proxy_id)
    : undefined;
  if (proxy) {
    const { host, port } = proxy.proxy_settings;
    return { name: proxy.name, address: `${host}:${port}` };
  }
  const vpn = profile.vpn_id
    ? lookups.vpns.find((item) => item.id === profile.vpn_id)
    : undefined;
  return { name: vpn?.name ?? "", address: "" };
}

/** The cells of one profile, in `PROFILE_CSV_COLUMNS` order. */
export function profileCsvRow(
  profile: BrowserProfile,
  lookups: ProfilesCsvLookups,
): Record<ProfileCsvColumn, string> {
  const proxy = proxyCells(profile, lookups);
  return {
    name: profile.name,
    group:
      lookups.groups.find((group) => group.id === profile.group_id)?.name ?? "",
    tags: (profile.tags ?? []).join(", "),
    note: profile.note ?? "",
    status:
      lookups.statuses.find((status) => status.id === profile.status_id)
        ?.name ?? "",
    proxyName: proxy.name,
    proxyAddress: proxy.address,
    engine: ENGINE_NAMES[profile.browser] ?? profile.browser,
    created: csvDate(profile.created_at),
    lastLaunched: csvDate(profile.last_launch),
  };
}

/**
 * The whole file's text: a header of `headers` (one per column) and a line per
 * profile, CRLF-separated as spreadsheets expect. The byte order mark is
 * added when the file is written.
 */
export function profilesToCsv(
  profiles: readonly BrowserProfile[],
  lookups: ProfilesCsvLookups,
  headers: Record<ProfileCsvColumn, string>,
): string {
  const header = PROFILE_CSV_COLUMNS.map((column) =>
    csvField(headers[column]),
  ).join(",");
  const lines = profiles.map((profile) => {
    const row = profileCsvRow(profile, lookups);
    return PROFILE_CSV_COLUMNS.map((column) => csvField(row[column])).join(",");
  });
  return `${[header, ...lines].join("\r\n")}\r\n`;
}
