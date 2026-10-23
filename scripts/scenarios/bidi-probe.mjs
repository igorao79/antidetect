// usage: node bidi-probe.mjs <ws_endpoint> [url] [--set-cookie name=value]
import { PROBE } from "./probe-expr.mjs";
const [endpoint, url, ...rest] = process.argv.slice(2);
const setCookie = rest[0] === "--set-cookie" ? rest[1] : null;
const ws = new WebSocket(endpoint);
let id = 0; const pending = new Map();
const send = (method, params = {}, timeout = 60000) => new Promise((resolve, reject) => {
  const mid = ++id; pending.set(mid, { resolve, reject });
  ws.send(JSON.stringify({ id: mid, method, params }));
  setTimeout(() => { if (pending.delete(mid)) reject(new Error("timed out: " + method)); }, timeout);
});
ws.onmessage = (ev) => {
  const msg = JSON.parse(ev.data);
  const p = pending.get(msg.id); if (!p) return; pending.delete(msg.id);
  msg.type === "error" ? p.reject(new Error(msg.error + ": " + msg.message)) : p.resolve(msg.result);
};
ws.onerror = (e) => { console.error("WS error", e.message ?? ""); process.exitCode = 1; };
await new Promise((r) => (ws.onopen = r));
let sessionOpen = false;
try {
  await send("session.new", { capabilities: {} });
  sessionOpen = true;
  const tree = await send("browsingContext.getTree", {});
  const context = tree.contexts[0].context;
  // A navigation sent while the browser is still on its own startup load can
  // be dropped without an answer; one retry covers it.
  if (url) {
    try { await send("browsingContext.navigate", { context, url, wait: "complete" }, 20000); }
    catch { await send("browsingContext.navigate", { context, url, wait: "complete" }, 30000); }
  }
  if (setCookie) {
    await send("script.evaluate", { expression: `document.cookie = ${JSON.stringify(setCookie + "; max-age=86400; path=/")}`, target: { context }, awaitPromise: false });
  }
  const r = await send("script.evaluate", { expression: PROBE, target: { context }, awaitPromise: true });
  if (r.type === "exception") throw new Error(JSON.stringify(r.exceptionDetails).slice(0, 400));
  console.log(JSON.stringify(JSON.parse(r.result.value), null, 1));
} catch (e) { console.error("ERR", e.message); process.exitCode = 1; }
// Firefox allows one BiDi session at a time: always hand it back.
if (sessionOpen) { try { await send("session.end", {}); } catch {} }
ws.close();
