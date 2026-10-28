#!/usr/bin/env node
// The weekly check of Owl against Donut Browser: the upstream commits of the
// last days that Owl has not merged go to DeepSeek with what the fork is (the
// fork section of AGENTS.md), and its short review goes to the team's
// Telegram group. Run by .github/workflows/upstream-review.yml.
//
//   node scripts/upstream-review.mjs            # needs the variables below
//   node scripts/upstream-review.mjs --dry-run  # prints the prompt, sends nothing
//
// DEEPSEEK_API_KEY, TELEGRAM_BOT_TOKEN, TELEGRAM_CHAT_ID; optional
// DEEPSEEK_MODEL (deepseek-flash), UPSTREAM_REF (upstream/main), REVIEW_DAYS (7).
// The last merged upstream commit is read from .github/upstream-merged, which
// every upstream merge updates: the published history has no upstream commits
// to find it by.

import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const dryRun = process.argv.includes("--dry-run");
const upstreamRef = process.env.UPSTREAM_REF || "upstream/main";
const days = Number(process.env.REVIEW_DAYS || 7);
const model = process.env.DEEPSEEK_MODEL || "deepseek-flash";
const UPSTREAM_URL = "https://github.com/zhom/donutbrowser";
// One commit's diff, and all of them together, stay within DeepSeek's context.
const COMMIT_DIFF_CHARS = 6000;
const TOTAL_DIFF_CHARS = 150_000;
const TELEGRAM_CHUNK = 3900;
// Lock files and translations are long and say nothing a review needs.
const DIFF_EXCLUDES = [
  ":(exclude)*.lock",
  ":(exclude)pnpm-lock.yaml",
  ":(exclude)src/i18n/locales/*",
  ":(exclude)src/generated/*",
];

const git = (...args) =>
  execFileSync("git", args, {
    cwd: root,
    encoding: "utf8",
    maxBuffer: 256 * 1024 * 1024,
  }).trim();

const escapeHtml = (text) =>
  text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

function required(name) {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is not set`);
  return value;
}

const [merged, mergedName = ""] = readFileSync(
  resolve(root, ".github/upstream-merged"),
  "utf8",
)
  .trim()
  .split(/\s+/);
const upstreamHead = git("rev-parse", upstreamRef);
const pending = Number(git("rev-list", "--count", `${merged}..${upstreamRef}`));
let latestRelease = "";
try {
  latestRelease = git(
    "describe",
    "--tags",
    "--abbrev=0",
    "--match",
    "v[0-9]*",
    upstreamRef,
  );
} catch {}

const shas = git(
  "log",
  "--no-merges",
  "--reverse",
  `--since=${days}.days.ago`,
  "--format=%H",
  `${merged}..${upstreamRef}`,
)
  .split("\n")
  .filter(Boolean);

let diffBudget = TOTAL_DIFF_CHARS;
const commits = shas.map((sha) => {
  const head = git(
    "show",
    "-s",
    "--format=%h %ad %s%n%n%b",
    "--date=short",
    sha,
  );
  const stat = git("show", "--format=", "--stat=120", sha);
  let diff = git(
    "show",
    "--format=",
    "--patch",
    "-U2",
    "--no-color",
    sha,
    "--",
    ".",
    ...DIFF_EXCLUDES,
  );
  if (diff.length > COMMIT_DIFF_CHARS) {
    diff = `${diff.slice(0, COMMIT_DIFF_CHARS)}\n[... diff cut]`;
  }
  if (diff.length > diffBudget) diff = "[diff left out: the prompt is full]";
  diffBudget -= diff.length;
  return `=== ${head}\n${stat}\n${diff}`;
});

const header = [
  `<b>Donut → Owl: сверка за ${days} дн.</b>`,
  `Новых коммитов в Donut: ${commits.length}. Всего не влито: ${pending}.`,
  `Влито до ${escapeHtml(mergedName || merged.slice(0, 8))}` +
    (latestRelease
      ? `, последний релиз Donut: ${escapeHtml(latestRelease)}`
      : "") +
    ".",
  `<a href="${UPSTREAM_URL}/compare/${merged}...${upstreamHead}">Сравнение на GitHub</a>`,
].join("\n");

const forkContext = readFileSync(resolve(root, "AGENTS.md"), "utf8").split(
  "\n---\n",
)[0];

const system = `Ты ревьюер форка Donut Browser под названием Owl. Вот что это за форк и чем он отличается от Donut:

${forkContext}

Тебе дают коммиты Donut, которые ещё не влиты в Owl. Напиши по-русски короткое ревью для группы в Telegram, не длиннее 2500 знаков:
1. Одной-двумя фразами: что главное поменялось.
2. Что поменялось, по темам (2-6 пунктов), с короткими хешами коммитов.
3. Что рекомендуешь Owl: влить скоро (безопасность, исправления прокси, запуска и Camoufox, общего кода), можно подождать, или не нужно (только Wayfern, облако Donut, Linux/Windows, их сервер синхронизации на Rust). Отметь, если коммит трогает то, что форк изменил у себя, и мерж будет конфликтным.
Пиши по делу, без вступлений. Форматирование только тегами Telegram HTML: <b>, <i>, <code>. Не используй Markdown. Символы <, > и & вне тегов пиши как &lt; &gt; &amp;.`;

const user = `Коммиты Donut за последние ${days} дней (всего не влито ${pending}):\n\n${commits.join("\n\n")}`;

async function review() {
  const response = await fetch("https://api.deepseek.com/chat/completions", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${required("DEEPSEEK_API_KEY")}`,
    },
    body: JSON.stringify({
      model,
      temperature: 0.3,
      // Flash thinks before it answers, and the thinking counts here too.
      max_tokens: 16000,
      messages: [
        { role: "system", content: system },
        { role: "user", content: user },
      ],
    }),
    signal: AbortSignal.timeout(10 * 60 * 1000),
  });
  const body = await response.json().catch(() => ({}));
  const text = body.choices?.[0]?.message?.content?.trim();
  if (!response.ok || !text) {
    throw new Error(
      `DeepSeek answered ${response.status}: ${body.error?.message ?? "no review"}`,
    );
  }
  return text;
}

/** Splits at line ends so a chunk never cuts a tag in half. */
function chunks(text) {
  const out = [];
  let current = "";
  for (const line of text.split("\n")) {
    if (current && current.length + line.length + 1 > TELEGRAM_CHUNK) {
      out.push(current);
      current = "";
    }
    current = current ? `${current}\n${line}` : line;
  }
  if (current) out.push(current);
  return out;
}

async function send(text) {
  const token = required("TELEGRAM_BOT_TOKEN");
  const chatId = required("TELEGRAM_CHAT_ID");
  const post = (payload) =>
    fetch(`https://api.telegram.org/bot${token}/sendMessage`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        chat_id: chatId,
        disable_web_page_preview: true,
        ...payload,
      }),
    }).then((r) => r.json());
  for (const chunk of chunks(text)) {
    let result = await post({ text: chunk, parse_mode: "HTML" });
    // The model's markup is not always what Telegram accepts; plain text is.
    if (!result.ok && /parse/i.test(result.description ?? "")) {
      result = await post({
        text: chunk
          .replace(/<[^>]+>/g, "")
          .replace(/&lt;/g, "<")
          .replace(/&gt;/g, ">")
          .replace(/&amp;/g, "&"),
      });
    }
    if (!result.ok) throw new Error(`Telegram: ${result.description}`);
  }
}

if (dryRun) {
  console.log(header);
  console.log(
    `\ncommits=${commits.length} pending=${pending} system_chars=${system.length} user_chars=${user.length}`,
  );
  console.log(`\n${user.slice(0, 3000)}`);
} else if (commits.length === 0) {
  await send(`${header}\n\nЗа эти дни в Donut ничего нового.`);
  console.log("Sent: no new upstream commits");
} else {
  await send(`${header}\n\n${await review()}`);
  console.log(`Sent a review of ${commits.length} upstream commits`);
}
