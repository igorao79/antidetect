// usage: node search-engine.mjs <ws_endpoint>
// Prints the address bar's default search engine, where it sends a search, and
// the engines listed. Reads the browser's parent process, which Owl launches
// Camoufox with `--remote-allow-system-access` for.
const [endpoint] = process.argv.slice(2);
// The browser opens its BiDi port a moment after the start call returns.
let ws;
for (const end = Date.now() + 30000; ; ) {
  try {
    ws = new WebSocket(endpoint);
    await new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = reject; });
    break;
  } catch {
    if (Date.now() > end) { console.log(JSON.stringify({ error: "no BiDi connection" })); process.exit(1); }
    await new Promise((r) => setTimeout(r, 250));
  }
}
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
try {
  await send("session.new", { capabilities: {} });
  const { context } = (await send("browsingContext.getTree", { "moz:scope": "chrome", maxDepth: 0 })).contexts[0];
  const expression = `(async () => {
    const { SearchService } = ChromeUtils.importESModule("moz-src:///toolkit/components/search/SearchService.sys.mjs");
    await SearchService.init();
    const engine = await SearchService.getDefault();
    return JSON.stringify({
      name: engine.name,
      url: engine.getSubmission("owl search").uri.spec,
      engines: (await SearchService.getVisibleEngines()).map((e) => e.name),
    });
  })()`;
  const r = await send("script.evaluate", { expression, target: { context }, awaitPromise: true });
  console.log(r.type === "exception" ? JSON.stringify({ error: r.exceptionDetails.text }) : r.result.value);
  await send("session.end", {});
} catch (e) { console.log(JSON.stringify({ error: e.message })); process.exitCode = 1; }
ws.close();
