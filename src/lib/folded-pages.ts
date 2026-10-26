/**
 * Pages the fork keeps without a rail item of their own, each under the page
 * it belongs to: groups beside profiles, the Owl server as the Integrations
 * page's Server tab. Shortcuts, the command palette, tips and `owl://` links
 * still open them, and the rail marks the page they live under.
 */
export const FOLDED_PAGES: Readonly<Record<string, string>> = {
  groups: "profiles",
  workspace: "integrations",
};

/** The rail item that stands for `page`. */
export function railPageOf(page: string): string {
  return FOLDED_PAGES[page] ?? page;
}
