// Every surface a site hashes to tell devices apart, read once.
// Evaluated in a page; returns JSON of short hashes and plain values.
(async () => {
  const fnv = (input) => {
    let h = 0x811c9dc5;
    for (let i = 0; i < input.length; i++) {
      h ^= typeof input === "string" ? input.charCodeAt(i) : input[i];
      h = Math.imul(h, 0x01000193) >>> 0;
    }
    return h.toString(16).padStart(8, "0");
  };
  const out = {};
  const draw = () => {
    const c = document.createElement("canvas");
    c.width = 280;
    c.height = 60;
    const x = c.getContext("2d");
    x.fillStyle = "#f60";
    x.fillRect(125, 1, 62, 20);
    x.fillStyle = "#069";
    x.font = "18pt Arial";
    x.fillText("Cwm fjordbank glyphs vext quiz", 4, 45);
    const g = x.createLinearGradient(0, 0, 280, 0);
    g.addColorStop(0, "rgba(255,0,0,.5)");
    g.addColorStop(1, "rgba(0,0,255,.5)");
    x.fillStyle = g;
    x.fillRect(0, 30, 280, 30);
    return [c, x];
  };
  {
    const [c, x] = draw();
    out.canvasToDataURL = fnv(c.toDataURL());
    out.canvasToDataURLAgain = fnv(draw()[0].toDataURL());
    out.canvasGetImageData = fnv(x.getImageData(0, 0, 280, 60).data);
    const blob = await new Promise((r) => c.toBlob(r));
    out.canvasToBlob = fnv(new Uint8Array(await blob.arrayBuffer()));
  }
  try {
    const c = document.createElement("canvas");
    c.width = 64;
    c.height = 64;
    const gl = c.getContext("webgl", { preserveDrawingBuffer: true });
    const vs = gl.createShader(gl.VERTEX_SHADER);
    gl.shaderSource(
      vs,
      "attribute vec2 p;varying vec2 v;void main(){v=p;gl_Position=vec4(p,0,1);}",
    );
    gl.compileShader(vs);
    const fs = gl.createShader(gl.FRAGMENT_SHADER);
    gl.shaderSource(
      fs,
      "precision mediump float;varying vec2 v;void main(){gl_FragColor=vec4(sin(v.x*9.0)*.5+.5,cos(v.y*7.0)*.5+.5,v.x*v.y*.5+.5,1);}",
    );
    gl.compileShader(fs);
    const prog = gl.createProgram();
    gl.attachShader(prog, vs);
    gl.attachShader(prog, fs);
    gl.linkProgram(prog);
    // biome-ignore lint/correctness/useHookAtTopLevel: WebGL's useProgram, not a React hook
    gl.useProgram(prog);
    gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
    gl.bufferData(
      gl.ARRAY_BUFFER,
      new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]),
      gl.STATIC_DRAW,
    );
    const loc = gl.getAttribLocation(prog, "p");
    gl.enableVertexAttribArray(loc);
    gl.vertexAttribPointer(loc, 2, gl.FLOAT, false, 0, 0);
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
    const px = new Uint8Array(64 * 64 * 4);
    gl.readPixels(0, 0, 64, 64, gl.RGBA, gl.UNSIGNED_BYTE, px);
    out.webglReadPixels = fnv(px);
    out.webglToDataURL = fnv(c.toDataURL());
    const dbg = gl.getExtension("WEBGL_debug_renderer_info");
    out.webglVendor = dbg ? gl.getParameter(dbg.UNMASKED_VENDOR_WEBGL) : null;
    out.webglRenderer = dbg
      ? gl.getParameter(dbg.UNMASKED_RENDERER_WEBGL)
      : null;
  } catch (e) {
    out.webglError = String(e);
  }
  try {
    const ctx = new OfflineAudioContext(1, 44100, 44100);
    const osc = ctx.createOscillator();
    osc.type = "triangle";
    osc.frequency.value = 10000;
    const comp = ctx.createDynamicsCompressor();
    comp.threshold.value = -50;
    comp.knee.value = 40;
    comp.ratio.value = 12;
    comp.attack.value = 0;
    comp.release.value = 0.25;
    osc.connect(comp);
    comp.connect(ctx.destination);
    osc.start(0);
    const data = (await ctx.startRendering()).getChannelData(0);
    let sum = 0;
    for (let i = 4500; i < 5000; i++) sum += Math.abs(data[i]);
    out.audioSum = sum.toFixed(10);
    out.audioSamples = fnv(new Uint8Array(data.buffer, 4500 * 4, 2000));
  } catch (e) {
    out.audioError = String(e);
  }
  {
    const span = document.createElement("span");
    span.style.cssText =
      "position:absolute;left:-9999px;font:17.3px Arial;transform:rotate(12.7deg) scale(1.013)";
    span.textContent = "Cwm fjordbank glyphs vext quiz 0123456789";
    document.body.appendChild(span);
    const r = span.getBoundingClientRect();
    out.clientRects = [r.x, r.y, r.width, r.height]
      .map((n) => n.toFixed(6))
      .join(",");
    span.remove();
    const m = document.createElement("canvas").getContext("2d");
    m.font = "17.3px Arial";
    out.textWidth = m
      .measureText("Cwm fjordbank glyphs vext quiz")
      .width.toFixed(6);
  }
  out.userAgent = navigator.userAgent;
  out.platform = navigator.platform;
  out.cores = navigator.hardwareConcurrency;
  out.memory = navigator.deviceMemory;
  out.screen = `${screen.width}x${screen.height}`;
  out.webdriver = navigator.webdriver;
  if (navigator.userAgentData) {
    const high = await navigator.userAgentData.getHighEntropyValues([
      "fullVersionList",
      "platformVersion",
      "architecture",
    ]);
    out.uaPlatform = high.platform;
    out.uaFullVersion = high.fullVersionList.find((b) =>
      /Chrom/.test(b.brand),
    )?.version;
  }
  return JSON.stringify(out);
})();
