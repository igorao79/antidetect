// A minimal HTTP CONNECT proxy that logs what goes through it.
// usage: node test-proxy.mjs <port> <logfile> [upstream]
// With an upstream (http://user:pass@host:port) every tunnel is chained through
// that proxy, so the exit is the upstream's and not this machine's.
import http from "node:http"; import net from "node:net"; import fs from "node:fs";
const [port, logfile, upstreamUrl] = process.argv.slice(2);
const upstreamProxy = upstreamUrl ? new URL(upstreamUrl) : null;
const log = (line) => fs.appendFileSync(logfile, line + "\n");

// Open a socket to host:port, directly or through the upstream proxy.
function dial(host, p, ready, failed) {
  if (!upstreamProxy) {
    const socket = net.connect(p, host, () => ready(socket));
    socket.on("error", failed);
    return;
  }
  const socket = net.connect(Number(upstreamProxy.port) || 80, upstreamProxy.hostname, () => {
    const auth = upstreamProxy.username
      ? "Proxy-Authorization: Basic " + Buffer.from(decodeURIComponent(upstreamProxy.username) + ":" + decodeURIComponent(upstreamProxy.password)).toString("base64") + "\r\n"
      : "";
    socket.write(`CONNECT ${host}:${p} HTTP/1.1\r\nHost: ${host}:${p}\r\n${auth}\r\n`);
    let seen = Buffer.alloc(0);
    const onData = (chunk) => {
      seen = Buffer.concat([seen, chunk]);
      const end = seen.indexOf("\r\n\r\n");
      if (end < 0) return;
      socket.off("data", onData);
      if (!/^HTTP\/1\.[01] 200/.test(seen.toString("latin1"))) { socket.destroy(); return failed(new Error("upstream refused")); }
      const rest = seen.subarray(end + 4);
      if (rest.length) socket.unshift(rest);
      ready(socket);
    };
    socket.on("data", onData);
  });
  socket.on("error", failed);
}

const server = http.createServer((req, res) => {
  log("HTTP " + req.url);
  const u = new URL(req.url);
  const up = http.request({ host: u.hostname, port: u.port || 80, path: u.pathname + u.search, method: req.method, headers: req.headers }, (r) => { res.writeHead(r.statusCode, r.headers); r.pipe(res); });
  up.on("error", () => res.destroy()); req.pipe(up);
});
server.on("connect", (req, client, head) => {
  log("CONNECT " + req.url);
  const [host, p] = req.url.split(":");
  client.on("error", () => {});
  dial(host, Number(p) || 443, (upstream) => {
    client.write("HTTP/1.1 200 Connection Established\r\n\r\n");
    upstream.write(head); upstream.pipe(client); client.pipe(upstream);
    client.on("close", () => upstream.destroy()); upstream.on("close", () => client.destroy());
  }, () => client.destroy());
});
server.listen(Number(port), "127.0.0.1", () => log("LISTENING " + port + (upstreamProxy ? " via " + upstreamProxy.host : "")));
