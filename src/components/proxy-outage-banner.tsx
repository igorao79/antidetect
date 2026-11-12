"use client";

import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { LuUnplug } from "react-icons/lu";
import { Button } from "@/components/ui/button";
import { outageReason } from "@/hooks/use-proxy-outages";
import type { OutageRow } from "@/lib/proxy-outages";

type Props = {
  rows: readonly OutageRow[];
  onOpen: (profileId: string) => void;
  onStop: (profileId: string) => Promise<void>;
};

function OutageItem({
  row,
  onOpen,
  onStop,
}: {
  row: OutageRow;
  onOpen: Props["onOpen"];
  onStop: Props["onStop"];
}) {
  const { t } = useTranslation();
  const nameId = useId();
  const [isStopping, setIsStopping] = useState(false);
  const note = row.cut ? t("proxyOutage.cut") : t("proxyOutage.waiting");

  const handleStop = async () => {
    setIsStopping(true);
    try {
      await onStop(row.profileId);
    } catch {
      // The stop handler already reported the failure.
    } finally {
      setIsStopping(false);
    }
  };

  return (
    <li
      data-testid="proxy-outage-row"
      data-profile-id={row.profileId}
      className="flex flex-wrap items-center gap-x-3 gap-y-1"
    >
      <p className="min-w-0 flex-1 text-xs">
        <span id={nameId} className="font-medium text-foreground">
          {row.name}
        </span>
        <span className="text-muted-foreground">
          {" · "}
          {outageReason(t, row.code)} {note}
        </span>
      </p>
      <div className="flex shrink-0 gap-1">
        <Button
          type="button"
          size="sm"
          variant="subtle"
          className="h-7"
          aria-describedby={nameId}
          onClick={() => onOpen(row.profileId)}
        >
          {t("profileTable.menu.openDetails")}
        </Button>
        <Button
          type="button"
          size="sm"
          variant="soft"
          className="h-7 text-destructive-text"
          aria-describedby={nameId}
          disabled={isStopping}
          onClick={() => void handleStop()}
        >
          {t("common.buttons.stop")}
        </Button>
      </div>
    </li>
  );
}

/**
 * Running profiles whose proxy stopped answering, above the page content.
 * Never modal: it takes its own height and leaves when the last profile
 * recovers, stops or is gone. The live region stays mounted, so a screen
 * reader hears the banner arrive.
 */
export function ProxyOutageBanner({ rows, onOpen, onStop }: Props) {
  const { t } = useTranslation();
  const titleId = useId();
  return (
    <div role="status" aria-live="polite" className="shrink-0">
      {rows.length > 0 && (
        <section
          aria-labelledby={titleId}
          data-testid="proxy-outage-banner"
          className="mx-3 mt-2.5 rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2"
        >
          <h2
            id={titleId}
            className="flex items-center gap-2 text-sm font-medium text-destructive-text"
          >
            <LuUnplug aria-hidden="true" className="size-4 shrink-0" />
            {t("proxyOutage.bannerTitle", { count: rows.length })}
          </h2>
          <ul className="mt-1.5 max-h-36 space-y-1 overflow-y-auto">
            {rows.map((row) => (
              <OutageItem
                key={row.profileId}
                row={row}
                onOpen={onOpen}
                onStop={onStop}
              />
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}
