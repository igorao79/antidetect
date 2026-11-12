const HOST_NAME = /^[\p{L}\p{N}_.-]+$/u;

function isIpv6(address: string): boolean {
  if (!address.includes(":")) {
    return false;
  }
  try {
    new URL(`http://[${address}]/`);
    return true;
  } catch {
    return false;
  }
}

/**
 * Whether `host` can be stored as a proxy host: an IPv4 or IPv6 address (the
 * latter with or without brackets) or a name of letters, digits, `-`, `_` and
 * `.`. A scheme, path, port suffix, space or markup is refused. Mirrors
 * `normalize_proxy_host` in `proxy_manager.rs`, which decides for the backend.
 */
export function isValidProxyHost(host: string): boolean {
  const trimmed = host.trim();
  if (!trimmed) {
    return false;
  }
  if (trimmed.startsWith("[") && trimmed.endsWith("]")) {
    return isIpv6(trimmed.slice(1, -1));
  }
  if (isIpv6(trimmed)) {
    return true;
  }
  return (
    HOST_NAME.test(trimmed) &&
    !trimmed.startsWith(".") &&
    !trimmed.endsWith(".")
  );
}
