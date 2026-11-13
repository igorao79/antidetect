const SPLITTERS = /[\s._-]+/;

/**
 * One or two capital letters for a person: the first letters of the first two
 * words of a name, or of an email's local part. "?" when there is nothing.
 */
export function holderInitials(label: string | null | undefined): string {
  const text = (label ?? "").trim();
  if (!text) return "?";
  const local = text.includes("@") ? text.slice(0, text.indexOf("@")) : text;
  const letters = local
    .split(SPLITTERS)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => [...part][0]);
  return letters.length > 0 ? letters.join("").toLocaleUpperCase() : "?";
}
