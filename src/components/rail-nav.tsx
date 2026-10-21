"use client";

import { motion, useReducedMotion } from "motion/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { FaDownload } from "react-icons/fa";
import { FiWifi } from "react-icons/fi";
import { GoGear, GoKebabHorizontal } from "react-icons/go";
import {
  LuBot,
  LuCloud,
  LuCookie,
  LuInfo,
  LuKeyboard,
  LuLightbulb,
  LuPlug,
  LuPuzzle,
  LuServer,
  LuTrash2,
  LuUser,
  LuUsers,
} from "react-icons/lu";
import { useInputModality } from "@/hooks/use-input-modality";
import { CLOUD_ONLY_PAGES } from "@/lib/cloud-only-pages";
import { launchDonutClone } from "@/lib/donut-physics";
import { MOTION_SPRING_POSITION } from "@/lib/motion";
import { cn } from "@/lib/utils";
import { Logo } from "./icons/logo";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";

export type AppPage =
  | "profiles"
  | "proxies"
  | "extensions"
  | "groups"
  | "cookieBot"
  | "agent"
  | "vpns"
  | "settings"
  | "integrations"
  | "account"
  | "workspace"
  | "import"
  | "shortcuts"
  | "trash";

const CLICK_THRESHOLD = 5;
const CLICK_WINDOW_MS = 2000;
const LOGO_HIDDEN_KEY = "donut-logo-hidden";

function useLogoEasterEgg({
  currentPage,
  onNavigate,
}: {
  currentPage: AppPage;
  onNavigate: (page: AppPage) => void;
}) {
  const reduceMotion = useReducedMotion();
  const inputModality = useInputModality();
  const playfulMotion = !reduceMotion && inputModality === "pointer";
  const clickTimestamps = useRef<number[]>([]);
  const [isPressed, setIsPressed] = useState(false);
  const [wobbleKey, setWobbleKey] = useState(0);
  /** Wiggles earned by the cheat code, which arrives by keyboard by nature. */
  const [cheerKey, setCheerKey] = useState(0);
  const [isFalling, setIsFalling] = useState(false);
  /**
   * Click count toward the bounce trigger while the user is on the profiles
   * page. Capped at 4: each click here grows the logo by 25%, so step 4 has
   * doubled the original size. Click 5 fires `triggerFall` and resets.
   */
  const [growStep, setGrowStep] = useState(0);
  const resetTimeoutRef = useRef<number | null>(null);
  const [isHidden, setIsHidden] = useState(() => {
    try {
      return sessionStorage.getItem(LOGO_HIDDEN_KEY) === "1";
    } catch {
      return false;
    }
  });
  const logoRef = useRef<HTMLButtonElement>(null);
  const cancelFallRef = useRef<(() => void) | null>(null);

  const triggerFall = useCallback(() => {
    const el = logoRef.current;
    if (!el || isFalling) return;
    setIsFalling(true);

    cancelFallRef.current = launchDonutClone(el, {
      onExit: () => {
        try {
          sessionStorage.setItem(LOGO_HIDDEN_KEY, "1");
        } catch {
          // ignore — sessionStorage unavailable in some Tauri WebViews
        }
        setIsHidden(true);
        setIsFalling(false);
      },
    });
  }, [isFalling]);

  useEffect(() => {
    return () => {
      cancelFallRef.current?.();
    };
  }, []);

  const handleClick = useCallback(() => {
    if (isFalling || isHidden) return;

    // First behaviour: any click from elsewhere in the app just routes the
    // user back to the profiles list. Growing the donut requires the user
    // to already be home — that keeps the easter egg from accidentally
    // firing during normal navigation.
    if (currentPage !== "profiles") {
      onNavigate("profiles");
      clickTimestamps.current = [];
      setGrowStep(0);
      if (resetTimeoutRef.current !== null) {
        window.clearTimeout(resetTimeoutRef.current);
        resetTimeoutRef.current = null;
      }
      return;
    }

    if (!playfulMotion) return;

    const now = Date.now();
    clickTimestamps.current = clickTimestamps.current.filter(
      (t) => now - t < CLICK_WINDOW_MS,
    );
    clickTimestamps.current.push(now);

    if (clickTimestamps.current.length >= CLICK_THRESHOLD) {
      clickTimestamps.current = [];
      setGrowStep(0);
      if (resetTimeoutRef.current !== null) {
        window.clearTimeout(resetTimeoutRef.current);
        resetTimeoutRef.current = null;
      }
      triggerFall();
    } else {
      setGrowStep(
        Math.min(clickTimestamps.current.length, CLICK_THRESHOLD - 1),
      );
      setWobbleKey((k) => k + 1);
      if (resetTimeoutRef.current !== null) {
        window.clearTimeout(resetTimeoutRef.current);
      }
      resetTimeoutRef.current = window.setTimeout(() => {
        clickTimestamps.current = [];
        setGrowStep(0);
        resetTimeoutRef.current = null;
      }, CLICK_WINDOW_MS);
    }
  }, [
    currentPage,
    isFalling,
    isHidden,
    onNavigate,
    triggerFall,
    playfulMotion,
  ]);

  // Leaving the profiles page mid-streak cancels growth so we never end up
  // with an outsized logo when the user returns later.
  useEffect(() => {
    if (currentPage !== "profiles" || !playfulMotion) {
      clickTimestamps.current = [];
      setGrowStep(0);
      if (resetTimeoutRef.current !== null) {
        window.clearTimeout(resetTimeoutRef.current);
        resetTimeoutRef.current = null;
      }
    }
  }, [currentPage, playfulMotion]);

  // The cheat code (see useKonamiCode) is received with the same wiggle a
  // click earns, so the donut visibly takes the credit. It is typed, so the
  // pointer-only gate on click wiggles cannot apply; only reduced motion does.
  useEffect(() => {
    if (reduceMotion) return;
    const onCheatCode = () => setCheerKey((k) => k + 1);
    window.addEventListener("donut-cheat-code", onCheatCode);
    return () => {
      window.removeEventListener("donut-cheat-code", onCheatCode);
    };
  }, [reduceMotion]);

  useEffect(() => {
    if (!reduceMotion) return;
    cancelFallRef.current?.();
    cancelFallRef.current = null;
    setIsFalling(false);
    setIsPressed(false);
    if (logoRef.current) logoRef.current.style.visibility = "";
  }, [reduceMotion]);

  useEffect(() => {
    return () => {
      if (resetTimeoutRef.current !== null) {
        window.clearTimeout(resetTimeoutRef.current);
      }
    };
  }, []);

  return {
    logoRef,
    isPressed,
    setIsPressed,
    wobbleKey,
    cheerKey,
    isFalling,
    isHidden,
    growStep,
    handleClick,
    playfulMotion,
  };
}

interface RailNavProps {
  currentPage: AppPage;
  onNavigate: (page: AppPage) => void;
  onOpenAbout: () => void;
  /** Opens the feature tips catalog. */
  onOpenTips: () => void;
  /**
   * A remote session is running right now. The Cookie Bot item carries a dot so
   * the state is legible from every other page — an overnight job you cannot
   * see from where you are standing may as well not be observable at all.
   */
  cookieBotRunning?: boolean;
  /** Open agent questions and help requests waiting for the person. */
  agentRequests?: number;
}

/** Shared-element indicator that slides between the active rail items. */
function ActiveIndicator() {
  const reduceMotion = useReducedMotion();
  const inputModality = useInputModality();
  const animate = !reduceMotion && inputModality === "pointer";
  return (
    <motion.span
      aria-hidden="true"
      initial={false}
      layoutId={animate ? "rail-indicator" : undefined}
      transition={animate ? MOTION_SPRING_POSITION : { duration: 0 }}
      className="absolute inset-y-1.5 left-[-7px] w-[2px] rounded-full bg-primary lg:inset-y-2 lg:left-[-8px] lg:w-[3px]"
    />
  );
}

/** One destination: a square icon in the rail, an icon and its name in the
 *  sidebar. */
const RAIL_ITEM_CLASS =
  "relative grid size-7 shrink-0 cursor-pointer place-items-center rounded-md transition-colors duration-100 lg:flex lg:h-8 lg:w-full lg:items-center lg:justify-start lg:gap-2.5 lg:px-2.5";

const RAIL_LABEL_CLASS = "hidden min-w-0 truncate text-[13px] lg:block";

function railItemTone(active: boolean) {
  return active
    ? "bg-accent font-medium text-accent-foreground"
    : "text-muted-foreground hover:bg-accent hover:text-accent-foreground";
}

interface RailItem {
  page: AppPage;
  Icon: React.ComponentType<{ className?: string }>;
  labelKey: string;
}

const ALL_TOP_ITEMS: RailItem[] = [
  { page: "profiles", Icon: LuUser, labelKey: "rail.profiles" },
  { page: "proxies", Icon: FiWifi, labelKey: "rail.network" },
  { page: "extensions", Icon: LuPuzzle, labelKey: "rail.extensions" },
  { page: "groups", Icon: LuUsers, labelKey: "rail.groups" },
  { page: "cookieBot", Icon: LuCookie, labelKey: "rail.cookieBot" },
  { page: "agent", Icon: LuBot, labelKey: "rail.agent" },
  { page: "integrations", Icon: LuPlug, labelKey: "rail.integrations" },
  { page: "workspace", Icon: LuServer, labelKey: "rail.workspace" },
  { page: "account", Icon: LuCloud, labelKey: "rail.account" },
];

const TOP_ITEMS = ALL_TOP_ITEMS.filter(
  (item) => !CLOUD_ONLY_PAGES.has(item.page),
);

interface MoreMenuItem {
  page: AppPage;
  Icon: React.ComponentType<{ className?: string }>;
  labelKey: string;
  hintKey: string;
}

const MORE_ITEMS: MoreMenuItem[] = [
  {
    page: "import",
    Icon: FaDownload,
    labelKey: "rail.more.importProfile",
    hintKey: "rail.more.importProfileHint",
  },
  {
    page: "shortcuts",
    Icon: LuKeyboard,
    labelKey: "rail.more.keyboardShortcuts",
    hintKey: "rail.more.keyboardShortcutsHint",
  },
  {
    page: "trash",
    Icon: LuTrash2,
    labelKey: "rail.more.trash",
    hintKey: "rail.more.trashHint",
  },
];

export function RailNav({
  currentPage,
  onNavigate,
  onOpenAbout,
  onOpenTips,
  cookieBotRunning = false,
  agentRequests = 0,
}: RailNavProps) {
  const { t } = useTranslation();
  const [moreOpen, setMoreOpen] = useState(false);
  const {
    logoRef,
    isPressed,
    setIsPressed,
    wobbleKey,
    cheerKey,
    isFalling,
    isHidden,
    growStep,
    handleClick,
    playfulMotion,
  } = useLogoEasterEgg({ currentPage, onNavigate });

  useEffect(() => {
    if (!moreOpen) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setMoreOpen(false);
      }
    };
    document.addEventListener("keydown", closeOnEscape);
    return () => document.removeEventListener("keydown", closeOnEscape);
  }, [moreOpen]);

  return (
    // A rail of icons in a narrow window; from 1024px it opens into a sidebar
    // that names every destination.
    <nav className="relative flex w-10 shrink-0 flex-col items-center gap-1 border-r border-border bg-sidebar py-2 lg:w-52 lg:items-stretch lg:px-2 lg:py-3">
      <div className="flex shrink-0 items-center gap-2 lg:px-1">
        {!isHidden ? (
          <button
            ref={logoRef}
            type="button"
            aria-label={t("header.donutLogo")}
            className="grid size-7 shrink-0 cursor-pointer place-items-center rounded-md bg-transparent text-primary-text select-none"
            onClick={handleClick}
            onPointerDown={() => {
              setIsPressed(true);
            }}
            onPointerUp={() => {
              setIsPressed(false);
            }}
            onPointerLeave={() => {
              setIsPressed(false);
            }}
          >
            {/* Inner wrapper survives clicks (no `key`) so the scale change
              animates smoothly across the wiggle layer's remounts. */}
            <span
              style={{
                transform: !playfulMotion
                  ? "none"
                  : isPressed
                    ? `scale(${(1 + growStep * 0.25) * 0.9})`
                    : `scale(${1 + growStep * 0.25})`,
              }}
              className="inline-grid place-items-center transition-transform duration-300 ease-out motion-reduce:transition-none"
            >
              <span
                key={`${wobbleKey}:${cheerKey}`}
                className={cn(
                  "inline-grid place-items-center",
                  !isFalling &&
                    !isPressed &&
                    ((playfulMotion && wobbleKey > 0) || cheerKey > 0) &&
                    "animate-[wiggle_0.3s_ease-in-out]",
                )}
              >
                <Logo className="size-5 will-change-transform" />
              </span>
            </span>
          </button>
        ) : (
          <div className="size-7 shrink-0" />
        )}
        <span
          aria-hidden="true"
          className="hidden min-w-0 truncate text-sm font-semibold tracking-tight text-foreground lg:block"
        >
          {t("rail.brand")}
        </span>
      </div>

      <div className="my-1 h-px w-5 shrink-0 bg-border lg:my-2 lg:w-auto" />

      <div className="flex min-h-0 w-full scrollbar-none flex-col items-center gap-1 overflow-y-auto [-ms-overflow-style:none] lg:items-stretch lg:overflow-visible [&::-webkit-scrollbar]:hidden">
        {TOP_ITEMS.map(({ page, Icon, labelKey }) => {
          const active = currentPage === page;
          const waiting = page === "agent" ? agentRequests : 0;
          const label =
            waiting > 0
              ? t("rail.agentWaiting", { count: waiting })
              : page === "cookieBot" && cookieBotRunning
                ? t("rail.cookieBotRunning")
                : t(labelKey);
          return (
            <Tooltip key={page} delayDuration={300}>
              <TooltipTrigger asChild>
                <button
                  type="button"
                  onClick={() => {
                    onNavigate(page);
                  }}
                  aria-label={label}
                  aria-current={active ? "page" : undefined}
                  className={cn(RAIL_ITEM_CLASS, railItemTone(active))}
                >
                  {active && <ActiveIndicator />}
                  <Icon className="size-3.5 shrink-0 lg:size-4" />
                  <span aria-hidden="true" className={RAIL_LABEL_CLASS}>
                    {t(labelKey)}
                  </span>
                  {page === "cookieBot" && cookieBotRunning && (
                    <span
                      aria-hidden="true"
                      className="absolute top-1 right-1 size-1.5 rounded-full bg-success lg:static lg:ml-auto"
                    />
                  )}
                  {waiting > 0 && (
                    <span
                      aria-hidden="true"
                      data-testid="agent-rail-badge"
                      className="absolute -top-1 -right-1 grid h-3.5 min-w-3.5 place-items-center rounded-full bg-warning px-1 text-[9px] leading-none font-semibold tabular-nums text-warning-foreground lg:static lg:ml-auto"
                    >
                      {waiting > 9 ? "9+" : waiting}
                    </span>
                  )}
                </button>
              </TooltipTrigger>
              <TooltipContent side="right">{label}</TooltipContent>
            </Tooltip>
          );
        })}
      </div>

      <div className="flex-1" />

      <Tooltip delayDuration={300}>
        <TooltipTrigger asChild>
          <button
            type="button"
            onClick={() => {
              setMoreOpen((v) => !v);
            }}
            aria-label={t("rail.more.label")}
            aria-expanded={moreOpen}
            className={cn(RAIL_ITEM_CLASS, railItemTone(moreOpen))}
          >
            <GoKebabHorizontal className="size-3.5 shrink-0 lg:size-4" />
            <span aria-hidden="true" className={RAIL_LABEL_CLASS}>
              {t("rail.more.label")}
            </span>
          </button>
        </TooltipTrigger>
        <TooltipContent side="right">{t("rail.more.label")}</TooltipContent>
      </Tooltip>

      <Tooltip delayDuration={300}>
        <TooltipTrigger asChild>
          <button
            type="button"
            onClick={() => {
              onNavigate("settings");
            }}
            aria-label={t("rail.settings")}
            aria-current={currentPage === "settings" ? "page" : undefined}
            className={cn(
              RAIL_ITEM_CLASS,
              railItemTone(currentPage === "settings"),
            )}
          >
            {currentPage === "settings" && <ActiveIndicator />}
            <GoGear className="size-3.5 shrink-0 lg:size-4" />
            <span aria-hidden="true" className={RAIL_LABEL_CLASS}>
              {t("rail.settings")}
            </span>
          </button>
        </TooltipTrigger>
        <TooltipContent side="right">{t("rail.settings")}</TooltipContent>
      </Tooltip>

      {moreOpen && (
        <>
          <button
            type="button"
            aria-label={t("rail.more.closeAriaLabel")}
            className="fixed inset-0 z-30 cursor-default bg-transparent"
            onClick={() => {
              setMoreOpen(false);
            }}
          />
          <div
            role="menu"
            aria-label={t("rail.more.label")}
            className="absolute bottom-14 left-11 z-40 w-56 rounded-lg bg-popover p-1 text-popover-foreground shadow-lg ring-1 ring-border lg:left-[13.25rem]"
          >
            {MORE_ITEMS.map(({ page, Icon, labelKey, hintKey }) => (
              <button
                key={page}
                type="button"
                role="menuitem"
                onClick={() => {
                  setMoreOpen(false);
                  onNavigate(page);
                }}
                className="flex w-full cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors duration-100 hover:bg-accent hover:text-accent-foreground"
              >
                <span className="grid size-5 shrink-0 place-items-center text-muted-foreground">
                  <Icon className="size-3" />
                </span>
                <span className="flex min-w-0 flex-col">
                  <span className="truncate text-xs font-medium text-foreground">
                    {t(labelKey)}
                  </span>
                  <span className="truncate text-[10px] text-muted-foreground">
                    {t(hintKey)}
                  </span>
                </span>
              </button>
            ))}
            <button
              type="button"
              role="menuitem"
              data-slot="rail-open-tips"
              onClick={() => {
                setMoreOpen(false);
                onOpenTips();
              }}
              className="flex w-full cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors duration-100 hover:bg-accent hover:text-accent-foreground"
            >
              <span className="grid size-5 shrink-0 place-items-center text-muted-foreground">
                <LuLightbulb className="size-3" />
              </span>
              <span className="flex min-w-0 flex-col">
                <span className="truncate text-xs font-medium text-foreground">
                  {t("rail.more.tips")}
                </span>
                <span className="truncate text-[10px] text-muted-foreground">
                  {t("rail.more.tipsHint")}
                </span>
              </span>
            </button>
            <button
              type="button"
              role="menuitem"
              onClick={() => {
                setMoreOpen(false);
                onOpenAbout();
              }}
              className="flex w-full cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors duration-100 hover:bg-accent hover:text-accent-foreground"
            >
              <span className="grid size-5 shrink-0 place-items-center text-muted-foreground">
                <LuInfo className="size-3" />
              </span>
              <span className="flex min-w-0 flex-col">
                <span className="truncate text-xs font-medium text-foreground">
                  {t("rail.more.about")}
                </span>
                <span className="truncate text-[10px] text-muted-foreground">
                  {t("rail.more.aboutHint")}
                </span>
              </span>
            </button>
          </div>
        </>
      )}
    </nav>
  );
}
