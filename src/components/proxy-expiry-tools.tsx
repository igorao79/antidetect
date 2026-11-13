"use client";

import { useTranslation } from "react-i18next";
import { LuCalendarClock, LuCalendarPlus } from "react-icons/lu";
import { DataTableActionBarAction } from "@/components/data-table-action-bar";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { PROXY_EXPIRY_EXTEND_DAYS } from "@/lib/proxy-expiry";
import { cn } from "@/lib/utils";

interface ProxyExpirySummaryProps {
  count: number;
  days: number;
  active: boolean;
  onToggle: () => void;
}

/** A quiet line above the table; clicking it shows only those proxies. */
export function ProxyExpirySummary({
  count,
  days,
  active,
  onToggle,
}: ProxyExpirySummaryProps) {
  const { t } = useTranslation();
  if (count === 0 && !active) return null;
  return (
    <button
      type="button"
      onClick={onToggle}
      aria-pressed={active}
      className={cn(
        "mb-2 inline-flex w-fit items-center gap-1.5 rounded-md px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-foreground/5 hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring",
        active && "bg-warning/10 text-foreground",
      )}
    >
      <LuCalendarClock className="size-3.5" aria-hidden="true" />
      <span>{t("proxies.expiry.summary", { count, days })}</span>
      <span className="text-muted-foreground">
        {active
          ? t("proxies.expiry.summaryShowAll")
          : t("proxies.expiry.summaryFilter")}
      </span>
    </button>
  );
}

interface ProxyExtendMenuProps {
  onExtend: (days: number) => void;
}

/** The selection bar's "Extend by N days" choice. */
export function ProxyExtendMenu({ onExtend }: ProxyExtendMenuProps) {
  const { t } = useTranslation();
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <DataTableActionBarAction label={t("proxies.expiry.extend")}>
          <LuCalendarPlus />
        </DataTableActionBarAction>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start">
        {PROXY_EXPIRY_EXTEND_DAYS.map((days) => (
          <DropdownMenuItem key={days} onSelect={() => onExtend(days)}>
            {t("proxies.expiry.extendBy", { days })}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
