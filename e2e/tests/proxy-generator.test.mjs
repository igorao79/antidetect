import assert from "node:assert/strict";
import { createHmac } from "node:crypto";
import { createServer } from "node:http";
import test from "node:test";
import { withApp } from "../lib/app.mjs";

const KEY = "pgk_e2e_0123456789";
const SECRET = "e2e-secret";
const PREFIX = "/proxygen";

const sortDeep = (value) =>
  value && typeof value === "object" && !Array.isArray(value)
    ? Object.fromEntries(
        Object.keys(value)
          .sort()
          .map((key) => [key, sortDeep(value[key])]),
      )
    : Array.isArray(value)
      ? value.map(sortDeep)
      : value;

const sign = (payload) =>
  createHmac("sha256", SECRET)
    .update(JSON.stringify(sortDeep(payload)))
    .digest("hex");

/**
 * A stand-in for the proxy generator: it checks every signature the way the
 * real service does (`proxygenerator/src/signing`), hands out ports and
 * records what it was asked.
 */
async function startGenerator() {
  const calls = [];
  let nextPort = 15000;
  const proxyFor = (body, index) => {
    const port = nextPort++;
    const baseName = body.name ?? null;
    return {
      id: `00000000-0000-4000-8000-${String(port).padStart(12, "0")}`,
      host: "127.0.0.1",
      port,
      username: body.countries
        ? `login__cr.${body.countries.join(",")}`
        : "login",
      password: "pw",
      name: baseName && body.count ? `${baseName}_${index + 1}` : baseName,
      note: body.note ?? null,
      expires_at: body.ttl_minutes
        ? new Date(Date.now() + body.ttl_minutes * 60_000).toISOString()
        : null,
    };
  };
  const server = createServer((req, res) => {
    let raw = "";
    req.on("data", (chunk) => {
      raw += chunk;
    });
    req.on("end", () => {
      const url = new URL(req.url, "http://localhost");
      const path = url.pathname.slice(PREFIX.length);
      const isGet = req.method === "GET";
      const query = Object.fromEntries(url.searchParams);
      const body = isGet ? {} : JSON.parse(raw || "{}");
      const reply = (status, payload) => {
        res.writeHead(status, { "Content-Type": "application/json" });
        res.end(JSON.stringify(payload));
      };
      const expected = sign({ v: 2, method: req.method, path, query, body });
      if (
        req.headers["x-store-key"] !== KEY ||
        req.headers["x-signature"] !== expected
      ) {
        reply(401, {
          ok: false,
          error: { code: "INVALID_SIGNATURE", message: "Invalid signature" },
        });
        return;
      }
      const { _timestamp, ...fields } = isGet ? query : body;
      calls.push({ method: req.method, path, fields });
      if (path === "/api/v1/options") {
        reply(200, {
          ok: true,
          result: {
            enabled: true,
            countries: ["us", "de", "ru"],
            popular_countries: ["us", "de"],
            protocols: ["http", "socks5"],
            ttl_hours: [1, 24, 168],
            ttl_minutes_min: 5,
            ttl_minutes_max: 43200,
            bulk_max: 20,
            owned_max: 100,
          },
        });
      } else if (path === "/api/v1/proxies" && req.method === "POST") {
        reply(201, { ok: true, result: proxyFor(fields, 0) });
      } else if (path === "/api/v1/proxies/bulk") {
        reply(201, {
          ok: true,
          result: {
            items: Array.from({ length: fields.count }, (_, i) =>
              proxyFor(fields, i),
            ),
          },
        });
      } else if (req.method === "DELETE") {
        reply(200, { ok: true, result: { deleted: true } });
      } else {
        reply(404, { ok: false, error: { code: "NOT_FOUND", message: "?" } });
      }
    });
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address();
  return { url: `http://127.0.0.1:${port}${PREFIX}`, calls, server };
}

test("generated proxies arrive with their note and end, and erasing one frees its port", async () => {
  const generator = await startGenerator();
  try {
    await withApp(
      "entities-proxy-generator",
      async (app) => {
        assert.deepEqual(await app.invoke("proxy_generator_status"), {
          configured: false,
          key_hint: null,
        });
        assert.match(
          await app.invokeError("proxy_generator_options"),
          /PROXY_GENERATOR_NO_KEY/,
        );
        assert.match(
          await app.invokeError("proxy_generator_set_credentials", {
            key: KEY,
            secret: "wrong",
          }),
          /PROXY_GENERATOR_KEY_INVALID/,
        );
        assert.equal(
          (await app.invoke("proxy_generator_status")).configured,
          false,
          "a refused pair is not kept",
        );
        assert.deepEqual(
          await app.invoke("proxy_generator_set_credentials", {
            key: ` ${KEY} `,
            secret: SECRET,
          }),
          { configured: true, key_hint: "pgk_…6789" },
        );
        assert.equal(
          (await app.invoke("proxy_generator_options")).bulk_max,
          20,
        );

        const batch = await app.invoke("proxy_generator_generate", {
          request: {
            count: 3,
            countriesMode: "only",
            countries: ["us"],
            ttlMinutes: 90,
            name: "kraken",
            note: "reg",
            protocol: "http",
          },
        });
        assert.deepEqual(generator.calls.at(-1), {
          method: "POST",
          path: "/api/v1/proxies/bulk",
          fields: {
            count: 3,
            countries_mode: "only",
            countries: ["us"],
            ttl_minutes: 90,
            name: "kraken",
            note: "reg",
          },
        });
        assert.deepEqual(
          batch.map((p) => p.name),
          ["kraken_1", "kraken_2", "kraken_3"],
        );
        const now = Math.floor(Date.now() / 1000);
        for (const proxy of batch) {
          assert.equal(proxy.note, "reg");
          assert.equal(proxy.proxy_settings.proxy_type, "http");
          assert.equal(proxy.proxy_settings.username, "login__cr.us");
          assert.ok(proxy.generator_id);
          assert.ok(Math.abs(proxy.expires_at - (now + 90 * 60)) < 120);
        }

        const [single] = await app.invoke("proxy_generator_generate", {
          request: { count: 1, name: "kraken_1", protocol: "socks5" },
        });
        assert.equal(single.name, "kraken_1 (2)", "a taken name gets a number");
        assert.equal(single.proxy_settings.proxy_type, "socks5");
        assert.equal(single.expires_at ?? null, null);
        assert.deepEqual(generator.calls.at(-1).fields, { name: "kraken_1" });

        // The trash can still bring a proxy back, so its port stays taken.
        const [first] = batch;
        await app.invoke("trash_entity", { kind: "proxy", id: first.id });
        await new Promise((resolve) => setTimeout(resolve, 500));
        assert.equal(
          generator.calls.some((c) => c.method === "DELETE"),
          false,
        );
        await app.invoke("purge_trashed_entity", {
          kind: "proxy",
          id: first.id,
        });
        await app.waitFor(
          () =>
            generator.calls.some(
              (c) =>
                c.method === "DELETE" &&
                c.path === `/api/v1/proxies/${first.generator_id}`,
            ),
          { description: "the erased proxy released in the generator" },
        );

        const noted = await app.invoke("create_stored_proxy", {
          name: "Hand made",
          proxySettings: {
            proxy_type: "socks5",
            host: "127.0.0.1",
            port: 1080,
          },
          note: "  manual  ",
        });
        assert.equal(noted.note, "manual");
        const cleared = await app.invoke("update_stored_proxy", {
          proxyId: noted.id,
          note: "",
        });
        assert.equal(cleared.note ?? null, null);

        await app.clickSelector('[aria-label="Network"]');
        await app.waitForText("kraken_2");
        await app.waitForText("reg");
        await app.clickSelector('[aria-label="Generate"]');
        await app.waitForText("Exit countries");
        await app.waitForText("Change key");
        await app.pressShortcut({ key: "Escape" });

        await app.invoke("proxy_generator_clear_credentials");
        assert.equal(
          (await app.invoke("proxy_generator_status")).configured,
          false,
        );
        await app.clickSelector('[aria-label="Generate"]');
        await app.waitForText("Save key");
      },
      { extraEnv: { OWL_PROXYGEN_URL: generator.url } },
    );
  } finally {
    generator.server.close();
  }
});
