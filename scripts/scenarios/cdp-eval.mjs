// usage: node cdp-eval.mjs <debug_port> <file with a JS expression returning a JSON string>
import fs from "node:fs";
const [port, file] = process.argv.slice(2);
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const page = targets.find((t) => t.type === "page");
const ws = new WebSocket(page.webSocketDebuggerUrl);
let id = 0; const pending = new Map();
const send = (method, params = {}) => new Promise((res, rej) => { const i = ++id; pending.set(i, { res, rej }); ws.send(JSON.stringify({ id: i, method, params })); });
ws.onmessage = (ev) => { const m = JSON.parse(ev.data); const p = pending.get(m.id); if (!p) return; pending.delete(m.id); m.error ? p.rej(new Error(m.error.message)) : p.res(m.result); };
await new Promise((r) => (ws.onopen = r));
try {
  const r = await send("Runtime.evaluate", { expression: fs.readFileSync(file, "utf8"), awaitPromise: true, returnByValue: true });
  console.log(r.exceptionDetails ? "EXC " + JSON.stringify(r.exceptionDetails).slice(0, 300) : r.result.value);
} catch (e) { console.error("ERR", e.message); }
ws.close();
