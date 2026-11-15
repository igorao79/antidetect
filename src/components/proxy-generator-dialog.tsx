"use client";

import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { LuCopy, LuKeyRound, LuLoaderCircle } from "react-icons/lu";
import { toast } from "sonner";
import { LoadingButton } from "@/components/loading-button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { translateBackendError } from "@/lib/backend-errors";
import { runPool } from "@/lib/bulk-proxy-check";
import { runProxyCheck } from "@/lib/proxy-check-store";
import {
  generateProxies,
  getProxyGeneratorOptions,
  getProxyGeneratorStatus,
  type ProxyGeneratorStatus,
} from "@/lib/proxy-generator";
import {
  DEFAULT_FORM,
  formProblem,
  type GeneratorFormState,
  type ProxyGeneratorOptions,
  toRequest,
} from "@/lib/proxy-generator-form";
import { formatProxyUri } from "@/lib/proxy-uri";
import type { StoredProxy } from "@/types";
import { ProxyCheckButton } from "./proxy-check-button";
import { ProxyGeneratorFields } from "./proxy-generator-fields";
import { ProxyGeneratorKeyStep } from "./proxy-generator-key-step";
import { RippleButton } from "./ui/ripple";

const CHECK_LANES = 4;

type Phase = "loading" | "key" | "form" | "done";

interface Props {
  isOpen: boolean;
  onClose: () => void;
  /** One proxy for a picker: no count, closes as soon as it is made. */
  single?: boolean;
  onGenerated?: (proxies: StoredProxy[]) => void;
}

function GeneratedList({ proxies }: { proxies: StoredProxy[] }) {
  return (
    <ul className="grid max-h-72 gap-1 overflow-y-auto">
      {proxies.map((proxy) => (
        <li
          key={proxy.id}
          className="flex items-center gap-3 rounded-md px-2 py-1.5 hover:bg-foreground/5"
        >
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-medium">{proxy.name}</p>
            <p className="truncate font-mono text-xs text-muted-foreground">
              {proxy.proxy_settings.host}:{proxy.proxy_settings.port}
              {proxy.note ? ` · ${proxy.note}` : ""}
            </p>
          </div>
          <ProxyCheckButton proxy={proxy} variant="status" />
        </li>
      ))}
    </ul>
  );
}

/** Generates proxies with the team's proxy generator (fork). */
export function ProxyGeneratorDialog({
  isOpen,
  onClose,
  single = false,
  onGenerated,
}: Props) {
  const { t } = useTranslation();
  const [phase, setPhase] = useState<Phase>("loading");
  const [status, setStatus] = useState<ProxyGeneratorStatus | null>(null);
  const [options, setOptions] = useState<ProxyGeneratorOptions | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [form, setForm] = useState<GeneratorFormState>(DEFAULT_FORM);
  const [generated, setGenerated] = useState<StoredProxy[]>([]);
  const [submitting, setSubmitting] = useState(false);

  const loadOptions = useCallback(async () => {
    setLoadError(null);
    setOptions(null);
    try {
      setOptions(await getProxyGeneratorOptions());
    } catch (e) {
      setLoadError(translateBackendError(t, e));
    }
  }, [t]);

  const showForm = useCallback(
    (next: ProxyGeneratorStatus) => {
      setStatus(next);
      setPhase("form");
      void loadOptions();
    },
    [loadOptions],
  );

  useEffect(() => {
    if (!isOpen) return;
    let cancelled = false;
    setPhase("loading");
    setForm(DEFAULT_FORM);
    setGenerated([]);
    getProxyGeneratorStatus()
      .then((current) => {
        if (cancelled) return;
        if (current.configured) showForm(current);
        else {
          setStatus(current);
          setPhase("key");
        }
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        setLoadError(translateBackendError(t, e));
        setPhase("key");
      });
    return () => {
      cancelled = true;
    };
  }, [isOpen, showForm, t]);

  const problem = options ? formProblem(form, options) : null;
  // A lifetime not typed yet keeps Generate off without a warning.
  const showProblem =
    problem !== null &&
    !(form.ttl.kind === "custom" && form.ttl.amount.trim() === "");

  const submit = async () => {
    if (!options || problem || submitting) return;
    setSubmitting(true);
    try {
      const made = await generateProxies(
        toRequest(single ? { ...form, count: 1 } : form),
      );
      toast.success(t("proxyGenerator.done.toast", { count: made.length }));
      onGenerated?.(made);
      void runPool(made, CHECK_LANES, async (proxy) => {
        await runProxyCheck(proxy, (error) => translateBackendError(t, error));
      });
      if (single) {
        onClose();
        return;
      }
      setGenerated(made);
      setPhase("done");
    } catch (e) {
      toast.error(translateBackendError(t, e));
    } finally {
      setSubmitting(false);
    }
  };

  const copyAll = async () => {
    const text = generated
      .map((proxy) => formatProxyUri(proxy.proxy_settings))
      .join("\n");
    try {
      await navigator.clipboard.writeText(text);
      toast.success(t("proxyGenerator.done.copied"));
    } catch {
      toast.error(t("toasts.error.copyFailed"));
    }
  };

  const handleOpenChange = (open: boolean) => {
    if (!open && !submitting) onClose();
  };

  return (
    <Dialog open={isOpen} onOpenChange={handleOpenChange}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>{t("proxyGenerator.title")}</DialogTitle>
          <DialogDescription>
            {phase === "key"
              ? t("proxyGenerator.key.description")
              : t("proxyGenerator.description")}
          </DialogDescription>
        </DialogHeader>

        {phase === "loading" && (
          <div className="flex justify-center py-10">
            <LuLoaderCircle className="size-5 animate-spin text-muted-foreground" />
          </div>
        )}

        {phase === "key" && (
          <ProxyGeneratorKeyStep
            status={status}
            onSaved={showForm}
            onForgotten={() => {
              setStatus({ configured: false, key_hint: null });
              setOptions(null);
            }}
            onCancel={() => (status?.configured ? setPhase("form") : onClose())}
          />
        )}

        {phase === "form" && (
          <form
            className="grid gap-4"
            onSubmit={(event) => {
              event.preventDefault();
              void submit();
            }}
          >
            {options ? (
              <ProxyGeneratorFields
                options={options}
                form={form}
                single={single}
                disabled={submitting}
                onChange={setForm}
              />
            ) : loadError ? (
              <div
                role="alert"
                className="grid gap-3 rounded-lg bg-destructive/10 p-3"
              >
                <p className="text-sm text-destructive">{loadError}</p>
                <RippleButton
                  type="button"
                  size="sm"
                  variant="soft"
                  className="w-fit"
                  onClick={() => void loadOptions()}
                >
                  {t("common.buttons.retry")}
                </RippleButton>
              </div>
            ) : (
              <div className="flex justify-center py-10">
                <LuLoaderCircle className="size-5 animate-spin text-muted-foreground" />
              </div>
            )}
            {showProblem && options && (
              <p className="text-xs text-warning-text">
                {t(problem, { max: options.bulk_max })}
              </p>
            )}
            <DialogFooter className="gap-2 sm:justify-between">
              <RippleButton
                type="button"
                variant="ghost"
                className="gap-1.5"
                onClick={() => setPhase("key")}
                disabled={submitting}
              >
                <LuKeyRound className="size-3.5" />
                {t("proxyGenerator.key.change")}
              </RippleButton>
              <div className="flex gap-2">
                <RippleButton
                  type="button"
                  variant="outline"
                  onClick={onClose}
                  disabled={submitting}
                >
                  {t("common.buttons.cancel")}
                </RippleButton>
                <LoadingButton
                  type="submit"
                  isLoading={submitting}
                  disabled={!options || problem !== null}
                >
                  {t("proxyGenerator.generate")}
                </LoadingButton>
              </div>
            </DialogFooter>
          </form>
        )}

        {phase === "done" && (
          <div className="grid gap-4">
            <GeneratedList proxies={generated} />
            <DialogFooter className="gap-2 sm:justify-between">
              <RippleButton
                variant="soft"
                className="gap-1.5"
                onClick={() => void copyAll()}
              >
                <LuCopy className="size-3.5" />
                {t("proxyGenerator.done.copyAll")}
              </RippleButton>
              <div className="flex gap-2">
                <RippleButton
                  variant="outline"
                  onClick={() => setPhase("form")}
                >
                  {t("proxyGenerator.done.more")}
                </RippleButton>
                <RippleButton onClick={onClose}>
                  {t("proxyGenerator.done.finish")}
                </RippleButton>
              </div>
            </DialogFooter>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
