/**
 * Destinations that only work against Donut's cloud (the cloud Cookie Bot and
 * the remote-control agent console). The fork runs without that cloud, so they
 * are left out of the rail, the shortcuts and the command palette; the local
 * counterpart of the Cookie Bot is "Warm up cookies" on a profile selection.
 */
export const CLOUD_ONLY_PAGES: ReadonlySet<string> = new Set([
  "cookieBot",
  "agent",
]);

export const CLOUD_ONLY_SHORTCUTS: ReadonlySet<string> = new Set([
  "goCookieBot",
  "goAgent",
]);
