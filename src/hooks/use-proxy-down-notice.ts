"use client";

import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import type { ProxyHealth } from "@/hooks/use-proxy-health";
import { translateBackendError } from "@/lib/backend-errors";
import { dismissToast, showErrorToast } from "@/lib/toast-utils";
import type { BrowserProfile } from "@/types";

type ProxyHealthEvent = ProxyHealth | { profile_id: string; cleared: true };

const toastId = (profileId: string) => `proxy-down-${profileId}`;

/**
 * One toast when a running profile's route goes down, naming the profile.
 * Further "down" verdicts of the same outage say nothing; recovery or a stop
 * ends the outage and takes its toast away.
 */
export function useProxyDownNotice(profiles: readonly BrowserProfile[]) {
  const { t } = useTranslation();
  const latest = useRef({ t, profiles });
  latest.current = { t, profiles };

  useEffect(() => {
    let disposed = false;
    const down = new Set<string>();
    const unlisten = listen<ProxyHealthEvent>(
      "proxy-health-changed",
      (event) => {
        if (disposed) return;
        const { profile_id } = event.payload;
        const isDown =
          !("cleared" in event.payload) && event.payload.status === "down";
        if (!isDown) {
          if (down.delete(profile_id)) dismissToast(toastId(profile_id));
          return;
        }
        if (down.has(profile_id)) return;
        down.add(profile_id);

        const { t: translate, profiles: known } = latest.current;
        const name =
          known.find((profile) => profile.id === profile_id)?.name ??
          translate("common.labels.unknownProfile");
        const code = "code" in event.payload ? event.payload.code : undefined;
        showErrorToast(translate("profileTable.health.downToast", { name }), {
          id: toastId(profile_id),
          description: code
            ? translateBackendError(translate, JSON.stringify({ code }))
            : undefined,
        });
      },
    );
    return () => {
      disposed = true;
      void unlisten.then((stop) => stop());
    };
  }, []);
}
