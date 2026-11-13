/**
 * A proxy's settings as one copyable URI: `scheme://user:pass@host:port`.
 * Xray types carry their share link in `vless_uri`, which is returned as is.
 *
 * Kept free of imports so `proxy-uri.test.mjs` can load it directly.
 */

export interface ProxyUriSettings {
  proxy_type: string;
  host: string;
  port: number;
  username?: string | null;
  password?: string | null;
  vless_uri?: string | null;
}

const SCHEME_ALIASES = new Map<string, string>([
  ["shadowsocks", "ss"],
  ["httpstls", "https"],
]);

const SHARE_LINK_SCHEMES = new Set([
  "vless",
  "vmess",
  "trojan",
  "hysteria2",
  "hy2",
]);

export function formatProxyUri(settings: ProxyUriSettings): string {
  const type = settings.proxy_type.trim().toLowerCase();
  const share = settings.vless_uri?.trim();
  if (share && SHARE_LINK_SCHEMES.has(type)) {
    return share;
  }
  const scheme = SCHEME_ALIASES.get(type) ?? type;
  const user = settings.username ?? "";
  const pass = settings.password ?? "";
  let credentials = "";
  if (user || pass) {
    credentials = encodeURIComponent(user);
    if (pass) {
      credentials += `:${encodeURIComponent(pass)}`;
    }
    credentials += "@";
  }
  const host =
    settings.host.includes(":") && !settings.host.startsWith("[")
      ? `[${settings.host}]`
      : settings.host;
  return `${scheme}://${credentials}${host}:${settings.port}`;
}
