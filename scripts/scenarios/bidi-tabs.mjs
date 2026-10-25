// usage: node bidi-tabs.mjs <ws_endpoint> [--mark <url part> | --open <url>]
// Lists the open tabs (url and sessionStorage.owl); --mark sets sessionStorage.owl
// in every tab whose url contains <url part>, from a script of the page; --open
// opens <url> in a new background tab first. Every tab is brought to the front
// once, so tabs a restored session loads on demand show their address.
const [endpoint, flag, part] = process.argv.slice(2);
const ws = new WebSocket(endpoint);
let id = 0; const pending = new Map();
const send = (method, params = {}) => new Promise((resolve, reject) => {
  const mid = ++id; pending.set(mid, { resolve, reject });
  ws.send(JSON.stringify({ id: mid, method, params }));
});
ws.onmessage = (ev) => {
  const msg = JSON.parse(ev.data);
  const p = pending.get(msg.id); if (!p) return; pending.delete(msg.id);
  msg.type === "error" ? p.reject(new Error(msg.error + ": " + msg.message)) : p.resolve(msg.result);
};
await new Promise((r) => (ws.onopen = r));
try {
  await send("session.new", { capabilities: {} });
  if (flag === "--open") {
    const { context } = await send("browsingContext.create", { type: "tab", background: true });
    await send("browsingContext.navigate", { context, url: part, wait: "none" });
  }
  const contexts = (await send("browsingContext.getTree", {})).contexts;
  for (const { context } of contexts) await send("browsingContext.activate", { context });
  if (contexts.length) await send("browsingContext.activate", { context: contexts[0].context });
  const tabs = [];
  for (const { context, url } of (await send("browsingContext.getTree", {})).contexts) {
    // Written by a script of the page itself: Firefox's session store records
    // what a page writes, not a value set from outside over BiDi.
    const mark = flag === "--mark" && url.includes(part)
      ? `{ const s = document.createElement("script"); s.textContent = 'sessionStorage.owl = "1"'; document.head.append(s); }`
      : "";
    const r = await send("script.evaluate", { expression: `${mark} sessionStorage.owl ?? null`, target: { context }, awaitPromise: false });
    tabs.push({ url, owl: r.result?.value ?? null });
  }
  console.log(JSON.stringify(tabs));
  await send("session.end", {});
} catch (e) { console.error("ERR", e.message); process.exitCode = 1; }
ws.close();
