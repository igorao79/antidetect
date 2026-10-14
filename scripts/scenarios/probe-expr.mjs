// The page-side script both probes evaluate: what a site can read about the device.
export const PROBE = `(async () => {
  const canvas = document.createElement("canvas");
  canvas.width = 240; canvas.height = 60;
  const ctx = canvas.getContext("2d");
  ctx.textBaseline = "top"; ctx.font = "16px Arial";
  ctx.fillStyle = "#f60"; ctx.fillRect(10, 5, 120, 30);
  ctx.fillStyle = "#069"; ctx.fillText("Antidetect probe \\u{1F600}", 4, 20);
  // FNV-1a over the rendered pixels: works on any page, secure context or not.
  let h = 0x811c9dc5;
  for (const ch of canvas.toDataURL()) { h ^= ch.charCodeAt(0); h = Math.imul(h, 0x01000193) >>> 0; }
  const canvasHash = h.toString(16).padStart(8, "0");
  const gl = document.createElement("canvas").getContext("webgl");
  const dbg = gl && gl.getExtension("WEBGL_debug_renderer_info");
  const webrtc = await new Promise((resolve) => {
    const ips = new Set();
    let pc;
    try { pc = new RTCPeerConnection({ iceServers: [{ urls: "stun:stun.l.google.com:19302" }] }); }
    catch (e) { return resolve(["unavailable"]); }
    pc.createDataChannel("x");
    pc.onicecandidate = (e) => {
      if (!e.candidate) return;
      const m = /([0-9]{1,3}(?:\\.[0-9]{1,3}){3}|[0-9a-f:]{6,})\\s+\\d+\\s+typ\\s+(\\w+)/i.exec(e.candidate.candidate);
      if (m) ips.add(m[2] + ":" + m[1]);
    };
    pc.createOffer().then(o => pc.setLocalDescription(o)).catch(() => {});
    setTimeout(() => { try { pc.close(); } catch {} resolve([...ips]); }, 2500);
  });
  return JSON.stringify({
    url: location.href,
    userAgent: navigator.userAgent,
    platform: navigator.platform,
    uaDataPlatform: navigator.userAgentData ? navigator.userAgentData.platform : undefined,
    languages: navigator.languages,
    timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
    locale: Intl.DateTimeFormat().resolvedOptions().locale,
    numberLocale: Intl.NumberFormat().resolvedOptions().locale,
    dateSample: new Date(Date.UTC(2026, 0, 15, 12)).toLocaleDateString(),
    formMessage: Object.assign(document.createElement("input"), { required: true }).validationMessage,
    screen: screen.width + "x" + screen.height,
    avail: screen.availWidth + "x" + screen.availHeight,
    screenInMedia: matchMedia("(device-width: " + screen.width + "px) and (device-height: " + screen.height + "px)").matches,
    cores: navigator.hardwareConcurrency,
    memory: navigator.deviceMemory,
    webdriver: navigator.webdriver,
    webglVendor: dbg ? gl.getParameter(dbg.UNMASKED_VENDOR_WEBGL) : null,
    webglRenderer: dbg ? gl.getParameter(dbg.UNMASKED_RENDERER_WEBGL) : null,
    canvasHash,
    webrtc,
    cookie: document.cookie,
    text: document.body ? document.body.innerText.trim().slice(0, 120) : "",
  });
})()`;
