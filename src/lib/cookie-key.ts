export function cookieKey(cookie: { name: string; path: string }): string {
  return `${cookie.name}\t${cookie.path}`;
}
