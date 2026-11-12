"use client";

import { openUrl } from "@tauri-apps/plugin-opener";
import { useTranslation } from "react-i18next";
import { LuCircleArrowUp, LuLoaderCircle, LuRefreshCw } from "react-icons/lu";
import type { AppUpdateState } from "@/hooks/use-app-update-notifications";
import { cn } from "@/lib/utils";
import { RAIL_BUTTON, RAIL_LABEL_CLASS } from "./rail-nav";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";

/**
 * The update as one button in the rail (fork), there for as long as an update
 * is known, also after its toast was put off: it starts the download, shows
 * how far it got, and restarts into the new version.
 */
export function RailUpdateButton({ update }: { update: AppUpdateState }) {
  const { t } = useTranslation();
  const info = update.updateInfo;
  if (!info || info.repo_update) return null;

  const version = info.new_version;
  const percent = update.updateProgress?.percentage;
  const downloading = update.isUpdating && !update.updateReady;
  const stage = update.updateProgress?.stage;
  const installing = downloading && !!stage && stage !== "downloading";
  const shownPercent =
    downloading && !installing && typeof percent === "number"
      ? Math.round(percent)
      : null;

  const label = update.updateReady
    ? t("appUpdate.toast.restartNow")
    : installing
      ? t("appUpdate.toast.installing")
      : shownPercent !== null
        ? t("appUpdate.toast.downloadingPercent", { percent: shownPercent })
        : downloading
          ? t("appUpdate.toast.downloading")
          : info.manual_update_required
            ? t("about.update.download")
            : t("about.update.update");
  const hint = update.updateReady
    ? t("about.update.ready", { version })
    : installing
      ? t("about.update.installing", { version })
      : !downloading
        ? t("about.update.available", { version })
        : typeof percent === "number"
          ? t("about.update.downloadingPercent", {
              version,
              percent: Math.round(percent),
            })
          : t("about.update.downloading", { version });
  const Icon = update.updateReady
    ? LuRefreshCw
    : downloading
      ? LuLoaderCircle
      : LuCircleArrowUp;

  const run = () => {
    if (update.updateReady) {
      void update.restartToUpdate();
    } else if (info.manual_update_required) {
      void openUrl(info.release_page_url ?? info.download_url);
    } else {
      void update.installAppUpdate(info);
    }
  };

  return (
    <Tooltip delayDuration={300}>
      <TooltipTrigger asChild>
        <button
          type="button"
          data-slot="rail-update"
          disabled={downloading}
          onClick={run}
          aria-label={hint}
          className={cn(
            RAIL_BUTTON,
            "bg-primary/10 text-primary-text hover:bg-primary/15 disabled:cursor-default disabled:hover:bg-primary/10",
          )}
        >
          <Icon
            aria-hidden="true"
            className={cn(
              "relative size-3.5 shrink-0 lg:size-4",
              downloading && "animate-spin motion-reduce:animate-none",
            )}
          />
          <span aria-hidden="true" className={RAIL_LABEL_CLASS}>
            {label}
          </span>
          {shownPercent !== null && (
            <span
              aria-hidden="true"
              className="absolute inset-x-1 bottom-0.5 h-0.5 overflow-hidden rounded-full bg-primary/20"
            >
              <span
                className="block h-full rounded-full bg-primary transition-[width] duration-200 motion-reduce:transition-none"
                style={{ width: `${shownPercent}%` }}
              />
            </span>
          )}
        </button>
      </TooltipTrigger>
      <TooltipContent side="right">{hint}</TooltipContent>
    </Tooltip>
  );
}
