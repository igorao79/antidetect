#!/usr/bin/env node
// One Cloudflare signup: mailbox from mailhub, proxy from proxygen (or a stored
// one), a Camoufox profile in Owl, then the form over WebDriver BiDi.
//
//   node scripts/cloudflare-signup/signup.mjs --proxy-uuid <uuid>
//   node scripts/cloudflare-signup/signup.mjs --proxygen-key pgk_… --proxygen-secret … --country de
//
// Env: MAILHUB_KEY, MAILHUB_SECRET, MAILHUB_BASE, PROXYGEN_KEY, PROXYGEN_SECRET,
// PROXYGEN_BASE, OWL_BASE. Every value has a flag too; the flag wins.
//
// The Turnstile checkbox is clicked at a fixed offset inside the widget box
// (found between the password field and the submit button), because the widget
// lives in a closed shadow root the page cannot see. `--manual` keeps the
// browser open for the operator to click it instead.

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const OWL_BASE = (process.env.OWL_BASE || "http://127.0.0.1:58888").replace(
  /\/$/,
  "",
);
const MAILHUB_BASE = (
  process.env.MAILHUB_BASE || "https://telemanager.online/mailhub"
).replace(/\/$/, "");
const PROXYGEN_BASE = (
  process.env.PROXYGEN_BASE || "https://telemanager.online/proxygen"
).replace(/\/$/, "");
const SIGNUP_URL = "https://dash.cloudflare.com/sign-up";
const VIEWPORT = { width: 1280, height: 1000 };

// ---------------------------------------------------------------- arguments

function parseArgs(argv) {
  const out = { positional: [] };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (!arg.startsWith("--")) {
      out.positional.push(arg);
      continue;
    }
    const [name, inline] = arg.slice(2).split("=");
    const key = name.replace(/-([a-z])/g, (_, c) => c.toUpperCase());
    if (inline !== undefined) out[key] = inline;
    else if (argv[i + 1] && !argv[i + 1].startsWith("--")) out[key] = argv[++i];
    else out[key] = true;
  }
  return out;
}

const args = parseArgs(process.argv.slice(2));
const option = (name, fallback) => {
  const value = args[name];
  return value === undefined || value === true ? fallback : value;
};

const config = {
  mailhubKey: option("mailhubKey", process.env.MAILHUB_KEY),
  mailhubSecret: option("mailhubSecret", process.env.MAILHUB_SECRET),
  proxygenKey: option("proxygenKey", process.env.PROXYGEN_KEY),
  proxygenSecret: option("proxygenSecret", process.env.PROXYGEN_SECRET),
  domain: option("domain"),
  email: option("email"),
  password: option("password"),
  proxyUuid: option("proxyUuid"),
  proxy: option("proxy"),
  country: option("country"),
  ttlMinutes: option("ttlMinutes") ? Number(option("ttlMinutes")) : undefined,
  browserOs: option("os", "mac"),
  headed: Boolean(args.headed),
  manual: Boolean(args.manual),
  keep: Boolean(args.keep),
  noToken: Boolean(args.noToken),
  tokenOnly: Boolean(args.tokenOnly),
  turnstileTimeoutMs: Number(option("turnstileTimeout", "120")) * 1000,
  manualTimeoutMs: Number(option("manualTimeout", "300")) * 1000,
  emailTimeoutMs: Number(option("emailTimeout", "180")) * 1000,
  keyRounds: Number(option("keyRounds", "3")),
  keyWaitMs: Number(option("keyWait", "60")) * 1000,
  reportPath: option("report"),
};

// ---------------------------------------------------------------- reporting

const HERE = path.dirname(fileURLToPath(import.meta.url));
const startedAt = new Date();
const stamp = startedAt.toISOString().replace(/[:.]/g, "-").slice(0, 19);
const workDir = option("runDir", path.join(HERE, "reports", stamp));
fs.mkdirSync(workDir, { recursive: true });
const report = {
  startedAt: startedAt.toISOString(),
  email: null,
  password: null,
  proxy: null,
  profile: null,
  steps: {},
  outcome: null,
  error: null,
  screenshots: [],
};

function say(message) {
  console.log(`[${new Date().toISOString().slice(11, 19)}] ${message}`);
}

function saveReport() {
  report.finishedAt = new Date().toISOString();
  const file = config.reportPath || path.join(workDir, "report.json");
  fs.writeFileSync(file, `${JSON.stringify(report, null, 2)}\n`);
  return file;
}

async function timed(step, fn) {
  const at = Date.now();
  try {
    return await fn();
  } finally {
    report.steps[step] = (report.steps[step] || 0) + (Date.now() - at);
  }
}

// ---------------------------------------------------------------- signed APIs
// v2: hex HMAC-SHA256 over the canonical JSON of {v, method, path, query, body}
// with keys sorted at every level; `_timestamp` in ms in a GET query or the
// non-GET body. Both telemanager services share the scheme.

function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value && typeof value === "object") {
    const keys = Object.keys(value).sort();
    return `{${keys.map((k) => `${JSON.stringify(k)}:${canonical(value[k])}`).join(",")}}`;
  }
  return JSON.stringify(value);
}

async function signed(
  base,
  method,
  pathName,
  key,
  secret,
  { query, body } = {},
) {
  const isGet = method === "GET";
  const q = isGet
    ? { ...(query || {}), _timestamp: String(Date.now()) }
    : { ...(query || {}) };
  const b = isGet ? {} : { ...(body || {}), _timestamp: Date.now() };
  const signature = crypto
    .createHmac("sha256", secret)
    .update(canonical({ v: 2, method, path: pathName, query: q, body: b }))
    .digest("hex");
  const qs = isGet && Object.keys(q).length ? `?${new URLSearchParams(q)}` : "";
  const response = await fetch(`${base}${pathName}${qs}`, {
    method,
    headers: {
      "Content-Type": "application/json",
      "X-Store-Key": key,
      "X-Signature": signature,
    },
    body: isGet ? undefined : JSON.stringify(b),
  });
  const text = await response.text();
  let envelope;
  try {
    envelope = JSON.parse(text);
  } catch {
    throw new Error(
      `${method} ${pathName}: HTTP ${response.status} ${text.slice(0, 200)}`,
    );
  }
  if (!envelope.ok) {
    const error = envelope.error || {};
    throw new Error(
      `${method} ${pathName}: ${error.code || response.status} ${error.message || ""}`.trim(),
    );
  }
  return envelope.result;
}

const mailhub = (method, pathName, options) =>
  signed(
    MAILHUB_BASE,
    method,
    pathName,
    config.mailhubKey,
    config.mailhubSecret,
    options,
  );
const proxygen = (method, pathName, options) =>
  signed(
    PROXYGEN_BASE,
    method,
    pathName,
    config.proxygenKey,
    config.proxygenSecret,
    options,
  );

// ---------------------------------------------------------------- Owl API

async function owl(method, pathName, body) {
  const response = await fetch(`${OWL_BASE}${pathName}`, {
    method,
    headers: { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await response.text();
  let json;
  try {
    json = JSON.parse(text);
  } catch {
    json = null;
  }
  if (!response.ok) {
    const message = json?.error || json?.message || text.slice(0, 200);
    throw new Error(
      `${method} ${pathName}: HTTP ${response.status} ${message}`,
    );
  }
  return json?.data ?? json;
}

// ---------------------------------------------------------------- WebDriver BiDi

class Bidi {
  constructor(ws) {
    this.ws = ws;
    this.id = 0;
    this.pending = new Map();
    ws.onmessage = (event) => {
      const message = JSON.parse(event.data);
      const pending = this.pending.get(message.id);
      if (!pending) return;
      this.pending.delete(message.id);
      if (message.type === "error")
        pending.reject(new Error(`${message.error}: ${message.message}`));
      else pending.resolve(message.result);
    };
  }

  static async connect(endpoint, { attempts = 45, gapMs = 2000 } = {}) {
    let last;
    for (let i = 0; i < attempts; i++) {
      try {
        const ws = await new Promise((resolve, reject) => {
          const socket = new WebSocket(endpoint);
          socket.onopen = () => resolve(socket);
          socket.onerror = () => reject(new Error("WebSocket refused"));
        });
        const bidi = new Bidi(ws);
        await bidi.send("session.new", { capabilities: {} });
        return bidi;
      } catch (error) {
        last = error;
        await new Promise((ok) => setTimeout(ok, gapMs));
      }
    }
    throw new Error(`no BiDi session at ${endpoint}: ${last?.message}`);
  }

  send(method, params = {}, timeout = 60000) {
    return new Promise((resolve, reject) => {
      const id = ++this.id;
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify({ id, method, params }));
      setTimeout(() => {
        if (this.pending.delete(id)) reject(new Error(`timeout: ${method}`));
      }, timeout);
    });
  }

  async context() {
    const tree = await this.send("browsingContext.getTree", { maxDepth: 0 });
    return tree.contexts[0].context;
  }

  async evaluate(context, expression) {
    const result = await this.send("script.evaluate", {
      expression,
      target: { context },
      awaitPromise: false,
    });
    if (result.type === "exception") {
      throw new Error(
        `page exception: ${JSON.stringify(result.exceptionDetails).slice(0, 300)}`,
      );
    }
    return result.result?.value;
  }

  async json(context, expression) {
    const value = await this.evaluate(context, `JSON.stringify(${expression})`);
    return value ? JSON.parse(value) : null;
  }

  async click(context, x, y) {
    await this.send("input.performActions", {
      context,
      actions: [
        {
          type: "pointer",
          id: "mouse",
          parameters: { pointerType: "mouse" },
          actions: [
            { type: "pointerMove", x, y, origin: "viewport", duration: 200 },
            { type: "pointerDown", button: 0 },
            { type: "pointerUp", button: 0 },
          ],
        },
      ],
    });
  }

  async type(context, text) {
    for (const char of text) {
      await this.send("input.performActions", {
        context,
        actions: [
          {
            type: "key",
            id: "keyboard",
            actions: [
              { type: "keyDown", value: char },
              { type: "keyUp", value: char },
            ],
          },
        ],
      });
      await new Promise((ok) => setTimeout(ok, 35));
    }
  }

  async screenshot(context, name) {
    try {
      const shot = await this.send("browsingContext.captureScreenshot", {
        context,
      });
      const file = path.join(workDir, `${name}.png`);
      fs.writeFileSync(file, Buffer.from(shot.data, "base64"));
      report.screenshots.push(file);
      return file;
    } catch {
      return null;
    }
  }

  async end() {
    try {
      await this.send("session.end", {}, 5000);
    } catch {
      /* the browser may already be gone */
    }
    this.ws.close();
  }
}

const sleep = (ms) => new Promise((ok) => setTimeout(ok, ms));

async function waitFor(read, done, timeoutMs, intervalMs = 1000) {
  const deadline = Date.now() + timeoutMs;
  let seen;
  while (Date.now() < deadline) {
    seen = await read();
    if (done(seen)) return seen;
    await sleep(intervalMs);
  }
  return seen;
}

async function fillInput(bidi, context, selector, value) {
  // React may still be hydrating, or the page may re-render between the moment
  // the field appears and the moment it is typed into: wait for the element and
  // retry instead of failing on the first miss.
  for (let attempt = 0; attempt < 3; attempt++) {
    const present = await waitFor(
      () =>
        bidi.evaluate(
          context,
          `!!document.querySelector(${JSON.stringify(selector)})`,
        ),
      (found) => found === true,
      12000,
      500,
    );
    if (!present) continue;
    const point = await bidi.json(
      context,
      `(() => { const element = document.querySelector(${JSON.stringify(selector)}); if (!element) return null;
        const rect = element.getBoundingClientRect();
        return { x: Math.round(rect.x + 40), y: Math.round(rect.y + rect.height / 2) }; })()`,
    );
    if (!point) continue;
    await bidi.click(context, point.x, point.y);
    await sleep(250);
    await bidi.evaluate(
      context,
      `document.querySelector(${JSON.stringify(selector)}).select()`,
    );
    await bidi.type(context, value);
    await sleep(250);
    const typed = await bidi.evaluate(
      context,
      `document.querySelector(${JSON.stringify(selector)})?.value`,
    );
    if (typed === value) return;
  }
  throw new Error(`field ${selector} did not take the value`);
}

// ---------------------------------------------------------------- steps

function randomPassword() {
  const alphabet = "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
  let password = "";
  for (let i = 0; i < 18; i++)
    password += alphabet[crypto.randomInt(alphabet.length)];
  return `${password}!Aa1`;
}

async function pickDomain() {
  const { domains, unavailable } = await mailhub("GET", "/api/v1/domains");
  const blocked = new Set((unavailable || []).map((entry) => entry.domain));
  const usable = (domains || []).filter((domain) => !blocked.has(domain));
  if (config.domain) {
    if (blocked.has(config.domain))
      throw new Error(`domain ${config.domain} is not accepting mail`);
    return config.domain;
  }
  if (!usable.length)
    throw new Error("mailhub has no domain that accepts mail");
  return usable[0];
}

async function createMailbox() {
  if (config.email) {
    const [local, domain] = config.email.split("@");
    if (!local || !domain) throw new Error("--email must be an address");
    const mailbox = await mailhub("PUT", `/api/v1/mailboxes/${config.email}`, {
      body: { label: "cf signup" },
    });
    return mailbox;
  }
  const domain = await pickDomain();
  return mailhub("POST", "/api/v1/mailboxes", {
    body: { domain, ttl_hours: 24, label: "cf signup" },
  });
}

async function proxyFromProxygen() {
  if (!config.proxygenKey || !config.proxygenSecret) {
    throw new Error(
      "no proxy: give --proxy-uuid, --proxy, or proxygen credentials",
    );
  }
  const body = {};
  if (config.country) {
    body.countries_mode = "only";
    body.countries = [config.country.toLowerCase()];
  }
  if (config.ttlMinutes) body.ttl_minutes = config.ttlMinutes;
  body.name = `cf-${Date.now().toString(36)}`;
  body.note = "cloudflare signup";
  const generated = await proxygen("POST", "/api/v1/proxies", { body });
  return {
    host: generated.host,
    port: generated.port,
    login: generated.username,
    password: generated.password,
    type: "socks5",
    generatorId: generated.id,
    expiresAt: generated.expires_at,
  };
}

async function resolveProxy() {
  if (config.proxyUuid) {
    const proxies = await owl("GET", "/api/v2/automation/proxies");
    const found = (proxies || []).find(
      (entry) => entry.uuid === config.proxyUuid,
    );
    if (!found) throw new Error(`no stored proxy ${config.proxyUuid}`);
    say(`proxy: stored ${found.title} (${found.type})`);
    return { uuid: found.uuid, title: found.title, type: found.type };
  }

  let proxy;
  let generated = false;
  if (config.proxy) {
    const match = config.proxy.match(
      /^(?:(socks5|socks4|http|https):\/\/)?(?:([^:@]+):([^@]+)@)?([^:]+):(\d+)$/,
    );
    if (!match)
      throw new Error("--proxy wants [scheme://][user:pass@]host:port");
    proxy = {
      type: match[1] || "socks5",
      login: match[2] || "",
      password: match[3] || "",
      host: match[4],
      port: Number(match[5]),
    };
  } else {
    proxy = await proxyFromProxygen();
    generated = true;
  }
  const created = await owl("POST", "/api/v2/automation/proxies", {
    title: proxy.generatorId
      ? `proxygen ${proxy.host}:${proxy.port}`
      : `${proxy.host}:${proxy.port}`,
    type: proxy.type,
    host: proxy.host,
    port: proxy.port,
    login: proxy.login,
    password: proxy.password,
  });
  say(`proxy: ${created.title}${generated ? " (generated)" : ""}`);
  return {
    uuid: created.uuid,
    title: created.title,
    type: created.type,
    generated,
  };
}

async function createProfile(proxy) {
  const profile = await owl("POST", "/api/v2/automation/profiles", {
    title: `cf ${new Date().toISOString().slice(5, 19)} ${crypto.randomBytes(2).toString("hex")}`,
    browser: "camoufox",
    fingerprint: { os: config.browserOs },
    ...(proxy?.uuid ? { proxy: { uuid: proxy.uuid } } : {}),
  });
  say(`profile: ${profile.uuid}`);
  return profile;
}

function findWidgetExpression() {
  return `(() => {
    const password = document.querySelector('input[name=password]');
    const submit = document.querySelector('button[type=submit]');
    if (!password || !submit) return null;
    const passwordRect = password.getBoundingClientRect();
    const submitRect = submit.getBoundingClientRect();
    const boxes = [...document.querySelectorAll('div,section')]
      .map((element) => element.getBoundingClientRect())
      // The widget box is ~71 px tall; the label above it (~49 px) and the
      // terms paragraph (~42 px) must not be mistaken for it.
      .filter((rect) => rect.width >= 320 && rect.width <= 620 && rect.height >= 60 && rect.height <= 90)
      .filter((rect) => rect.y > passwordRect.bottom + 15 && rect.y + rect.height < submitRect.top + 2)
      .map((rect) => ({ x: Math.round(rect.x), y: Math.round(rect.y), w: Math.round(rect.width), h: Math.round(rect.height) }));
    if (!boxes.length) return null;
    // The widget itself is the lowest such box, right above the submit button;
    // a wrapper around the label sits above it.
    boxes.sort((a, b) => b.y - a.y);
    return boxes[0];
  })()`;
}

async function solveTurnstile(bidi, context) {
  const started = Date.now();
  const deadline =
    started +
    config.turnstileTimeoutMs +
    (config.manual ? config.manualTimeoutMs : 0);
  let manualAnnounced = false;
  let box = null;

  while (Date.now() < deadline) {
    const token = await bidi.evaluate(
      context,
      "(document.querySelector('input[name=cf_challenge_response]')?.value || '')",
    );
    if (token)
      return { token, ms: Date.now() - started, manual: manualAnnounced };

    box = await bidi.json(context, findWidgetExpression());
    if (box) {
      // The widget's own challenge may finish without any click: give it room
      // first, then tick the checkbox.
      await sleep(4000);
      const passive = await waitFor(
        () =>
          bidi.evaluate(
            context,
            "(document.querySelector('input[name=cf_challenge_response]')?.value || '')",
          ),
        (value) => Boolean(value),
        8000,
        1000,
      );
      if (passive)
        return {
          token: passive,
          ms: Date.now() - started,
          manual: manualAnnounced,
        };

      // The checkbox sits at the left of the widget, vertically centred. The
      // first click may only focus the widget, so each point is tried twice.
      for (const [dx, dy] of [
        [22, 34],
        [22, 42],
        [26, 30],
        [18, 34],
      ]) {
        for (let attempt = 0; attempt < 2; attempt++) {
          await bidi.click(context, box.x + dx, box.y + dy);
          const got = await waitFor(
            () =>
              bidi.evaluate(
                context,
                "(document.querySelector('input[name=cf_challenge_response]')?.value || '')",
              ),
            (value) => Boolean(value),
            7000,
            700,
          );
          if (got)
            return {
              token: got,
              ms: Date.now() - started,
              manual: manualAnnounced,
            };
        }
      }
    }

    if (config.manual && !manualAnnounced) {
      manualAnnounced = true;
      say("Turnstile: click the checkbox in the browser window (waiting)");
    }
    await sleep(1500);
  }
  await bidi.screenshot(context, "turnstile-timeout");
  throw new Error(
    `Turnstile gave no token in ${Math.round((Date.now() - started) / 1000)}s`,
  );
}

async function submitForm(bidi, context) {
  const button = await bidi.json(
    context,
    `(() => { const button = document.querySelector('button[type=submit]'); if (!button) return null;
      const rect = button.getBoundingClientRect();
      return { x: Math.round(rect.x + rect.width / 2), y: Math.round(rect.y + rect.height / 2) }; })()`,
  );
  if (!button) throw new Error("no submit button");
  await bidi.click(context, button.x, button.y);
  await sleep(2500);

  const state = await waitFor(
    () =>
      bidi.json(
        context,
        `(() => ({ url: location.href, body: (document.body.innerText || '').slice(0, 1200) }))()`,
      ),
    (value) =>
      value &&
      (value.url !== SIGNUP_URL ||
        /captcha|verify|error|check your email|add a site|welcome/i.test(
          value.body,
        )),
    30000,
    2000,
  );
  const body = state?.body || "";
  if (/please complete the captcha/i.test(body))
    return { ok: false, reason: "captcha", url: state.url, body };
  if (state?.url && state.url !== SIGNUP_URL)
    return { ok: true, url: state.url, body };
  if (/check your email|verify your email/i.test(body))
    return { ok: true, url: state?.url, body };
  return { ok: state?.url !== SIGNUP_URL, url: state?.url, body };
}

async function findVerificationLink(address, since) {
  const messages = await mailhub(
    "GET",
    `/api/v1/mailboxes/${address}/messages`,
    {
      query: { since, limit: "10" },
    },
  );
  const items = messages.items || [];
  for (const message of items) {
    for (const link of message.links || []) {
      if (/cloudflare\.com/i.test(link)) return { link, message };
    }
  }
  for (const message of items) {
    for (const link of message.links || []) {
      if (/^https:\/\//i.test(link)) return { link, message };
    }
  }
  return items.length ? { message: items[0] } : null;
}

// ---------------------------------------------------------------- login, key, token

const CLOUDFLARE_API = "https://api.cloudflare.com/client/v4";

async function navigateTo(bidi, context, url) {
  for (let attempt = 0; attempt < 4; attempt++) {
    try {
      await bidi.send(
        "browsingContext.navigate",
        { context, url, wait: "complete" },
        45000,
      );
      return;
    } catch (error) {
      if (attempt === 3) throw error;
      await sleep(2500);
    }
  }
}

async function findLatestCode(email, since) {
  const messages = await mailhub("GET", `/api/v1/mailboxes/${email}/messages`, {
    query: { since, limit: "5" },
  });
  for (const message of messages.items || []) {
    const seven = (message.codes || []).find((code) => /^\d{7}$/.test(code));
    if (seven) return seven;
  }
  return null;
}

// Cloudflare asks for a 7-digit code by email when the sign-in comes from an
// address it has not seen; mailhub already extracts codes from the message.
async function handleTwoFactor(bidi, context, email, since) {
  const selector =
    "input[autocomplete=one-time-code],input[name=code],input[inputmode=numeric]";
  let window = since;
  for (let cycle = 0; cycle < 3; cycle++) {
    // A fresh code per cycle: Cloudflare's code is single-use, so retyping the
    // same one after a rejected attempt never succeeds.
    let code = null;
    for (let round = 0; round < 25 && !code; round++) {
      code = await findLatestCode(email, window);
      if (!code) await sleep(3000);
    }
    if (!code) {
      await clickButtonText(bidi, context, "/resend/i");
      say("login: no two-factor code, asked for a resend");
      window = new Date().toISOString();
      await sleep(5000);
      continue;
    }
    say(`login: two-factor code ${code}`);
    const field = await bidi.json(
      context,
      `(() => { const element = document.querySelector(${JSON.stringify(selector)}); if (!element) return null;
        const rect = element.getBoundingClientRect();
        return { x: Math.round(rect.x + rect.width / 2), y: Math.round(rect.y + rect.height / 2), maxLength: element.maxLength, value: element.value }; })()`,
    );
    say(`login: code field ${JSON.stringify(field)}`);
    if (!field) {
      await sleep(2000);
      continue;
    }
    await bidi.click(context, field.x, field.y);
    await sleep(300);
    await bidi.evaluate(
      context,
      `document.querySelector(${JSON.stringify(selector)}).select()`,
    );
    await bidi.type(context, code);
    await sleep(300);
    const typed = await bidi.evaluate(
      context,
      `document.querySelector(${JSON.stringify(selector)})?.value`,
    );
    say(`login: typed code ${typed}`);
    if (typed !== code) {
      // Segmented input: type again over whatever is there.
      await bidi.click(context, field.x, field.y);
      await bidi.type(context, code);
      await sleep(300);
    }
    const buttons = await bidi.json(
      context,
      `(() => [...document.querySelectorAll('button')].filter((b) => /verify|continue|sign in/i.test((b.textContent || '').trim())).map((b) => { const r = b.getBoundingClientRect(); return { text: (b.textContent || '').trim(), x: Math.round(r.x + r.width / 2), y: Math.round(r.y + r.height / 2), disabled: b.disabled }; }))()`,
    );
    say(`login: two-factor buttons ${JSON.stringify(buttons)}`);
    const verify =
      (buttons || []).find((b) => /^verify$/i.test(b.text)) ||
      (buttons || [])[0];
    if (!verify) throw new Error("no Verify button on the two-factor page");
    await bidi.click(context, verify.x, verify.y);
    await sleep(5000);
    const url = await bidi.evaluate(context, "location.href");
    if (!url.includes("/two-factor")) return;
    say(`login: code rejected (still at ${url}), asking for a new one`);
    await clickButtonText(bidi, context, "/resend/i");
    window = new Date().toISOString();
    await sleep(5000);
  }
  throw new Error("the two-factor code was not accepted");
}

async function login(bidi, context, email, password) {
  for (let attempt = 0; attempt < 4; attempt++) {
    await navigateTo(bidi, context, "https://dash.cloudflare.com/login");
    await waitFor(
      () =>
        bidi.evaluate(context, "!!document.querySelector('input[name=email]')"),
      (ready) => ready === true,
      60000,
    );
    await sleep(1500);
    await fillInput(bidi, context, "input[name=email]", email);
    const hasPassword = await bidi.evaluate(
      context,
      "!!document.querySelector('input[name=password]')",
    );
    if (!hasPassword) {
      await clickButtonText(
        bidi,
        context,
        "/continue with password|continue|next/i.test((button.textContent || '').trim())",
      );
      await sleep(1500);
    }
    await fillInput(bidi, context, "input[name=password]", password);

    const since = new Date().toISOString();
    // The sign-in page's Turnstile is invisible and scores behaviour: a few
    // pointer moves and a longer wait before submitting give it a chance.
    for (let move = 0; move < 4; move++) {
      await bidi.send("input.performActions", {
        context,
        actions: [
          {
            type: "pointer",
            id: "mouse",
            parameters: { pointerType: "mouse" },
            actions: [
              {
                type: "pointerMove",
                x: 520 + move * 55,
                y: 300 + (move % 2) * 60,
                origin: "viewport",
                duration: 260,
              },
            ],
          },
        ],
      });
      await sleep(250);
    }
    await waitFor(
      () =>
        bidi.evaluate(
          context,
          "(document.querySelector('input[name=cf_challenge_response]')?.value || '')",
        ),
      (value) => Boolean(value),
      25000,
    );
    const submit = await bidi.json(
      context,
      `(() => { const button = document.querySelector('button[type=submit]'); if (!button) return null;
        const rect = button.getBoundingClientRect();
        return { x: Math.round(rect.x + rect.width / 2), y: Math.round(rect.y + rect.height / 2) }; })()`,
    );
    if (!submit) throw new Error("no login submit button");
    await bidi.click(context, submit.x, submit.y);

    const left = await waitFor(
      () => bidi.evaluate(context, "location.href"),
      (url) => typeof url === "string" && !url.includes("/login"),
      30000,
      1500,
    );
    let url = typeof left === "string" ? left : "";
    if (url.includes("/two-factor")) {
      await handleTwoFactor(bidi, context, email, since);
      url = await bidi.evaluate(context, "location.href");
    }
    if (
      url.startsWith("https://dash.cloudflare.com") &&
      !url.includes("/login") &&
      !url.includes("/two-factor")
    ) {
      // The dashboard SPA takes a while; navigating before it settles sends the
      // next page back to /login.
      await sleep(3000);
      await waitFor(
        () =>
          bidi.evaluate(
            context,
            "document.readyState === 'complete' && !/loading dashboard/i.test((document.body.innerText || '').slice(0, 120))",
          ),
        (settled) => settled === true,
        60000,
        2000,
      );
      await sleep(4000);
      return;
    }
    say(`login: attempt ${attempt + 1} did not sign in, retrying`);
    await sleep(2000);
  }
  await bidi.screenshot(context, "login-failed");
  throw new Error("login did not leave the sign-in page");
}

async function clickButtonText(bidi, context, expression, predicate = "true") {
  const point = await bidi.json(
    context,
    `(() => {
      const buttons = [...document.querySelectorAll('button')].filter((button) => ${expression});
      const button = buttons.find((candidate) => ${predicate});
      if (!button) return null;
      const rect = button.getBoundingClientRect();
      return { x: Math.round(rect.x + rect.width / 2), y: Math.round(rect.y + rect.height / 2) };
    })()`,
  );
  if (!point) return false;
  await bidi.click(context, point.x, point.y);
  return true;
}

async function ensureEmailVerified(bidi, context, email) {
  await navigateTo(bidi, context, "https://dash.cloudflare.com/");
  const current = await bidi.evaluate(context, "location.href");
  if (typeof current === "string" && current.includes("/login")) {
    return "login-required";
  }
  await sleep(5000);
  const unverified = await bidi.evaluate(
    context,
    "/verify your email address/i.test(document.body.innerText)",
  );
  if (!unverified) return "already";
  const since = new Date().toISOString();
  const clicked = await clickButtonText(
    bidi,
    context,
    "/resend verification email/i.test((button.textContent || '').trim())",
  );
  if (!clicked) throw new Error("no Resend verification email button");
  say("verification email resend requested");
  const deadline = Date.now() + 120000;
  let link = null;
  while (Date.now() < deadline && !link) {
    const hit = await findVerificationLink(email, since);
    if (hit?.link) link = hit.link;
    else await sleep(5000);
  }
  if (!link) throw new Error("no verification email after the resend");
  await navigateTo(bidi, context, link);
  await sleep(7000);
  const still = await bidi.evaluate(
    context,
    "/verify your email address/i.test(document.body.innerText)",
  );
  return still ? "still-unverified" : "verified";
}

async function revealGlobalKey(bidi, context, password, email) {
  const keysUrl = "https://dash.cloudflare.com/profile/api-keys";
  const token = () =>
    bidi.evaluate(
      context,
      "(document.querySelector('input[name=cf-turnstile-response]')?.value || '')",
    );

  for (let round = 0; round < config.keyRounds; round++) {
    let ready = false;
    for (let attempt = 0; attempt < 3 && !ready; attempt++) {
      await navigateTo(bidi, context, keysUrl);
      await sleep(2000);
      // A dialog left open from the previous round survives the SPA route; a
      // real reload is what closes it.
      const stuck = await bidi.evaluate(
        context,
        "/complete the security check/i.test(document.body.innerText)",
      );
      if (stuck) {
        try {
          await bidi.send(
            "browsingContext.reload",
            { context, wait: "complete" },
            45000,
          );
        } catch {
          /* the reload raced a navigation */
        }
        await sleep(3000);
      }
      ready = await waitFor(
        () =>
          bidi.evaluate(
            context,
            "/global api key/i.test(document.body.innerText)",
          ),
        (value) => value === true,
        60000,
      );
      if (ready) break;
      const url = await bidi.evaluate(context, "location.href");
      if (url.includes("/login")) {
        await login(bidi, context, email, password);
      } else {
        await sleep(3000);
      }
    }
    if (!ready) {
      const url = await bidi.evaluate(context, "location.href");
      throw new Error(`the API keys page did not open (at ${url})`);
    }
    // Let the page finish hydrating: a click during the first paint opens the
    // dialog but React re-renders it and drops the typed password.
    await sleep(2500);

    await sleep(3000);
    // Page row "View" sits at the far right; the dialog copies are lower.
    let modalOpen = false;
    for (let viewTry = 0; viewTry < 3 && !modalOpen; viewTry++) {
      const clicked =
        (await clickButtonText(
          bidi,
          context,
          `/^view$/i.test((button.textContent || "").trim())`,
          "candidate.getBoundingClientRect().x > 1000",
        )) ||
        (await clickButtonText(
          bidi,
          context,
          `/^view$/i.test((button.textContent || "").trim())`,
        ));
      if (!clicked) {
        say("key: no View button on the keys page");
        await sleep(1500);
        continue;
      }
      modalOpen = await waitFor(
        () =>
          bidi.evaluate(
            context,
            "!!document.querySelector('input[type=password]')",
          ),
        (value) => value === true,
        8000,
        500,
      );
      if (!modalOpen)
        say(`key: identity dialog did not open (try ${viewTry + 1})`);
    }
    if (!modalOpen) {
      say("key: identity dialog never opened, starting over");
      continue;
    }
    await fillInput(bidi, context, "input[type=password]", password);
    if (
      !(await clickButtonText(
        bidi,
        context,
        `/^verify$/i.test((button.textContent || "").trim())`,
      ))
    ) {
      throw new Error("no Verify button in the identity dialog");
    }
    await sleep(3500);

    // "Your API key" dialog: the security check is an invisible Turnstile, so
    // there is no checkbox to click. It either answers by itself or not at all
    // (the footer buttons are Cancel / View, not the widget).
    const opened = await bidi.evaluate(
      context,
      "/complete the security check/i.test(document.body.innerText)",
    );
    say(`key dialog open: ${opened}`);
    let solved = "";
    if (opened) {
      // The dialog holds a visible Turnstile checkbox: a ~404x71 widget between
      // the text and the footer (the ~436x60 footer holds Cancel / View, and
      // clicking its left side is what dismissed the dialog before).
      let widgets = [];
      for (let i = 0; i < 12 && !widgets.length; i++) {
        widgets = await bidi.json(
          context,
          `(() => [...document.querySelectorAll('div,section')]
            .map((element) => element.getBoundingClientRect())
            .filter((rect) => rect.width >= 380 && rect.width <= 450 && rect.height >= 65 && rect.height <= 82 && rect.y > 100)
            .map((rect) => ({ x: Math.round(rect.x), y: Math.round(rect.y), w: Math.round(rect.width), h: Math.round(rect.height) })))()`,
        );
        if (!widgets.length) await sleep(1500);
      }
      say(`key widgets: ${JSON.stringify(widgets)}`);
      for (const box of widgets) {
        for (const [dx, dy] of [
          [19, 32],
          [22, 40],
          [16, 28],
        ]) {
          await bidi.click(context, box.x + dx, box.y + dy);
          solved = await waitFor(
            () => token(),
            (value) => Boolean(value),
            12000,
            1000,
          );
          if (solved) break;
        }
        if (solved) break;
      }
    }
    if (!solved) {
      say(`key: no security token (round ${round + 1}), starting over`);
      await sleep(2000);
      continue;
    }
    say("key dialog: security check cleared");
    // The View button enables a moment after the token lands; read its state,
    // then wait for the 37-hex key (text or read-only input).
    await sleep(1500);
    const viewButtons = await bidi.json(
      context,
      `(() => [...document.querySelectorAll('button')]
        .filter((b) => /^view$/i.test((b.textContent || '').trim()))
        .map((b) => { const r = b.getBoundingClientRect(); return { x: Math.round(r.x + r.width / 2), y: Math.round(r.y + r.height / 2), disabled: b.disabled }; }))()`,
    );
    say(`key view buttons: ${JSON.stringify(viewButtons)}`);
    const view = (viewButtons || []).find(
      (b) => b.x > 700 && b.x < 1000 && b.y > 200,
    );
    if (!view) throw new Error("no View button in the key dialog");
    await bidi.click(context, view.x, view.y);

    const readKey = () =>
      bidi.evaluate(
        context,
        `(() => {
          const text = document.body.innerText || "";
          if (/please verify your email/i.test(text)) return "EMAIL_NOT_VERIFIED";
          const match = text.match(/(cfk_[A-Za-z0-9]+|[0-9a-f]{37})/);
          if (match) return match[0];
          for (const input of document.querySelectorAll("input")) {
            if (input.value && /^(cfk_[A-Za-z0-9]+|[0-9a-f]{37})$/.test(input.value)) return input.value;
          }
          return "";
        })()`,
      );
    let key = "";
    for (let i = 0; i < 8 && !key; i++) {
      await sleep(1500);
      key = await readKey();
    }
    say(
      `key after view: ${key ? `${key.slice(0, 8)}…` : "(none)"} dialog: ${JSON.stringify(await bidi.evaluate(context, "(document.body.innerText || '').slice(-160)"))}`,
    );
    if (key === "EMAIL_NOT_VERIFIED") {
      throw new Error(
        "Cloudflare refuses the key: the account email is not verified",
      );
    }
    if (key) return key;
    say("key: token cleared but the key was not shown, starting over");
  }

  await bidi.screenshot(context, "key-turnstile-failed");
  throw new Error("the key dialog's security check did not clear");
}

async function cloudflareApi(method, pathName, auth, body) {
  const response = await fetch(`${CLOUDFLARE_API}${pathName}`, {
    method,
    headers: { "Content-Type": "application/json", ...auth },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const json = await response.json().catch(() => null);
  if (!json?.success) {
    const message = (json?.errors || [])
      .map((error) => `${error.code} ${error.message}`)
      .join("; ");
    throw new Error(
      `${method} ${pathName}: ${message || `HTTP ${response.status}`}`,
    );
  }
  return json.result;
}

// A user token that allows every account and zone permission on every
// resource: the widest scope Cloudflare's token API can express.
async function createAdminToken(email, globalKey) {
  const auth = { "X-Auth-Email": email, "X-Auth-Key": globalKey };
  const accounts = await cloudflareApi("GET", "/accounts", auth);
  const accountId = accounts?.[0]?.id;
  if (!accountId) throw new Error("the account id is missing");

  const groups = await cloudflareApi(
    "GET",
    "/user/tokens/permission_groups",
    auth,
  );
  // One policy per resource scope, at most 300 permission groups per policy
  // (the API rejects more), every resource of that scope allowed.
  const byScope = {
    "com.cloudflare.api.account": [],
    "com.cloudflare.api.account.zone": [],
  };
  for (const group of groups || []) {
    for (const scope of group.scopes || []) {
      if (byScope[scope]) byScope[scope].push(group);
    }
  }
  const policies = [];
  let count = 0;
  for (const [scope, list] of Object.entries(byScope)) {
    for (let i = 0; i < list.length; i += 300) {
      const part = list.slice(i, i + 300);
      count += part.length;
      policies.push({
        effect: "allow",
        resources: { [`${scope}.*`]: "*" },
        permission_groups: part.map((group) => ({ id: group.id })),
      });
    }
  }
  const token = await cloudflareApi("POST", "/user/tokens", auth, {
    name: `owl-admin-${new Date().toISOString().slice(0, 19)}`,
    policies,
  });
  return {
    accountId,
    token: token.value,
    tokenId: token.id,
    permissions: count,
  };
}

async function runTokenSteps(bidi, context, email, password) {
  const key = await timed("api-key", () =>
    revealGlobalKey(bidi, context, password, email),
  );
  report.apiKey = key;
  say(`global API key: ${key.slice(0, 6)}…`);
  try {
    const created = await timed("api-token", () =>
      createAdminToken(email, key),
    );
    report.apiToken = created.token;
    report.apiTokenId = created.tokenId;
    report.accountId = created.accountId;
    report.apiTokenPermissions = created.permissions;
    say(
      `api token: ${created.token.slice(0, 8)}… (${created.permissions} permission groups, account ${created.accountId})`,
    );
  } catch (error) {
    report.apiTokenError = error.message;
    say(`api token not created: ${error.message}`);
  }
}

async function openBrowser(proxy, url) {
  const profile = await timed("profile", () => createProfile(proxy));
  report.profile = { uuid: profile.uuid, title: profile.title };
  const started = await timed("launch", () =>
    owl("POST", "/api/profiles/start", {
      uuid: profile.uuid,
      headless: !config.headed,
      debug_port: true,
      url,
    }),
  );
  if (!started?.ws_endpoint) throw new Error("start returned no ws_endpoint");
  say(`browser: ${started.ws_endpoint}`);
  const bidi = await Bidi.connect(started.ws_endpoint);
  const context = await bidi.context();
  await bidi.send("browsingContext.setViewport", {
    context,
    viewport: VIEWPORT,
    devicePixelRatio: 1,
  });
  return { profile, bidi, context };
}

async function tokenOnlyRun() {
  if (!config.email || !config.password) {
    throw new Error("--token-only needs --email and --password");
  }
  report.email = config.email;
  report.password = config.password;
  const proxy = await timed("proxy", resolveProxy);
  report.proxy = proxy;
  const opened = await openBrowser(proxy, "https://dash.cloudflare.com/login");
  try {
    await timed("login", () =>
      login(opened.bidi, opened.context, config.email, config.password),
    );
    report.emailVerified = await ensureEmailVerified(
      opened.bidi,
      opened.context,
      config.email,
    );
    say(`email verification: ${report.emailVerified}`);
    await runTokenSteps(
      opened.bidi,
      opened.context,
      config.email,
      config.password,
    );
    report.outcome = "token-created";
  } finally {
    await opened.bidi.end();
    if (!config.keep) {
      await owl("POST", "/api/profiles/stop", {
        uuid: opened.profile.uuid,
      }).catch(() => {});
      await sleep(1500);
      await owl("DELETE", "/api/v2/automation/profiles", {
        uuids: [opened.profile.uuid],
      }).catch(() => {});
    }
  }
}

// ---------------------------------------------------------------- main

async function main() {
  if (config.tokenOnly) return tokenOnlyRun();
  if (!config.mailhubKey || !config.mailhubSecret)
    throw new Error("MAILHUB_KEY and MAILHUB_SECRET are required");
  if (!config.password) config.password = randomPassword();
  if (config.manual) config.headed = true;
  report.password = config.password;

  const mailbox = await timed("mailbox", createMailbox);
  report.email = mailbox.address;
  say(`mailbox: ${mailbox.address}`);

  const proxy = await timed("proxy", resolveProxy);
  report.proxy = proxy;

  const profile = await timed("profile", () => createProfile(proxy));
  report.profile = { uuid: profile.uuid, title: profile.title };

  let bidi = null;
  let context = null;
  let created = false;
  try {
    const started = await timed("launch", () =>
      owl("POST", "/api/profiles/start", {
        uuid: profile.uuid,
        headless: !config.headed,
        debug_port: true,
        url: SIGNUP_URL,
      }),
    );
    if (!started?.ws_endpoint) throw new Error("start returned no ws_endpoint");
    say(`browser: ${started.ws_endpoint}`);

    bidi = await Bidi.connect(started.ws_endpoint);
    context = await bidi.context();
    await bidi.send("browsingContext.setViewport", {
      context,
      viewport: VIEWPORT,
      devicePixelRatio: 1,
    });

    await timed("form", async () => {
      await waitFor(
        () =>
          bidi.evaluate(
            context,
            "!!document.querySelector('input[name=email]')",
          ),
        (ready) => ready === true,
        60000,
        1000,
      );
      await sleep(1500);
      await fillInput(bidi, context, "input[name=email]", report.email);
      await fillInput(bidi, context, "input[name=password]", report.password);
      report.fieldsFilled = true;
    });

    const turnstile = await timed("turnstile", () =>
      solveTurnstile(bidi, context),
    );
    say(
      `turnstile: token in ${turnstile.ms} ms${turnstile.manual ? " (manual)" : ""}`,
    );

    const since = new Date().toISOString();
    const submitted = await timed("submit", () => submitForm(bidi, context));
    say(
      `submit: ${submitted.ok ? "accepted" : submitted.reason || "no change"} ${submitted.url || ""}`,
    );
    if (!submitted.ok) {
      await bidi.screenshot(context, "submit-failed");
      throw new Error(
        `the form refused the signup: ${(submitted.body || "").slice(0, 300)}`,
      );
    }
    created = true;

    const found = await timed("verification-email", async () => {
      const deadline = Date.now() + config.emailTimeoutMs;
      while (Date.now() < deadline) {
        const hit = await findVerificationLink(report.email, since);
        if (hit?.link) return hit;
        await sleep(5000);
      }
      return null;
    });
    if (!found?.link) {
      say("verification email: not found in time");
      report.outcome = "created-unverified";
      return;
    }
    say(`verification link: ${found.link.slice(0, 120)}`);
    report.verificationLink = found.link;

    await timed("verify", async () => {
      // After the signup submit the browser is already in the account, so the
      // address can be verified without a fresh sign-in (a sign-in is what
      // triggers Cloudflare's two-factor email, which an unverified address
      // does not receive).
      await sleep(6000);
      report.emailVerified = await ensureEmailVerified(
        bidi,
        context,
        report.email,
      );
      if (
        report.emailVerified === "login-required" ||
        report.emailVerified === "still-unverified"
      ) {
        await navigateTo(bidi, context, found.link);
        await sleep(6000);
        await login(bidi, context, report.email, report.password);
        report.verification = {
          url: await bidi.evaluate(context, "location.href"),
        };
        report.emailVerified = await ensureEmailVerified(
          bidi,
          context,
          report.email,
        );
      }
      say(`email verification: ${report.emailVerified}`);
    });

    if (!config.noToken) {
      try {
        await runTokenSteps(bidi, context, report.email, report.password);
      } catch (error) {
        report.tokenError = error.message;
        say(`token steps failed: ${error.message}`);
        if (bidi && context) await bidi.screenshot(context, "token-failed");
      }
    }
    report.outcome = "created-verified";
  } catch (error) {
    report.error = error.message;
    report.outcome = created ? "created-unverified" : "failed";
    if (bidi && context) await bidi.screenshot(context, "error");
    throw error;
  } finally {
    if (bidi) await bidi.end();
    if (!config.keep) {
      await owl("POST", "/api/profiles/stop", { uuid: profile.uuid }).catch(
        () => {},
      );
      await sleep(1500);
      await owl("DELETE", "/api/v2/automation/profiles", {
        uuids: [profile.uuid],
      }).catch(() => {});
    }
  }
}

main()
  .then(() => {
    const file = saveReport();
    say(`done: ${report.outcome}`);
    say(`report: ${file}`);
    say(`screenshots: ${workDir}`);
    process.exit(report.outcome === "created-verified" ? 0 : 2);
  })
  .catch((error) => {
    report.outcome = report.outcome || "failed";
    report.error = report.error || error.message;
    const file = saveReport();
    say(`failed: ${error.message}`);
    say(`report: ${file}`);
    say(`screenshots: ${workDir}`);
    process.exit(1);
  });
