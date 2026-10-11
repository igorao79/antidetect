// Drive or read the synchronizer test page in a running Chromium profile.
// usage: node sync-drive.mjs act <debug port> <url>   — navigate, click, type, scroll like a user
//        node sync-drive.mjs read <debug port>        — print the page's state
const [mode, port, url] = process.argv.slice(2);
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const ws = new WebSocket(targets.find((t) => t.type === "page").webSocketDebuggerUrl);
let id = 0; const pending = new Map();
const send = (method, params = {}) => new Promise((res, rej) => { const i = ++id; pending.set(i, { res, rej }); ws.send(JSON.stringify({ id: i, method, params })); });
ws.onmessage = (ev) => { const m = JSON.parse(ev.data); const p = pending.get(m.id); if (!p) return; pending.delete(m.id); m.error ? p.rej(new Error(m.error.message)) : p.res(m.result); };
await new Promise((r) => (ws.onopen = r));
const pause = (ms) => new Promise((r) => setTimeout(r, ms));
const click = async (x, y) => {
  await send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", clickCount: 1 });
  await send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", clickCount: 1 });
  await pause(250);
};
try {
  if (mode === "act") {
    await send("Page.navigate", { url });
    await pause(6000);
    await click(140, 70); await click(140, 70);
    await click(190, 160);
    for (const ch of "hi") {
      const code = "Key" + ch.toUpperCase(), vk = ch.toUpperCase().charCodeAt(0);
      await send("Input.dispatchKeyEvent", { type: "keyDown", key: ch, code, windowsVirtualKeyCode: vk, text: ch });
      await send("Input.dispatchKeyEvent", { type: "keyUp", key: ch, code, windowsVirtualKeyCode: vk });
      await pause(150);
    }
    await send("Input.dispatchMouseEvent", { type: "mouseWheel", x: 500, y: 300, deltaX: 0, deltaY: 400 });
    await pause(2500);
  }
  const r = await send("Runtime.evaluate", { expression: "JSON.stringify(typeof window.state === 'function' ? window.state() : { url: location.href })", returnByValue: true });
  console.log(r.result.value);
} finally { ws.close(); }
