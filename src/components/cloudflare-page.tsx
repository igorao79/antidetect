"use client";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { LuCircleAlert, LuRefreshCw, LuTrash2 } from "react-icons/lu";
import { Button } from "@/components/ui/button";
import { CopyToClipboard } from "@/components/ui/copy-to-clipboard";
import { Input } from "@/components/ui/input";
import { showErrorToast, showSuccessToast } from "@/lib/toast-utils";

interface CloudflareSettings {
  mailhubKey: string;
  hasSecret: boolean;
  proxyUuid: string | null;
  scriptPath: string | null;
  country: string | null;
  ttlMinutes: number | null;
  domain: string | null;
}

interface CloudflareAccount {
  id: string;
  email: string;
  password: string;
  status: "queued" | "running" | "verified" | "token" | "failed" | string;
  apiKey?: string | null;
  apiToken?: string | null;
  accountId?: string | null;
  proxyId?: string | null;
  error?: string | null;
  steps?: string[];
  createdAt: number;
  finishedAt?: number | null;
}

const STATUS_KEY: Record<string, string> = {
  queued: "cloudflare.status.queued",
  running: "cloudflare.status.running",
  mailbox: "cloudflare.status.mailbox",
  proxy: "cloudflare.status.proxy",
  profile: "cloudflare.status.profile",
  browser: "cloudflare.status.browser",
  signup: "cloudflare.status.signup",
  submitted: "cloudflare.status.submitted",
  email: "cloudflare.status.email",
  token: "cloudflare.status.token",
  verified: "cloudflare.status.verified",
  failed: "cloudflare.status.failed",
};

export function CloudflarePage() {
  const { t } = useTranslation();
  const [settings, setSettings] = useState<CloudflareSettings | null>(null);
  const [accounts, setAccounts] = useState<CloudflareAccount[]>([]);
  const [running, setRunning] = useState(false);
  const [progress, setProgress] = useState<Record<string, string>>({});
  const [form, setForm] = useState({
    mailhubKey: "",
    mailhubSecret: "",
    domain: "",
    country: "",
    ttlMinutes: "",
    proxyUuid: "",
  });

  const load = useCallback(async () => {
    try {
      const [nextSettings, nextAccounts] = await Promise.all([
        invoke<CloudflareSettings>("cloudflare_settings"),
        invoke<CloudflareAccount[]>("cloudflare_accounts"),
      ]);
      setSettings(nextSettings);
      setAccounts(nextAccounts);
      setForm((current) => ({
        ...current,
        mailhubKey: nextSettings.mailhubKey ?? "",
        domain: nextSettings.domain ?? "",
        country: nextSettings.country ?? "",
        ttlMinutes: nextSettings.ttlMinutes
          ? String(nextSettings.ttlMinutes)
          : "",
        proxyUuid: nextSettings.proxyUuid ?? "",
      }));
    } catch (error) {
      showErrorToast(String(error));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    const unlisten = listen<{ id: string; status: string; line: string }>(
      "cloudflare-progress",
      (event) => {
        setProgress((current) => ({
          ...current,
          [event.payload.id]: event.payload.line,
        }));
        setAccounts((current) =>
          current.map((account) =>
            account.id === event.payload.id
              ? { ...account, status: event.payload.status }
              : account,
          ),
        );
      },
    );
    return () => {
      void unlisten.then((off) => off());
    };
  }, []);

  const save = useCallback(async () => {
    try {
      const next = await invoke<CloudflareSettings>(
        "cloudflare_save_settings",
        {
          mailhubKey: form.mailhubKey,
          mailhubSecret: form.mailhubSecret,
          proxyUuid: form.proxyUuid,
          country: form.country,
          ttlMinutes: form.ttlMinutes ? Number(form.ttlMinutes) : null,
          domain: form.domain,
        },
      );
      setSettings(next);
      setForm((current) => ({ ...current, mailhubSecret: "" }));
      showSuccessToast(t("cloudflare.saved"));
    } catch (error) {
      showErrorToast(String(error));
    }
  }, [form, t]);

  const generate = useCallback(async () => {
    setRunning(true);
    try {
      const account = await invoke<CloudflareAccount>("cloudflare_generate");
      setAccounts((current) => [
        account,
        ...current.filter((entry) => entry.id !== account.id),
      ]);
      if (account.status === "failed")
        showErrorToast(account.error ?? t("cloudflare.status.failed"));
      else showSuccessToast(`${account.email}`);
    } catch (error) {
      showErrorToast(String(error));
    } finally {
      setRunning(false);
      void load();
    }
  }, [load, t]);

  const remove = useCallback(async (id: string) => {
    try {
      await invoke("cloudflare_delete_account", { id });
      setAccounts((current) => current.filter((account) => account.id !== id));
    } catch (error) {
      showErrorToast(String(error));
    }
  }, []);

  const canGenerate = Boolean(settings?.mailhubKey) && settings?.hasSecret;

  return (
    <div className="mx-auto flex w-full max-w-4xl flex-col gap-6 p-6">
      <div className="flex items-center justify-between gap-4">
        <div>
          <h1 className="text-xl font-semibold">{t("cloudflare.title")}</h1>
          <p className="text-sm text-muted-foreground">
            {t("cloudflare.subtitle")}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={() => void load()}>
            <LuRefreshCw className="size-4" />
          </Button>
          <Button
            disabled={!canGenerate || running}
            onClick={() => void generate()}
          >
            {running ? t("cloudflare.running") : t("cloudflare.generate")}
          </Button>
        </div>
      </div>

      <section className="rounded-lg border bg-card p-4">
        <h2 className="mb-3 text-sm font-medium">{t("cloudflare.settings")}</h2>
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          <div className="flex flex-col gap-1 text-xs">
            {t("cloudflare.mailhubKey")}
            <Input
              value={form.mailhubKey}
              placeholder="mhk_…"
              onChange={(event) =>
                setForm({ ...form, mailhubKey: event.target.value })
              }
            />
          </div>
          <div className="flex flex-col gap-1 text-xs">
            {t("cloudflare.mailhubSecret")}
            <Input
              type="password"
              value={form.mailhubSecret}
              placeholder={settings?.hasSecret ? "••••••••" : ""}
              onChange={(event) =>
                setForm({ ...form, mailhubSecret: event.target.value })
              }
            />
          </div>
          <div className="flex flex-col gap-1 text-xs">
            {t("cloudflare.domain")}
            <Input
              value={form.domain}
              placeholder="buildlog.vip"
              onChange={(event) =>
                setForm({ ...form, domain: event.target.value })
              }
            />
          </div>
          <div className="flex flex-col gap-1 text-xs">
            {t("cloudflare.country")}
            <Input
              value={form.country}
              placeholder="de"
              onChange={(event) =>
                setForm({ ...form, country: event.target.value })
              }
            />
          </div>
          <div className="flex flex-col gap-1 text-xs">
            {t("cloudflare.ttl")}
            <Input
              value={form.ttlMinutes}
              placeholder="120"
              onChange={(event) =>
                setForm({ ...form, ttlMinutes: event.target.value })
              }
            />
          </div>
          <div className="flex flex-col gap-1 text-xs">
            {t("cloudflare.proxy")}
            <Input
              value={form.proxyUuid}
              placeholder={t("cloudflare.proxyHint")}
              onChange={(event) =>
                setForm({ ...form, proxyUuid: event.target.value })
              }
            />
          </div>
        </div>
        <div className="mt-3 flex items-center justify-between gap-3">
          {!canGenerate && !running && (
            <span className="flex items-center gap-1 text-xs text-muted-foreground">
              <LuCircleAlert className="size-3.5" />
              {t("cloudflare.needMailhub")}
            </span>
          )}
          <Button variant="outline" size="sm" onClick={() => void save()}>
            {t("cloudflare.save")}
          </Button>
        </div>
      </section>

      <section className="flex flex-col gap-2">
        {accounts.length === 0 && (
          <p className="text-sm text-muted-foreground">
            {t("cloudflare.empty")}
          </p>
        )}
        {accounts.map((account) => (
          <div key={account.id} className="rounded-lg border bg-card p-4">
            <div className="flex items-center justify-between gap-3">
              <div className="min-w-0">
                <div className="flex items-center gap-2">
                  <span className="truncate font-mono text-sm">
                    {account.email || account.id.slice(0, 8)}
                  </span>
                  <span className="rounded bg-muted px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-muted-foreground">
                    {t(
                      STATUS_KEY[account.status] ?? "cloudflare.status.queued",
                    )}
                  </span>
                </div>
                {progress[account.id] && (
                  <p className="mt-1 truncate font-mono text-[11px] text-muted-foreground">
                    {progress[account.id]}
                  </p>
                )}
                {account.error && (
                  <p className="mt-1 text-xs text-destructive">
                    {account.error}
                  </p>
                )}
              </div>
              <div className="flex shrink-0 items-center gap-1">
                <Button
                  variant="ghost"
                  size="icon"
                  aria-label={t("cloudflare.delete")}
                  onClick={() => void remove(account.id)}
                >
                  <LuTrash2 className="size-4" />
                </Button>
              </div>
            </div>
            {account.email && (
              <dl className="mt-3 grid grid-cols-1 gap-2 text-xs md:grid-cols-2">
                <div className="flex items-center gap-2">
                  <dt className="w-20 shrink-0 text-muted-foreground">
                    {t("cloudflare.password")}
                  </dt>
                  <dd className="flex min-w-0 items-center gap-1">
                    <code className="truncate">{account.password}</code>
                    <CopyToClipboard text={account.password} />
                  </dd>
                </div>
                {account.apiKey && (
                  <div className="flex items-center gap-2">
                    <dt className="w-20 shrink-0 text-muted-foreground">
                      {t("cloudflare.apiKey")}
                    </dt>
                    <dd className="flex min-w-0 items-center gap-1">
                      <code className="truncate">{account.apiKey}</code>
                      <CopyToClipboard text={account.apiKey} />
                    </dd>
                  </div>
                )}
                {account.apiToken && (
                  <div className="flex items-center gap-2">
                    <dt className="w-20 shrink-0 text-muted-foreground">
                      {t("cloudflare.apiToken")}
                    </dt>
                    <dd className="flex min-w-0 items-center gap-1">
                      <code className="truncate">{account.apiToken}</code>
                      <CopyToClipboard text={account.apiToken} />
                    </dd>
                  </div>
                )}
              </dl>
            )}
          </div>
        ))}
      </section>
    </div>
  );
}
