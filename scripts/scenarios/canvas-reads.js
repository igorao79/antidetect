(async () => {
  const fnv = (s) => {
    let h = 0x811c9dc5;
    for (let i = 0; i < s.length; i++) {
      h ^= s.charCodeAt(i);
      h = Math.imul(h, 0x01000193) >>> 0;
    }
    return h.toString(16).padStart(8, "0");
  };
  const bytes = (u8) => {
    let h = 0x811c9dc5;
    for (let i = 0; i < u8.length; i++) {
      h ^= u8[i];
      h = Math.imul(h, 0x01000193) >>> 0;
    }
    return h.toString(16).padStart(8, "0");
  };
  const draw = (c) => {
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
    return x;
  };
  const a = document.createElement("canvas");
  a.width = 280;
  a.height = 60;
  draw(a);
  const fresh1 = fnv(a.toDataURL());
  const b = document.createElement("canvas");
  b.width = 280;
  b.height = 60;
  const bx = draw(b);
  const img1 = bytes(bx.getImageData(0, 0, 280, 60).data);
  const img2 = bytes(bx.getImageData(0, 0, 280, 60).data);
  const afterRead = fnv(b.toDataURL());
  const jpeg = fnv(a.toDataURL("image/jpeg", 0.9));
  const webp = fnv(a.toDataURL("image/webp", 0.9));
  const blob = await new Promise((r) => a.toBlob(r));
  const blobHash = bytes(new Uint8Array(await blob.arrayBuffer()));
  let off = null;
  try {
    const o = new OffscreenCanvas(280, 60);
    draw(o);
    off = bytes(new Uint8Array(await (await o.convertToBlob()).arrayBuffer()));
  } catch (e) {
    off = "err";
  }
  // decode the PNG back and hash pixels, to see if the encoded image itself is noised
  const im = new Image();
  im.src = a.toDataURL();
  await im.decode();
  const c2 = document.createElement("canvas");
  c2.width = 280;
  c2.height = 60;
  const x2 = c2.getContext("2d");
  x2.drawImage(im, 0, 0);
  return JSON.stringify({
    origin: location.origin,
    toDataURL: fresh1,
    getImageData: [img1, img2],
    toDataURLAfterRead: afterRead,
    jpeg,
    webp,
    toBlob: blobHash,
    offscreen: off,
  });
})();
