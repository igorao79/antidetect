"use client";

import type * as React from "react";
import { useTranslation } from "react-i18next";
import { LuLoaderCircle, LuRadar } from "react-icons/lu";
import { toast } from "sonner";
import { FlagIcon } from "@/components/flag-icon";
import { translateBackendError } from "@/lib/backend-errors";
import { runProxyCheck, useProxyCheck } from "@/lib/proxy-check-store";
import { cn } from "@/lib/utils";
import type { StoredProxy } from "@/types";

interface ProxyChoiceLabelProps {
  proxy: StoredProxy;
  /** Marks the route the profile uses now. */
  current?: boolean;
}

const CHECK_SELECTOR =
  '[cmdk-item][data-selected="true"] [data-slot="proxy-choice-check"]';

/**
 * ArrowRight on a picker's highlighted proxy runs its Check without closing
 * the list. In the search field it only counts at the end of the text, so
 * the caret keeps moving there.
 */
export function checkHighlightedProxy(event: React.KeyboardEvent): void {
  if (event.key !== "ArrowRight") return;
  if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
  const target = event.target;
  if (
    target instanceof HTMLInputElement &&
    target.selectionStart !== target.value.length
  ) {
    return;
  }
  const button = event.currentTarget.querySelector<HTMLElement>(CHECK_SELECTOR);
  if (!button || button.getAttribute("aria-disabled") === "true") return;
  event.preventDefault();
  button.click();
}

/**
 * A proxy's row in a picker: its name, a "current" mark, and a Check action
 * that answers with the exit IP, country and timezone before anything is
 * assigned. It reads the shared check receipt, so a check run here also
 * updates the profile table and the Network page.
 */
export function ProxyChoiceLabel({ proxy, current }: ProxyChoiceLabelProps) {
  const { t } = useTranslation();
  const { result, checking, failure } = useProxyCheck(proxy);

  const check = async () => {
    try {
      await runProxyCheck(proxy, (error) => translateBackendError(t, error));
    } catch (error) {
      toast.error(
        t("proxyCheck.failed", { error: translateBackendError(t, error) }),
      );
    }
  };

  const answered = result?.is_valid ? result : null;
  const summary = answered
    ? [answered.ip, answered.country, answered.timezone]
        .filter(Boolean)
        .join(" · ")
    : null;

  return (
    <span className="flex min-w-0 flex-1 items-center gap-2">
      <span className="flex min-w-0 flex-1 flex-col">
        <span className="flex min-w-0 items-center gap-1.5">
          <span className="truncate">{proxy.name}</span>
          {current && (
            <span className="shrink-0 rounded-full bg-primary/10 px-1.5 text-[10px] leading-4 text-primary">
              {t("proxyPicker.current")}
            </span>
          )}
        </span>
        {(summary || failure) && (
          <span
            data-slot="proxy-choice-summary"
            className={cn(
              "flex min-w-0 items-center gap-1 truncate text-[11px] tabular-nums",
              failure ? "text-destructive-text" : "text-muted-foreground",
            )}
          >
            {answered && (
              <FlagIcon
                countryCode={answered.country_code}
                className="mr-0.5"
              />
            )}
            <span className="truncate">{failure ?? summary}</span>
          </span>
        )}
      </span>
      <span
        role="button"
        tabIndex={-1}
        data-slot="proxy-choice-check"
        aria-label={t("proxyPicker.checkNamed", { name: proxy.name })}
        aria-disabled={checking}
        aria-keyshortcuts="ArrowRight"
        title={t("proxyCheck.tooltipDefault")}
        className={cn(
          "ml-auto flex h-6 shrink-0 cursor-pointer items-center gap-1 rounded-md px-1.5 text-[11px] text-muted-foreground transition-colors hover:bg-foreground/5 hover:text-foreground",
          checking && "pointer-events-none",
        )}
        onPointerDown={(event) => event.stopPropagation()}
        onClick={(event) => {
          event.stopPropagation();
          void check();
        }}
        onKeyDown={(event) => {
          if (event.key !== "Enter" && event.key !== " ") return;
          event.preventDefault();
          event.stopPropagation();
          void check();
        }}
      >
        {checking ? (
          <LuLoaderCircle className="size-3 animate-spin" aria-hidden="true" />
        ) : (
          <LuRadar className="size-3" aria-hidden="true" />
        )}
        {checking ? t("proxyPicker.checking") : t("proxyPicker.check")}
        {!checking && (
          <kbd className="hidden rounded bg-foreground/5 px-1 font-sans text-[10px] [[data-selected=true]_&]:inline">
            →
          </kbd>
        )}
      </span>
    </span>
  );
}
