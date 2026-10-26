/**
 * Pages the fork keeps without a rail item of their own, each under the page
 * it belongs to: groups are managed from the profiles page's group strip.
 * Shortcuts, the command palette and tips still open them, and the rail marks
 * the page they live under.
 */
export const FOLDED_PAGES: Readonly<Record<string, string>> = {
  groups: "profiles",
};

/** The rail item that stands for `page`. */
export function railPageOf(page: string): string {
  return FOLDED_PAGES[page] ?? page;
}
