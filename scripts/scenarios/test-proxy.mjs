// A minimal HTTP CONNECT proxy that logs what goes through it.
// usage: node test-proxy.mjs <port> <logfile>
import http from "node:http"; import net from "node:net"; import fs from "node:fs";
const [port, logfile] = process.argv.slice(2);
const log = (line) => fs.appendFileSync(logfile, line + "\n");
const server = http.createServer((req, res) => {
  log("HTTP " + req.url);
  const u = new URL(req.url);
  const up = http.request({ host: u.hostname, port: u.port || 80, path: u.pathname + u.search, method: req.method, headers: req.headers }, (r) => { res.writeHead(r.statusCode, r.headers); r.pipe(res); });
  up.on("error", () => res.destroy()); req.pipe(up);
});
server.on("connect", (req, client, head) => {
  log("CONNECT " + req.url);
  const [host, p] = req.url.split(":");
  const upstream = net.connect(Number(p) || 443, host, () => {
    client.write("HTTP/1.1 200 Connection Established\r\n\r\n");
    upstream.write(head); upstream.pipe(client); client.pipe(upstream);
  });
  upstream.on("error", () => client.destroy()); client.on("error", () => upstream.destroy());
});
server.listen(Number(port), "127.0.0.1", () => log("LISTENING " + port));
