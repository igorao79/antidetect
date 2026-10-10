// usage: node cdp-probe.mjs <debug_port> [url] [--set-cookie name=value]
import { PROBE } from "./probe-expr.mjs";
const [port, url, ...rest] = process.argv.slice(2);
const setCookie = rest[0] === "--set-cookie" ? rest[1] : null;
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const page = targets.find((t) => t.type === "page");
if (!page) { console.error("no page target"); process.exit(1); }
const ws = new WebSocket(page.webSocketDebuggerUrl);
let id = 0; const pending = new Map();
const send = (method, params = {}) => new Promise((resolve, reject) => {
  const mid = ++id; pending.set(mid, { resolve, reject });
  ws.send(JSON.stringify({ id: mid, method, params }));
});
ws.onmessage = (ev) => {
  const msg = JSON.parse(ev.data);
  const p = pending.get(msg.id); if (!p) return; pending.delete(msg.id);
  msg.error ? p.reject(new Error(msg.error.message)) : p.resolve(msg.result);
};
await new Promise((r) => (ws.onopen = r));
try {
  if (url) {
    await send("Page.enable");
    await send("Page.navigate", { url });
    await new Promise((r) => setTimeout(r, 6000));
  }
  if (setCookie) {
    await send("Runtime.evaluate", { expression: `document.cookie = ${JSON.stringify(setCookie + "; max-age=86400; path=/")}` });
  }
  const r = await send("Runtime.evaluate", { expression: PROBE, awaitPromise: true, returnByValue: true });
  if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails).slice(0, 300));
  console.log(JSON.stringify(JSON.parse(r.result.value), null, 1));
} catch (e) { console.error("ERR", e.message); process.exitCode = 1; }
ws.close();
