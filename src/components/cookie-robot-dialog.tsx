"use client";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { LoadingButton } from "@/components/loading-button";
import { Badge } from "@/components/ui/badge";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Progress } from "@/components/ui/progress";
import { Textarea } from "@/components/ui/textarea";
import { translateBackendError } from "@/lib/backend-errors";
import type { CookieRobotState, CookieRobotStatus } from "@/types";
import { RippleButton } from "./ui/ripple";

const SITES_KEY = "cookieRobot.sites";
const MAX_PARALLEL = 5;

/** The site list from the last run, kept only as a convenience. */
function rememberedSites(): string {
  try {
    return window.localStorage.getItem(SITES_KEY) ?? "";
  } catch {
    return "";
  }
}

function rememberSites(sites: string) {
  try {
    window.localStorage.setItem(SITES_KEY, sites);
  } catch {
    // Storage is unavailable; the list is simply typed again next time.
  }
}

const STATE_CLASS: Record<CookieRobotState, string> = {
  queued: "bg-foreground/8 text-muted-foreground",
  running: "bg-primary/10 text-primary-text",
  done: "bg-success/10 text-success-text",
  failed: "bg-destructive/10 text-destructive-text",
  cancelled: "bg-warning/10 text-warning-text",
};

interface CookieRobotDialogProps {
  isOpen: boolean;
  onClose: () => void;
  selectedProfiles: string[];
}

export function CookieRobotDialog({
  isOpen,
  onClose,
  selectedProfiles,
}: CookieRobotDialogProps) {
  const { t } = useTranslation();
  const fieldId = useId();
  const [sites, setSites] = useState("");
  const [minSeconds, setMinSeconds] = useState(8);
  const [maxSeconds, setMaxSeconds] = useState(20);
  const [parallel, setParallel] = useState(1);
  const [shuffle, setShuffle] = useState(true);
  const [headless, setHeadless] = useState(false);
  const [status, setStatus] = useState<CookieRobotStatus | null>(null);
  // A finished run stays on screen until the user asks for a new one.
  const [showSetup, setShowSetup] = useState(true);
  const [isStarting, setIsStarting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isOpen) return;
    setError(null);
    setSites(rememberedSites());
    void invoke<CookieRobotStatus>("get_cookie_robot_status")
      .then((current) => {
        setStatus(current);
        setShowSetup(!current.running);
      })
      .catch((err: unknown) => {
        console.error("Failed to read Cookie Robot status:", err);
      });
  }, [isOpen]);

  useEffect(() => {
    if (!isOpen) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void listen<CookieRobotStatus>("cookie-robot-progress", (event) => {
      setStatus(event.payload);
    }).then((stop) => {
      if (cancelled) stop();
      else unlisten = stop;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [isOpen]);

  const handleStart = useCallback(async () => {
    setIsStarting(true);
    setError(null);
    try {
      const started = await invoke<CookieRobotStatus>("start_cookie_robot", {
        request: {
          profile_ids: selectedProfiles,
          urls: sites.split("\n"),
          min_seconds: minSeconds,
          max_seconds: Math.max(minSeconds, maxSeconds),
          shuffle,
          headless,
          parallel,
        },
      });
      rememberSites(sites);
      setStatus(started);
      setShowSetup(false);
    } catch (err) {
      setError(translateBackendError(t, err));
    } finally {
      setIsStarting(false);
    }
  }, [
    selectedProfiles,
    sites,
    minSeconds,
    maxSeconds,
    shuffle,
    headless,
    parallel,
    t,
  ]);

  const handleStop = useCallback(() => {
    void invoke<boolean>("stop_cookie_robot").catch((err: unknown) => {
      console.error("Failed to stop Cookie Robot:", err);
    });
  }, []);

  const running = status?.running ?? false;
  const hasSites = sites.trim().length > 0;

  return (
    <Dialog open={isOpen} onOpenChange={onClose}>
      <DialogContent className="flex max-h-[calc(100vh-2rem)] max-w-lg flex-col">
        <DialogHeader>
          <DialogTitle>{t("cookieRobot.title")}</DialogTitle>
          <DialogDescription>{t("cookieRobot.description")}</DialogDescription>
        </DialogHeader>

        {showSetup ? (
          <div className="-mx-1 min-h-0 flex-1 space-y-4 overflow-y-auto px-1">
            <p className="text-sm text-muted-foreground">
              {t("cookieRobot.profilesSelected", {
                count: selectedProfiles.length,
              })}
            </p>

            <div className="grid gap-2">
              <Label htmlFor={`${fieldId}-sites`}>
                {t("cookieRobot.sites")}
              </Label>
              <Textarea
                id={`${fieldId}-sites`}
                value={sites}
                onChange={(e) => {
                  setSites(e.target.value);
                }}
                placeholder={t("cookieRobot.sitesPlaceholder")}
                aria-describedby={`${fieldId}-sites-hint`}
                autoCapitalize="none"
                autoComplete="off"
                spellCheck={false}
                className="min-h-24 resize-y font-mono text-xs leading-relaxed"
              />
              <p
                id={`${fieldId}-sites-hint`}
                className="text-xs text-muted-foreground"
              >
                {t("cookieRobot.sitesHint")}
              </p>
            </div>

            <div className="grid grid-cols-3 items-end gap-3">
              <div className="grid gap-2">
                <Label htmlFor={`${fieldId}-min`}>
                  {t("cookieRobot.secondsFrom")}
                </Label>
                <Input
                  id={`${fieldId}-min`}
                  type="number"
                  min={3}
                  max={600}
                  value={minSeconds}
                  onChange={(e) => {
                    setMinSeconds(Number(e.target.value) || 0);
                  }}
                />
              </div>
              <div className="grid gap-2">
                <Label htmlFor={`${fieldId}-max`}>
                  {t("cookieRobot.secondsTo")}
                </Label>
                <Input
                  id={`${fieldId}-max`}
                  type="number"
                  min={3}
                  max={600}
                  value={maxSeconds}
                  onChange={(e) => {
                    setMaxSeconds(Number(e.target.value) || 0);
                  }}
                />
              </div>
              <div className="grid gap-2">
                <Label htmlFor={`${fieldId}-parallel`}>
                  {t("cookieRobot.parallel")}
                </Label>
                <Input
                  id={`${fieldId}-parallel`}
                  type="number"
                  min={1}
                  max={MAX_PARALLEL}
                  value={parallel}
                  onChange={(e) => {
                    const value = Number(e.target.value) || 1;
                    setParallel(Math.min(MAX_PARALLEL, Math.max(1, value)));
                  }}
                />
              </div>
            </div>

            <div className="space-y-2">
              <div className="flex items-center gap-2">
                <Checkbox
                  id={`${fieldId}-shuffle`}
                  checked={shuffle}
                  onCheckedChange={(state) => {
                    setShuffle(state === true);
                  }}
                />
                <Label htmlFor={`${fieldId}-shuffle`} className="font-normal">
                  {t("cookieRobot.shuffle")}
                </Label>
              </div>
              <div className="flex items-center gap-2">
                <Checkbox
                  id={`${fieldId}-headless`}
                  checked={headless}
                  onCheckedChange={(state) => {
                    setHeadless(state === true);
                  }}
                />
                <Label htmlFor={`${fieldId}-headless`} className="font-normal">
                  {t("cookieRobot.headless")}
                </Label>
              </div>
            </div>

            {error && (
              <div className="rounded-md bg-destructive/10 p-3 text-sm text-destructive-text">
                {error}
              </div>
            )}
          </div>
        ) : (
          <ul className="min-h-0 flex-1 space-y-3 overflow-y-auto pr-1">
            {(status?.profiles ?? []).map((profile) => (
              <li key={profile.profile_id} className="space-y-1.5">
                <div className="flex items-center justify-between gap-2">
                  <span className="truncate text-sm font-medium">
                    {profile.name}
                  </span>
                  <Badge variant="soft" className={STATE_CLASS[profile.state]}>
                    {t(`cookieRobot.state.${profile.state}`)}
                  </Badge>
                </div>
                <Progress
                  value={
                    profile.total > 0
                      ? ((profile.visited + profile.unreachable) /
                          profile.total) *
                        100
                      : 0
                  }
                  aria-label={profile.name}
                />
                <p className="truncate text-xs text-muted-foreground">
                  {profile.error
                    ? translateBackendError(t, profile.error)
                    : (profile.current_url ??
                      t("cookieRobot.visited", {
                        visited: profile.visited,
                        total: profile.total,
                      }))}
                  {!profile.error && profile.unreachable > 0 && (
                    <>
                      {" · "}
                      {t("cookieRobot.unreachable", {
                        count: profile.unreachable,
                      })}
                    </>
                  )}
                </p>
              </li>
            ))}
          </ul>
        )}

        <DialogFooter>
          <RippleButton variant="outline" onClick={onClose}>
            {t("common.buttons.close")}
          </RippleButton>
          {showSetup ? (
            <LoadingButton
              isLoading={isStarting}
              onClick={() => void handleStart()}
              disabled={running || !hasSites || selectedProfiles.length === 0}
            >
              {t("common.buttons.start")}
            </LoadingButton>
          ) : running ? (
            <RippleButton variant="destructive" onClick={handleStop}>
              {t("common.buttons.stop")}
            </RippleButton>
          ) : (
            <RippleButton
              onClick={() => {
                setShowSetup(true);
              }}
            >
              {t("cookieRobot.newRun")}
            </RippleButton>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
