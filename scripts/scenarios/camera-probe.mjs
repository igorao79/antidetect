// What a page granted camera access gets from a profile: the track, a frame and the device list.
// usage: node camera-probe.mjs <debug port>
const port = process.argv[2];
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const ws = new WebSocket(targets.find((t) => t.type === "page").webSocketDebuggerUrl);
let id = 0; const pending = new Map();
const send = (method, params = {}) => new Promise((res, rej) => { const i = ++id; pending.set(i, { res, rej }); ws.send(JSON.stringify({ id: i, method, params })); });
ws.onmessage = (ev) => { const m = JSON.parse(ev.data); const p = pending.get(m.id); if (!p) return; pending.delete(m.id); m.error ? p.rej(new Error(m.error.message)) : p.res(m.result); };
await new Promise((r) => (ws.onopen = r));
try {
  await send("Browser.grantPermissions", { origin: "https://example.com", permissions: ["videoCapture", "audioCapture"] });
  await send("Page.enable"); await send("Page.navigate", { url: "https://example.com" });
  await new Promise((r) => setTimeout(r, 5000));
  const expression = `(async () => {
    const out = {};
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ video: true });
      const track = stream.getVideoTracks()[0];
      out.label = track.label; out.width = track.getSettings().width; out.height = track.getSettings().height;
      const video = document.createElement("video"); video.srcObject = stream; video.muted = true; await video.play();
      const frame = async () => {
        const c = document.createElement("canvas"); c.width = 64; c.height = 36;
        const x = c.getContext("2d"); x.drawImage(video, 0, 0, 64, 36);
        const d = x.getImageData(0, 0, 64, 36).data; let sum = 0, h = 0;
        for (let i = 0; i < d.length; i += 4) { sum += d[i] + d[i + 1] + d[i + 2]; h = (h * 31 + d[i]) >>> 0; }
        return { brightness: Math.round(sum / (64 * 36 * 3)), hash: h };
      };
      await new Promise((r) => setTimeout(r, 700)); const first = await frame();
      await new Promise((r) => setTimeout(r, 900)); const second = await frame();
      out.brightness = first.brightness; out.moving = first.hash !== second.hash;
      stream.getTracks().forEach((t) => t.stop());
    } catch (e) { out.error = String(e); }
    out.devices = (await navigator.mediaDevices.enumerateDevices()).map((d) => d.kind + ":" + d.label);
    return JSON.stringify(out);
  })()`;
  const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  console.log(r.result.value ?? JSON.stringify(r));
} finally { ws.close(); }
