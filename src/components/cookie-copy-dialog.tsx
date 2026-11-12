"use client";

import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { LuChevronRight, LuCookie, LuSearch } from "react-icons/lu";
import { toast } from "sonner";
import { LoadingButton } from "@/components/loading-button";
import {
  AnimatedDisclosureChevron,
  AnimatedDisclosureContent,
  AnimatedDisclosureItem,
} from "@/components/ui/animated-disclosure";
import { Button } from "@/components/ui/button";
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
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { translateBackendError } from "@/lib/backend-errors";
import { getBrowserIcon } from "@/lib/browser-utils";
import { cookieKey } from "@/lib/cookie-key";
import type {
  BrowserProfile,
  CookieCopyRequest,
  CookieCopyResult,
  CookieReadResult,
  DomainCookies,
  SelectedCookie,
  UnifiedCookie,
} from "@/types";
import { RippleButton } from "./ui/ripple";

interface CookieCopyDialogProps {
  isOpen: boolean;
  onClose: () => void;
  selectedProfiles: string[];
  profiles: BrowserProfile[];
  runningProfiles: Set<string>;
  onCopyComplete?: () => void;
  /**
   * Opened from a profile's Cookie section: that profile is the source and
   * the person picks where its cookies go, instead of the other way round.
   */
  fixedSourceProfileId?: string | null;
}

type SelectionState = Record<
  string,
  {
    allSelected: boolean;
    cookies: Set<string>;
  }
>;

export function CookieCopyDialog({
  isOpen,
  onClose,
  selectedProfiles,
  profiles,
  runningProfiles,
  onCopyComplete,
  fixedSourceProfileId = null,
}: CookieCopyDialogProps) {
  const { t } = useTranslation();
  const [sourceProfileId, setSourceProfileId] = useState<string | null>(null);
  const [cookieData, setCookieData] = useState<CookieReadResult | null>(null);
  const [isLoadingCookies, setIsLoadingCookies] = useState(false);
  const [isCopying, setIsCopying] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [selection, setSelection] = useState<SelectionState>({});
  const [expandedDomains, setExpandedDomains] = useState<Set<string>>(
    new Set(),
  );
  const [error, setError] = useState<string | null>(null);
  const [chosenTargetIds, setChosenTargetIds] = useState<Set<string>>(
    new Set(),
  );

  // Never offer a selected profile as a source — you can't copy a profile's
  // cookies onto itself, and including it here would leave the user in a
  // dead-end state (source picked = target list empty = copy button disabled).
  const eligibleSourceProfiles = useMemo(() => {
    return profiles.filter(
      (p) =>
        !selectedProfiles.includes(p.id) &&
        (p.browser === "wayfern" || p.browser === "camoufox"),
    );
  }, [profiles, selectedProfiles]);

  // The profiles a fixed source may copy into.
  const targetCandidates = useMemo(() => {
    if (!fixedSourceProfileId) return [];
    return profiles.filter(
      (p) =>
        p.id !== fixedSourceProfileId &&
        p.ephemeral !== true &&
        (p.browser === "wayfern" || p.browser === "camoufox"),
    );
  }, [profiles, fixedSourceProfileId]);

  const targetProfiles = useMemo(() => {
    if (fixedSourceProfileId) {
      return targetCandidates.filter((p) => chosenTargetIds.has(p.id));
    }
    return profiles.filter(
      (p) =>
        selectedProfiles.includes(p.id) &&
        p.id !== sourceProfileId &&
        (p.browser === "wayfern" || p.browser === "camoufox"),
    );
  }, [
    profiles,
    selectedProfiles,
    sourceProfileId,
    fixedSourceProfileId,
    targetCandidates,
    chosenTargetIds,
  ]);

  const allTargetsChosen =
    targetCandidates.length > 0 &&
    targetCandidates.every((p) => chosenTargetIds.has(p.id));

  const toggleAllTargets = useCallback(() => {
    setChosenTargetIds(
      allTargetsChosen ? new Set() : new Set(targetCandidates.map((p) => p.id)),
    );
  }, [allTargetsChosen, targetCandidates]);

  const toggleTarget = useCallback((profileId: string) => {
    setChosenTargetIds((prev) => {
      const next = new Set(prev);
      if (next.has(profileId)) {
        next.delete(profileId);
      } else {
        next.add(profileId);
      }
      return next;
    });
  }, []);

  const filteredDomains = useMemo(() => {
    if (!cookieData) return [];
    if (!searchQuery.trim()) return cookieData.domains;

    const query = searchQuery.toLowerCase();
    return cookieData.domains.filter(
      (d) =>
        d.domain.toLowerCase().includes(query) ||
        d.cookies.some((c) => c.name.toLowerCase().includes(query)),
    );
  }, [cookieData, searchQuery]);

  const selectedCookieCount = useMemo(() => {
    let count = 0;
    for (const domain of Object.keys(selection)) {
      const domainSelection = selection[domain];
      if (domainSelection.allSelected) {
        const domainData = cookieData?.domains.find((d) => d.domain === domain);
        count += domainData?.cookie_count ?? 0;
      } else {
        count += domainSelection.cookies.size;
      }
    }
    return count;
  }, [selection, cookieData]);

  const loadCookies = useCallback(
    async (profileId: string) => {
      setIsLoadingCookies(true);
      setError(null);
      setCookieData(null);
      setSelection({});

      try {
        const result = await invoke<CookieReadResult>("read_profile_cookies", {
          profileId,
        });
        setCookieData(result);
      } catch (err) {
        console.error("Failed to load cookies:", err);
        setError(translateBackendError(t, err));
      } finally {
        setIsLoadingCookies(false);
      }
    },
    [t],
  );

  const handleSourceChange = useCallback(
    (profileId: string) => {
      setSourceProfileId(profileId);
      void loadCookies(profileId);
    },
    [loadCookies],
  );

  const toggleDomain = useCallback(
    (domain: string, cookies: UnifiedCookie[]) => {
      setSelection((prev) => {
        // `prev[domain]` is `undefined` for any domain not yet interacted with
        // and after the user fully deselects it (toggleCookie deletes the
        // entry on empty). Treat missing as "not selected".
        if (prev[domain]?.allSelected) {
          const newSelection = { ...prev };
          delete newSelection[domain];
          return newSelection;
        }
        return {
          ...prev,
          [domain]: {
            allSelected: true,
            cookies: new Set(cookies.map(cookieKey)),
          },
        };
      });
    },
    [],
  );

  const toggleCookie = useCallback(
    (domain: string, key: string, totalCookies: number) => {
      setSelection((prev) => {
        const current = prev[domain] ?? {
          allSelected: false,
          cookies: new Set<string>(),
        };
        const newCookies = new Set(current.cookies);

        if (newCookies.has(key)) {
          newCookies.delete(key);
        } else {
          newCookies.add(key);
        }

        const allSelected = newCookies.size === totalCookies;

        if (newCookies.size === 0) {
          const newSelection = { ...prev };
          delete newSelection[domain];
          return newSelection;
        }

        return {
          ...prev,
          [domain]: {
            allSelected,
            cookies: newCookies,
          },
        };
      });
    },
    [],
  );

  const toggleExpand = useCallback((domain: string) => {
    setExpandedDomains((prev) => {
      const next = new Set(prev);
      if (next.has(domain)) {
        next.delete(domain);
      } else {
        next.add(domain);
      }
      return next;
    });
  }, []);

  // Acts on the domains the search shows, so "google" then "select all"
  // takes every google domain and nothing else.
  const allShownDomainsSelected =
    filteredDomains.length > 0 &&
    filteredDomains.every((d) => selection[d.domain]?.allSelected);

  const toggleAllShownDomains = useCallback(() => {
    setSelection((prev) => {
      const next = { ...prev };
      for (const domain of filteredDomains) {
        if (allShownDomainsSelected) {
          delete next[domain.domain];
        } else {
          next[domain.domain] = {
            allSelected: true,
            cookies: new Set(domain.cookies.map(cookieKey)),
          };
        }
      }
      return next;
    });
  }, [filteredDomains, allShownDomainsSelected]);

  const buildSelectedCookies = useCallback((): SelectedCookie[] => {
    const result: SelectedCookie[] = [];

    for (const [domain, domainSelection] of Object.entries(selection)) {
      if (domainSelection.allSelected) {
        result.push({ domain, name: "" });
      } else {
        const domainData = cookieData?.domains.find((d) => d.domain === domain);
        for (const cookie of domainData?.cookies ?? []) {
          if (domainSelection.cookies.has(cookieKey(cookie))) {
            result.push({ domain, name: cookie.name, path: cookie.path });
          }
        }
      }
    }

    return result;
  }, [selection, cookieData]);

  const handleCopy = useCallback(async () => {
    if (!sourceProfileId || targetProfiles.length === 0) return;

    const runningTargets = targetProfiles.filter((p) =>
      runningProfiles.has(p.id),
    );
    if (runningTargets.length > 0) {
      const names = runningTargets.map((p) => p.name).join(", ");
      toast.error(
        runningTargets.length === 1
          ? t("cookies.copy.cannotCopyRunningOne", { names })
          : t("cookies.copy.cannotCopyRunningMany", { names }),
      );
      return;
    }

    setIsCopying(true);
    setError(null);

    try {
      const selectedCookies = buildSelectedCookies();
      const request: CookieCopyRequest = {
        source_profile_id: sourceProfileId,
        target_profile_ids: targetProfiles.map((p) => p.id),
        selected_cookies: selectedCookies,
      };

      const results = await invoke<CookieCopyResult[]>("copy_profile_cookies", {
        request,
      });

      let totalCopied = 0;
      let totalReplaced = 0;
      const errors: string[] = [];

      for (const result of results) {
        totalCopied += result.cookies_copied;
        totalReplaced += result.cookies_replaced;
        errors.push(...result.errors);
      }

      if (errors.length > 0) {
        toast.error(
          t("cookies.copy.someErrors", { errors: errors.join(", ") }),
        );
      } else {
        toast.success(
          t("cookies.copy.successMessage", {
            copied: totalCopied + totalReplaced,
            replaced: totalReplaced,
          }),
        );
        onCopyComplete?.();
        onClose();
      }
    } catch (err) {
      console.error("Failed to copy cookies:", err);
      toast.error(
        t("cookies.copy.failedMessage", {
          error: translateBackendError(t, err),
        }),
      );
    } finally {
      setIsCopying(false);
    }
  }, [
    sourceProfileId,
    targetProfiles,
    runningProfiles,
    buildSelectedCookies,
    onCopyComplete,
    onClose,
    t,
  ]);

  useEffect(() => {
    if (isOpen) {
      setSourceProfileId(fixedSourceProfileId);
      setCookieData(null);
      setSelection({});
      setSearchQuery("");
      setExpandedDomains(new Set());
      setChosenTargetIds(new Set());
      setError(null);
      if (fixedSourceProfileId) void loadCookies(fixedSourceProfileId);
    }
  }, [isOpen, fixedSourceProfileId, loadCookies]);

  const canCopy =
    sourceProfileId &&
    targetProfiles.length > 0 &&
    selectedCookieCount > 0 &&
    !isCopying;

  return (
    <Dialog open={isOpen} onOpenChange={onClose}>
      <DialogContent className="flex max-h-[80vh] max-w-[min(48rem,calc(100%-4rem))] flex-col">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <LuCookie className="size-5" />
            {t("cookies.copy.title")}
          </DialogTitle>
          <DialogDescription>
            {fixedSourceProfileId
              ? t("cookies.copy.fromProfileDescription", {
                  name:
                    profiles.find((p) => p.id === fixedSourceProfileId)?.name ??
                    "",
                })
              : selectedProfiles.length === 1
                ? t("cookies.copy.dialogDescription_one", {
                    count: selectedProfiles.length,
                  })
                : t("cookies.copy.dialogDescription_other", {
                    count: selectedProfiles.length,
                  })}
          </DialogDescription>
        </DialogHeader>

        <div className="flex-1 space-y-4 overflow-y-auto">
          <div className="space-y-2">
            <Label>{t("cookies.copy.sourceProfile")}</Label>
            <Select
              value={sourceProfileId ?? undefined}
              onValueChange={handleSourceChange}
              disabled={Boolean(fixedSourceProfileId)}
            >
              <SelectTrigger>
                <SelectValue
                  placeholder={t("cookies.copy.sourcePlaceholder")}
                />
              </SelectTrigger>
              <SelectContent>
                {eligibleSourceProfiles.map((profile) => {
                  const IconComponent = getBrowserIcon(profile.browser);
                  const isRunning = runningProfiles.has(profile.id);
                  return (
                    <SelectItem
                      key={profile.id}
                      value={profile.id}
                      disabled={isRunning}
                    >
                      <div className="flex items-center gap-2">
                        {IconComponent && <IconComponent className="size-4" />}
                        <span>{profile.name}</span>
                        {isRunning && (
                          <span className="text-xs text-muted-foreground">
                            {t("cookies.copy.running")}
                          </span>
                        )}
                      </div>
                    </SelectItem>
                  );
                })}
              </SelectContent>
            </Select>
          </div>

          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <Label>
                {t("cookies.copy.targetProfiles", {
                  count: targetProfiles.length,
                })}
              </Label>
              {fixedSourceProfileId && targetCandidates.length > 0 && (
                <Button
                  type="button"
                  variant="subtle"
                  size="sm"
                  onClick={toggleAllTargets}
                >
                  {allTargetsChosen
                    ? t("cookies.management.deselectAll")
                    : t("cookies.management.selectAll")}
                </Button>
              )}
            </div>
            {fixedSourceProfileId ? (
              <div className="max-h-40 space-y-1 overflow-y-auto rounded-md bg-foreground/5 p-2">
                {targetCandidates.length === 0 ? (
                  <p className="text-sm text-muted-foreground">
                    {t("cookies.copy.noOtherTargets")}
                  </p>
                ) : (
                  targetCandidates.map((p) => {
                    const checkboxId = `cookie-copy-target-${p.id}`;
                    return (
                      <div key={p.id} className="flex items-center gap-2">
                        <Checkbox
                          id={checkboxId}
                          checked={chosenTargetIds.has(p.id)}
                          onCheckedChange={() => {
                            toggleTarget(p.id);
                          }}
                        />
                        <label
                          htmlFor={checkboxId}
                          className="flex cursor-pointer items-center gap-2 text-sm"
                        >
                          {p.name}
                          {runningProfiles.has(p.id) && (
                            <span className="text-xs text-destructive-text">
                              {t("cookies.copy.running")}
                            </span>
                          )}
                        </label>
                      </div>
                    );
                  })
                )}
              </div>
            ) : (
              <div className="max-h-20 overflow-y-auto rounded-md bg-muted p-2">
                {targetProfiles.length === 0 ? (
                  <p className="text-sm text-muted-foreground">
                    {sourceProfileId
                      ? t("cookies.copy.noOtherTargets")
                      : t("cookies.copy.selectSourceFirst")}
                  </p>
                ) : (
                  <div className="flex flex-wrap gap-1">
                    {targetProfiles.map((p) => (
                      <span
                        key={p.id}
                        className="inline-flex items-center gap-1 rounded bg-background px-2 py-0.5 text-sm"
                      >
                        {p.name}
                        {runningProfiles.has(p.id) && (
                          <span className="text-xs text-destructive-text">
                            {t("cookies.copy.running")}
                          </span>
                        )}
                      </span>
                    ))}
                  </div>
                )}
              </div>
            )}
          </div>

          {sourceProfileId && (
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <Label>
                  {t("cookies.copy.selectCookies")}{" "}
                  {cookieData && (
                    <span className="text-muted-foreground">
                      {t("cookies.copy.selectionStatus", {
                        selected: selectedCookieCount,
                        total: cookieData.total_count,
                      })}
                    </span>
                  )}
                </Label>
                {filteredDomains.length > 0 && (
                  <Button
                    type="button"
                    variant="subtle"
                    size="sm"
                    onClick={toggleAllShownDomains}
                  >
                    {allShownDomainsSelected
                      ? t("cookies.management.deselectAll")
                      : t("cookies.management.selectAll")}
                  </Button>
                )}
              </div>

              <div className="relative">
                <LuSearch className="absolute top-1/2 left-2 size-4 -translate-y-1/2 text-muted-foreground" />
                <Input
                  placeholder={t("cookies.copy.searchPlaceholder")}
                  value={searchQuery}
                  onChange={(e) => {
                    setSearchQuery(e.target.value);
                  }}
                  className="pl-8"
                />
              </div>

              {isLoadingCookies ? (
                <div className="flex h-40 items-center justify-center">
                  <div className="size-6 animate-spin rounded-full border-2 border-primary border-t-transparent" />
                </div>
              ) : error ? (
                <div className="rounded-md bg-destructive/10 p-4 text-center text-destructive-text">
                  {error}
                </div>
              ) : filteredDomains.length === 0 ? (
                <div className="p-4 text-center text-muted-foreground">
                  {searchQuery
                    ? t("cookies.copy.noMatching")
                    : t("cookies.copy.noFound")}
                </div>
              ) : (
                <ScrollArea className="h-[clamp(150px,35vh,450px)] rounded-md border">
                  <div className="space-y-1 p-2">
                    {filteredDomains.map((domain) => (
                      <DomainRow
                        key={domain.domain}
                        domain={domain}
                        selection={selection}
                        isExpanded={expandedDomains.has(domain.domain)}
                        onToggleDomain={toggleDomain}
                        onToggleCookie={toggleCookie}
                        onToggleExpand={toggleExpand}
                      />
                    ))}
                  </div>
                </ScrollArea>
              )}

              <p className="text-xs text-muted-foreground">
                {t("cookies.copy.replaceNote")}
              </p>
            </div>
          )}
        </div>

        <DialogFooter>
          <RippleButton
            variant="outline"
            onClick={onClose}
            disabled={isCopying}
          >
            {t("common.buttons.cancel")}
          </RippleButton>
          <LoadingButton
            isLoading={isCopying}
            onClick={() => void handleCopy()}
            disabled={!canCopy}
          >
            {selectedCookieCount === 0
              ? t("cookies.copy.copyButtonEmpty")
              : selectedCookieCount === 1
                ? t("cookies.copy.copyButton_one", {
                    count: selectedCookieCount,
                  })
                : t("cookies.copy.copyButton_other", {
                    count: selectedCookieCount,
                  })}
          </LoadingButton>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

interface DomainRowProps {
  domain: DomainCookies;
  selection: SelectionState;
  isExpanded: boolean;
  onToggleDomain: (domain: string, cookies: UnifiedCookie[]) => void;
  onToggleCookie: (
    domain: string,
    cookieKey: string,
    totalCookies: number,
  ) => void;
  onToggleExpand: (domain: string) => void;
}

function DomainRow({
  domain,
  selection,
  isExpanded,
  onToggleDomain,
  onToggleCookie,
  onToggleExpand,
}: DomainRowProps) {
  // `selection[domain.domain]` is `undefined` for domains the user hasn't
  // touched yet (initial state after loading cookies is `{}`) and for any
  // domain the user fully deselected (toggleCookie deletes the entry on
  // empty). Default to "no cookies selected" instead of crashing.
  const domainSelection = selection[domain.domain];
  const isAllSelected = domainSelection?.allSelected ?? false;
  const selectedCount = domainSelection?.cookies.size ?? 0;
  const isPartial =
    selectedCount > 0 && selectedCount < domain.cookie_count && !isAllSelected;

  return (
    <AnimatedDisclosureItem>
      <div className="flex items-center gap-2 rounded p-2 hover:bg-muted">
        <Checkbox
          checked={isAllSelected || isPartial}
          onCheckedChange={() => {
            onToggleDomain(domain.domain, domain.cookies);
          }}
          className={isPartial ? "opacity-70" : ""}
        />
        <button
          type="button"
          aria-expanded={isExpanded}
          className="flex min-w-0 flex-1 cursor-pointer items-center gap-1 border-none bg-transparent text-left"
          onClick={() => {
            onToggleExpand(domain.domain);
          }}
        >
          <AnimatedDisclosureChevron open={isExpanded}>
            <LuChevronRight className="size-4" />
          </AnimatedDisclosureChevron>
          <span className="truncate font-medium">{domain.domain}</span>
          <span className="shrink-0 text-xs text-muted-foreground">
            ({domain.cookie_count})
          </span>
        </button>
      </div>
      <AnimatedDisclosureContent
        open={isExpanded}
        className="ml-8 space-y-1 border-l pl-2"
      >
        {domain.cookies.map((cookie) => {
          const isSelected =
            domainSelection?.cookies.has(cookieKey(cookie)) ?? false;
          return (
            <div
              key={`${domain.domain}-${cookieKey(cookie)}`}
              className="flex items-center gap-2 rounded p-1 text-sm hover:bg-accent/30"
            >
              <Checkbox
                checked={isSelected || isAllSelected}
                onCheckedChange={() => {
                  onToggleCookie(
                    domain.domain,
                    cookieKey(cookie),
                    domain.cookie_count,
                  );
                }}
              />
              <span className="truncate">{cookie.name}</span>
            </div>
          );
        })}
      </AnimatedDisclosureContent>
    </AnimatedDisclosureItem>
  );
}
