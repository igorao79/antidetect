"use client";

import { listen } from "@tauri-apps/api/event";
import type { TFunction } from "i18next";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { translateBackendError } from "@/lib/backend-errors";
import {
  applyHealthEvent,
  NOTIFY_GROUP_WINDOW_MS,
  type OutageNotice,
  type OutageRow,
  type Outages,
  outageNotice,
  outageRows,
  type ProxyHealthEvent,
  pruneStopped,
} from "@/lib/proxy-outages";
import { notifySystem } from "@/lib/system-notify";
import type { BrowserProfile } from "@/types";

/** A health code in the person's language, or the plain "down" label. */
export function outageReason(t: TFunction, code: string | undefined): string {
  return code
    ? translateBackendError(t, JSON.stringify({ code }))
    : t("profileTable.health.down");
}

function noticeText(t: TFunction, notice: OutageNotice) {
  const stopped = t("proxyOutage.trafficStopped");
  if (notice.kind === "one") {
    return {
      title: t("proxyOutage.notifyTitleOne", { name: notice.name }),
      body: `${outageReason(t, notice.code)}\n${stopped}`,
    };
  }
  const listed = notice.names.join(", ");
  const names =
    notice.more > 0
      ? t("proxyOutage.namesMore", { names: listed, more: notice.more })
      : listed;
  return {
    title: t("proxyOutage.notifyTitleMany", { count: notice.count, names }),
    body: notice.code ? `${outageReason(t, notice.code)}\n${stopped}` : stopped,
  };
}

/**
 * Running profiles whose proxy stopped answering, for the banner. A profile
 * that goes down also gets one system notification while the window is in
 * the background; profiles that go down within a few seconds of each other
 * (one proxy behind many profiles) share one.
 */
export function useProxyOutages(
  profiles: readonly BrowserProfile[],
  runningProfiles: ReadonlySet<string>,
): OutageRow[] {
  const { t } = useTranslation();
  const [outages, setOutages] = useState<Outages>(() => new Map());
  const latest = useRef({ t, profiles, runningProfiles, outages });
  latest.current = { t, profiles, runningProfiles, outages };

  useEffect(() => {
    let disposed = false;
    let batch = new Set<string>();
    let timer: ReturnType<typeof setTimeout> | null = null;

    const flush = () => {
      timer = null;
      const current = latest.current;
      const notice = outageNotice(
        batch,
        outageRows(current.outages, current.profiles),
      );
      batch = new Set();
      if (notice) void notifySystem(noticeText(current.t, notice));
    };

    const unlisten = listen<ProxyHealthEvent>(
      "proxy-health-changed",
      (event) => {
        if (disposed) return;
        const payload = event.payload;
        setOutages((current) =>
          applyHealthEvent(current, payload, (id) =>
            latest.current.runningProfiles.has(id),
          ),
        );
        const opensOutage =
          !("cleared" in payload) &&
          payload.status === "down" &&
          !latest.current.outages.has(payload.profile_id);
        if (!opensOutage) return;
        batch = new Set(batch).add(payload.profile_id);
        timer ??= setTimeout(flush, NOTIFY_GROUP_WINDOW_MS);
      },
    );
    return () => {
      disposed = true;
      if (timer) clearTimeout(timer);
      void unlisten.then((stop) => stop());
    };
  }, []);

  const previousRunning = useRef(runningProfiles);
  useEffect(() => {
    const previous = previousRunning.current;
    previousRunning.current = runningProfiles;
    setOutages((current) => pruneStopped(current, previous, runningProfiles));
  }, [runningProfiles]);

  return useMemo(() => outageRows(outages, profiles), [outages, profiles]);
}
