import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { RELEASE_FAILED_EVENT } from "@/lib/proxy-generator";

const NOTICE_ID = "proxy-generator-release-failed";

/**
 * Says so when erasing a generated proxy could not free its port in the
 * proxy generator (fork): the port stays taken there until it expires or is
 * deleted in the bot.
 */
export function useProxyGeneratorReleaseNotices() {
  const { t } = useTranslation();
  useEffect(() => {
    const unlisten = listen(RELEASE_FAILED_EVENT, () => {
      toast.warning(t("proxyGenerator.releaseFailed"), { id: NOTICE_ID });
    });
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, [t]);
}
