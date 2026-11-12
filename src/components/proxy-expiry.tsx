"use client";

import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";
import { LuCalendarClock, LuX } from "react-icons/lu";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import {
  dateInputDaysFromToday,
  expiryFromDateInput,
  formatExpiryDate,
  PROXY_EXPIRY_PRESET_DAYS,
  type ProxyExpiryLevel,
  type ProxyExpiryStatus,
  proxyExpiryStatus,
} from "@/lib/proxy-expiry";
import { cn } from "@/lib/utils";

const LEVEL_TEXT: Record<ProxyExpiryLevel, string> = {
  ok: "text-muted-foreground",
  soon: "text-warning-text",
  expired: "text-destructive-text",
};

/** "Proxy expired 12.11" / "Proxy expires in 2 days", for a profile's proxy. */
function describeForProfile(
  t: TFunction,
  status: ProxyExpiryStatus,
  locale: string,
): string {
  if (status.level === "expired") {
    return t("proxies.expiry.proxyExpiredOn", {
      date: formatExpiryDate(status.date, locale),
    });
  }
  if (status.daysLeft <= 0) return t("proxies.expiry.proxyEndsToday");
  return t("proxies.expiry.proxyEndsIn", { count: status.daysLeft });
}

/** "3 days left" / "Ends today" / "Expired 12.11", for the proxy itself. */
function describeRemaining(
  t: TFunction,
  status: ProxyExpiryStatus,
  locale: string,
): string {
  if (status.level === "expired") {
    return t("proxies.expiry.expiredOn", {
      date: formatExpiryDate(status.date, locale),
    });
  }
  if (status.daysLeft <= 0) return t("proxies.expiry.endsToday");
  return t("proxies.expiry.daysLeft", { count: status.daysLeft });
}

/**
 * A small calendar mark beside a profile's proxy when that proxy has expired
 * or ends within the warning window; nothing otherwise.
 */
export function ProxyExpiryMarker({
  expiresAt,
}: {
  expiresAt: number | null | undefined;
}) {
  const { t, i18n } = useTranslation();
  const status = proxyExpiryStatus(expiresAt);
  if (!status || status.level === "ok") return null;
  const text = describeForProfile(t, status, i18n.language);
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          role="img"
          aria-label={text}
          data-proxy-expiry={status.level}
          className={cn(
            "inline-flex size-5 shrink-0 items-center justify-center rounded-md",
            status.level === "expired"
              ? "bg-destructive/10 text-destructive-text"
              : "bg-warning/10 text-warning-text",
          )}
        >
          <LuCalendarClock className="size-3.5" aria-hidden="true" />
        </span>
      </TooltipTrigger>
      <TooltipContent>{text}</TooltipContent>
    </Tooltip>
  );
}

/** The Network page's "Expires" cell: the date, coloured once it is close. */
export function ProxyExpiryCell({
  expiresAt,
}: {
  expiresAt: number | null | undefined;
}) {
  const { t, i18n } = useTranslation();
  const status = proxyExpiryStatus(expiresAt);
  if (!status) {
    return <span className="text-xs text-muted-foreground">—</span>;
  }
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          data-proxy-expiry={status.level}
          className={cn(
            "block truncate text-xs tabular-nums",
            LEVEL_TEXT[status.level],
          )}
        >
          {formatExpiryDate(status.date, i18n.language)}
        </span>
      </TooltipTrigger>
      <TooltipContent>
        {describeRemaining(t, status, i18n.language)}
      </TooltipContent>
    </Tooltip>
  );
}

/**
 * The proxy form's optional expiry: a date, quick presets counted from today,
 * a clear button and how much time is left.
 */
export function ProxyExpiryField({
  value,
  onChange,
  disabled,
}: {
  /** The date input's `YYYY-MM-DD`, or "" for none. */
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
}) {
  const { t, i18n } = useTranslation();
  const status = proxyExpiryStatus(expiryFromDateInput(value));
  return (
    <div className="grid gap-2">
      <Label htmlFor="proxy-expires-at">{t("proxies.expiry.label")}</Label>
      <div className="flex items-center gap-2">
        <Input
          id="proxy-expires-at"
          type="date"
          value={value}
          onChange={(e) => {
            onChange(e.target.value);
          }}
          disabled={disabled}
          aria-describedby="proxy-expires-at-hint"
          className="w-auto flex-1"
        />
        {value && (
          <Button
            type="button"
            variant="subtle"
            size="icon"
            onClick={() => {
              onChange("");
            }}
            disabled={disabled}
            aria-label={t("common.buttons.clear")}
          >
            <LuX aria-hidden="true" />
          </Button>
        )}
      </div>
      <div className="flex flex-wrap gap-1.5">
        {PROXY_EXPIRY_PRESET_DAYS.map((days) => (
          <Button
            key={days}
            type="button"
            variant="soft"
            size="sm"
            className="h-7 px-2.5 text-xs tabular-nums"
            onClick={() => {
              onChange(dateInputDaysFromToday(days));
            }}
            disabled={disabled}
            aria-label={t("proxies.expiry.presetAria", { days })}
          >
            {t("proxies.expiry.presetDays", { days })}
          </Button>
        ))}
      </div>
      <p
        id="proxy-expires-at-hint"
        className={cn(
          "text-xs",
          status ? LEVEL_TEXT[status.level] : "text-muted-foreground",
        )}
      >
        {status
          ? describeRemaining(t, status, i18n.language)
          : t("proxies.expiry.hint")}
      </p>
    </div>
  );
}
