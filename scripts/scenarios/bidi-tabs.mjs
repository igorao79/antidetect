// usage: node bidi-tabs.mjs <ws_endpoint> [--mark <url part> | --open <url>]
// Lists the open tabs (url and sessionStorage.owl); --mark sets sessionStorage.owl
// in every tab whose url contains <url part>, from a script of the page; --open
// opens <url> in a new background tab first. Every tab is brought to the front
// once, so tabs a restored session loads on demand show their address.
const [endpoint, flag, part] = process.argv.slice(2);
const ws = new WebSocket(endpoint);
let id = 0; const pending = new Map();
// A command a page never answers (one still loading) gives up, so the session
// is always ended below.
const send = (method, params = {}) => new Promise((resolve, reject) => {
  const mid = ++id; pending.set(mid, { resolve, reject });
  ws.send(JSON.stringify({ id: mid, method, params }));
  setTimeout(() => pending.delete(mid) && reject(new Error("timed out: " + method)), 10000);
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
  // Firefox's own pages (a new tab is chrome://browser/content/blanktab.html)
  // cannot be activated over BiDi.
  const activate = (context) => send("browsingContext.activate", { context }).catch(() => {});
  for (const { context } of contexts) await activate(context);
  if (contexts.length) await activate(contexts[0].context);
  const tabs = [];
  for (const { context, url } of (await send("browsingContext.getTree", {})).contexts) {
    // Written by a script of the page itself: Firefox's session store records
    // what a page writes, not a value set from outside over BiDi.
    const mark = flag === "--mark" && url.includes(part)
      ? `{ const s = document.createElement("script"); s.textContent = 'sessionStorage.owl = "1"'; document.head.append(s); }`
      : "";
    const r = await send("script.evaluate", { expression: `${mark} sessionStorage.owl ?? null`, target: { context }, awaitPromise: false })
      .catch(() => null);
    tabs.push({ url, owl: r?.result?.value ?? null });
  }
  console.log(JSON.stringify(tabs));
} catch (e) { console.error("ERR", e.message); process.exitCode = 1; }
// Firefox keeps a session after its connection drops, and allows one: a
// session left open would keep the app from asking the browser to quit.
try { await send("session.end", {}); } catch {}
ws.close();
