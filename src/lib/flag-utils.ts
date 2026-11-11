import i18n from "@/i18n";

/**
 * Get flag icon CSS class for a country code (ISO 3166-1 alpha-2)
 */
export function getFlagIconClass(countryCode: string): string {
  if (countryCode?.length !== 2) {
    return "";
  }
  return `fi fi-${countryCode.toLowerCase()}`;
}

const RELATIVE_UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ["day", 86_400],
  ["hour", 3600],
  ["minute", 60],
];

/**
 * Format a unix-seconds timestamp relative to now in the interface language
 * ("2 minutes ago", "16 дней назад").
 */
export function formatRelativeTime(
  timestamp: number,
  language: string = i18n.language,
): string {
  const formatter = new Intl.RelativeTimeFormat(language || "en", {
    numeric: "auto",
  });
  const secondsAgo = Math.floor(Date.now() / 1000) - timestamp;
  for (const [unit, size] of RELATIVE_UNITS) {
    if (secondsAgo >= size) {
      return formatter.format(-Math.floor(secondsAgo / size), unit);
    }
  }
  return formatter.format(0, "second");
}
