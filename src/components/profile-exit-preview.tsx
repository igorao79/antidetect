"use client";

import { useTranslation } from "react-i18next";
import { LuLoaderCircle, LuRadar } from "react-icons/lu";
import { toast } from "sonner";
import { FlagIcon } from "@/components/flag-icon";
import { Button } from "@/components/ui/button";
import { translateBackendError } from "@/lib/backend-errors";
import { runProxyCheck, useProxyCheck } from "@/lib/proxy-check-store";
import { likelyLanguage } from "@/lib/proxy-choices";
import type { StoredProxy } from "@/types";

function languageName(code: string | null, uiLanguage: string): string | null {
  if (!code) return null;
  try {
    return (
      new Intl.DisplayNames([uiLanguage], { type: "language" }).of(code) ?? code
    );
  } catch {
    return code;
  }
}

/**
 * Where a profile will appear from, read from its proxy's last check, so the
 * answer is there before launch. Without a usable receipt it offers the check.
 */
export function ProfileExitPreview({ proxy }: { proxy: StoredProxy }) {
  const { t, i18n } = useTranslation();
  const { result, checking, failure } = useProxyCheck(proxy);
  const answered = result?.is_valid ? result : null;

  const check = async () => {
    try {
      await runProxyCheck(proxy, (error) => translateBackendError(t, error));
    } catch (error) {
      toast.error(
        t("proxyCheck.failed", { error: translateBackendError(t, error) }),
      );
    }
  };

  const rows = answered
    ? [
        {
          label: t("profileExit.country"),
          value: answered.country ?? t("proxyCheck.unknownLocation"),
          flag: answered.country_code,
        },
        answered.city
          ? { label: t("profileExit.city"), value: answered.city }
          : null,
        answered.timezone
          ? { label: t("profileExit.timezone"), value: answered.timezone }
          : null,
        {
          label: t("profileExit.language"),
          value:
            languageName(
              likelyLanguage(answered.country_code),
              i18n.language,
            ) ?? t("proxyCheck.unknownLocation"),
        },
      ].filter((row) => row !== null)
    : [];

  return (
    <div
      data-slot="profile-exit-preview"
      className="flex flex-col gap-2 rounded-lg bg-foreground/5 px-3 py-2.5"
    >
      <div className="flex items-center gap-2">
        <span className="text-xs font-medium">{t("profileExit.title")}</span>
        <Button
          type="button"
          variant="soft"
          size="sm"
          className="ml-auto h-7 gap-1.5 px-2 text-xs"
          data-slot="profile-exit-check"
          disabled={checking}
          onClick={() => void check()}
        >
          {checking ? (
            <LuLoaderCircle className="size-3.5 animate-spin" />
          ) : (
            <LuRadar className="size-3.5" />
          )}
          {answered ? t("profileExit.recheck") : t("proxyPicker.check")}
        </Button>
      </div>
      {answered ? (
        <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs">
          {rows.map((row) => (
            <div key={row.label} className="contents">
              <dt className="text-muted-foreground">{row.label}</dt>
              <dd className="flex min-w-0 items-center gap-1.5 tabular-nums">
                {row.flag && <FlagIcon countryCode={row.flag} />}
                <span className="truncate">{row.value}</span>
              </dd>
            </div>
          ))}
        </dl>
      ) : (
        <p className="text-xs text-muted-foreground">
          {failure ?? t("profileExit.notChecked")}
        </p>
      )}
    </div>
  );
}
