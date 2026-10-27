/**
 * Donut's cloud is switched off in this fork: no account to sign in to, no
 * sync, no Donut licence. Everything that only works against that cloud is
 * left out of the interface from here, so the upstream code stays in place
 * for merges and one flag brings it back.
 */
export const DONUT_CLOUD = false;

/**
 * Destinations that only work against Donut's cloud: the cloud Cookie Bot,
 * the remote-control agent console and the account page. They are left out of
 * the rail, the shortcuts and the command palette. The local counterpart of
 * the Cookie Bot is "Warm up cookies" on a profile selection.
 */
export const CLOUD_ONLY_PAGES: ReadonlySet<string> = new Set([
  "cookieBot",
  "agent",
  "account",
]);

export const CLOUD_ONLY_SHORTCUTS: ReadonlySet<string> = new Set([
  "goCookieBot",
  "goAgent",
  "goAccount",
  // Feedback is sent to Donut's servers.
  "sendFeedback",
]);

/** Table columns that show or switch sync, in every table that has one. */
export const CLOUD_ONLY_COLUMNS: readonly string[] = ["sync"];

/** Settings sections about the Donut licence and about sync. */
export const CLOUD_ONLY_SETTINGS: ReadonlySet<string> = new Set([
  "commercial",
  "encryption",
]);
