// usage: node bidi-eval.mjs <ws endpoint> <url> <expr file> [wait ms]
import fs from "node:fs";
const [endpoint, url, file, wait] = process.argv.slice(2);
const ws = new WebSocket(endpoint);
let id = 0; const pending = new Map();
const send = (method, params = {}, timeout = 60000) => new Promise((res, rej) => { const i = ++id; pending.set(i, { res, rej }); ws.send(JSON.stringify({ id: i, method, params })); setTimeout(() => { if (pending.delete(i)) rej(new Error("timed out: " + method)); }, timeout); });
ws.onmessage = (ev) => { const m = JSON.parse(ev.data); const p = pending.get(m.id); if (!p) return; pending.delete(m.id); m.type === "error" ? p.rej(new Error(m.error + ": " + m.message)) : p.res(m.result); };
await new Promise((r) => (ws.onopen = r));
let open = false;
try {
  await send("session.new", { capabilities: {} }); open = true;
  const context = (await send("browsingContext.getTree", {})).contexts[0].context;
  // A navigation sent while the browser is still on its own startup load can
  // be dropped without an answer; one retry covers it.
  try { await send("browsingContext.navigate", { context, url, wait: "complete" }, 20000); }
  catch { await send("browsingContext.navigate", { context, url, wait: "complete" }, 30000); }
  await new Promise((r) => setTimeout(r, Number(wait || 6000)));
  const r = await send("script.evaluate", { expression: fs.readFileSync(file, "utf8"), target: { context }, awaitPromise: true });
  console.log(r.type === "exception" ? "EXC " + JSON.stringify(r.exceptionDetails).slice(0, 300) : r.result.value);
} catch (e) { console.error("ERR", e.message); }
if (open) { try { await send("session.end", {}); } catch {} }
ws.close();
