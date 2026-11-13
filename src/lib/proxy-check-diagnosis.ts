/**
 * Plain-language reading of a route failure: what the backend's code (or, for
 * a proxy check, its fixed English sentence) means for the person, and what
 * to try. Pure, so `node --test` reads it directly. The texts live in the
 * locale files under `routeDiagnosis.<kind>.{meaning,action}`.
 */

export type RouteFailureKind =
  | "timeout"
  | "unreachable"
  | "refused"
  | "dns"
  | "auth"
  | "forbidden"
  | "payment"
  | "gateway"
  | "tls"
  | "notWorking"
  | "missing"
  | "config"
  | "generic";

export interface RouteDiagnosis {
  kind: RouteFailureKind;
  meaningKey: string;
  actionKey: string;
  /**
   * The check runs from this computer, so a local VPN or system proxy can
   * change the verdict (an allow-list sees the VPN's address, not yours).
   * False where the cause is in the app's own settings or the account.
   */
  localNetworkMatters: boolean;
}

export const ROUTE_LOCAL_NOTE_KEY = "routeDiagnosis.localNote";

const CODE_KINDS: Record<string, RouteFailureKind> = {
  PROXY_UPSTREAM_TIMEOUT: "timeout",
  PROXY_UPSTREAM_UNREACHABLE: "unreachable",
  VPN_UNREACHABLE: "unreachable",
  SYSTEM_PROXY_UNREACHABLE: "unreachable",
  PROXY_AUTH_FAILED: "auth",
  PROXY_TLS_HANDSHAKE_FAILED: "tls",
  PROXY_PAYMENT_REQUIRED: "payment",
  PROXY_NOT_WORKING: "notWorking",
  VPN_NOT_WORKING: "notWorking",
  PROFILE_ROUTE_UNAVAILABLE: "notWorking",
  PROFILE_PROXY_MISSING: "missing",
  PROFILE_VPN_MISSING: "missing",
  PROXY_HOST_INVALID: "config",
  PROXY_PORT_INVALID: "config",
  PROXY_TYPE_UNSUPPORTED: "config",
};

/** Same order as the backend's `classify_proxy_error`, which words them. */
const MESSAGE_KINDS: ReadonlyArray<readonly [RegExp, RouteFailureKind]> = [
  [/connection refused|connection reset/, "refused"],
  [/timed out|deadline has elapsed/, "timeout"],
  [/no such host|dns|resolve/, "dns"],
  [/authentication|407|proxy auth/, "auth"],
  [/403|forbidden/, "forbidden"],
  [/402|payment required/, "payment"],
  [/502|bad gateway|503|service unavailable/, "gateway"],
  [/tls connection|tls handshake|certificate/, "tls"],
  [/socks.*unreachable|could not connect through the proxy/, "unreachable"],
  [/invalid proxy|unsupported proxy/, "config"],
];

const NO_LOCAL_EFFECT: ReadonlySet<RouteFailureKind> = new Set([
  "payment",
  "missing",
  "config",
]);

function diagnosisFor(kind: RouteFailureKind): RouteDiagnosis {
  return {
    kind,
    meaningKey: `routeDiagnosis.${kind}.meaning`,
    actionKey: `routeDiagnosis.${kind}.action`,
    localNetworkMatters: !NO_LOCAL_EFFECT.has(kind),
  };
}

export const GENERIC_ROUTE_DIAGNOSIS: RouteDiagnosis = diagnosisFor("generic");

function codeOf(raw: string): string | null {
  const start = raw.startsWith("{") ? 0 : raw.indexOf('{"code":');
  if (start < 0) return null;
  try {
    const parsed: unknown = JSON.parse(raw.slice(start));
    if (parsed && typeof parsed === "object" && "code" in parsed) {
      const { code } = parsed as { code: unknown };
      return typeof code === "string" ? code : null;
    }
  } catch {
    // not JSON
  }
  return null;
}

export interface DiagnoseOptions {
  /**
   * The error is known to come from a route check, so text that matches
   * nothing still gets the general advice. Without it such text is not
   * ours to explain and gives `null`.
   */
  assumeRoute?: boolean;
}

/**
 * The diagnosis for a failed proxy check or launch, or `null` when the error
 * is not about the route (a locked profile, a missing browser build).
 */
export function diagnoseRouteFailure(
  error: unknown,
  { assumeRoute = false }: DiagnoseOptions = {},
): RouteDiagnosis | null {
  const raw = error instanceof Error ? error.message : String(error ?? "");
  const code = codeOf(raw);
  if (code !== null) {
    const kind = CODE_KINDS[code];
    return kind ? diagnosisFor(kind) : null;
  }
  const text = raw.toLowerCase();
  for (const [pattern, kind] of MESSAGE_KINDS) {
    if (pattern.test(text)) return diagnosisFor(kind);
  }
  return assumeRoute && text.trim() !== "" ? GENERIC_ROUTE_DIAGNOSIS : null;
}

/** The diagnosis as one paragraph, for a toast that has only a string. */
export function diagnosisSentence(
  t: (key: string) => string,
  diagnosis: RouteDiagnosis,
): string {
  const parts = [t(diagnosis.meaningKey), t(diagnosis.actionKey)];
  if (diagnosis.localNetworkMatters) parts.push(t(ROUTE_LOCAL_NOTE_KEY));
  return parts.join(" ");
}
