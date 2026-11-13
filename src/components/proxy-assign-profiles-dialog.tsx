"use client";

import { invoke } from "@tauri-apps/api/core";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { LoadingButton } from "@/components/loading-button";
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
import { RippleButton } from "@/components/ui/ripple";
import { translateBackendError } from "@/lib/backend-errors";
import { showErrorToast, showSuccessToast } from "@/lib/toast-utils";
import type { BrowserProfile, StoredProxy } from "@/types";

interface ProxyAssignProfilesDialogProps {
  /** The proxy being assigned; the dialog is open while it is set. */
  proxy: StoredProxy | null;
  profiles: BrowserProfile[] | null;
  onClose: () => void;
}

const idsUsing = (profiles: BrowserProfile[], proxyId: string): Set<string> =>
  new Set(
    profiles
      .filter((profile) => profile.proxy_id === proxyId)
      .map((profile) => profile.id),
  );

/** Tick boxes to attach this proxy to profiles or take it off them. */
export function ProxyAssignProfilesDialog({
  proxy,
  profiles,
  onClose,
}: ProxyAssignProfilesDialogProps) {
  const { t } = useTranslation();
  const [checked, setChecked] = useState<Set<string>>(new Set());
  const [query, setQuery] = useState("");
  const [isBusy, setIsBusy] = useState(false);
  const proxyId = proxy?.id;
  const loaded = profiles !== null;

  // Start from what the profiles use when the dialog opens, not on every
  // profile event while it is open, so ticks are never overwritten.
  // biome-ignore lint/correctness/useExhaustiveDependencies: see above
  useEffect(() => {
    if (!proxyId || !profiles) return;
    setChecked(idsUsing(profiles, proxyId));
    setQuery("");
  }, [proxyId, loaded]);

  const original = useMemo(
    () =>
      proxyId && profiles ? idsUsing(profiles, proxyId) : new Set<string>(),
    [proxyId, profiles],
  );
  const attach = [...checked].filter((id) => !original.has(id));
  const detach = [...original].filter((id) => !checked.has(id));
  const changes = attach.length + detach.length;

  const visible = (profiles ?? []).filter((profile) =>
    profile.name.toLowerCase().includes(query.trim().toLowerCase()),
  );

  const toggle = (id: string, on: boolean) => {
    setChecked((prev) => {
      const next = new Set(prev);
      if (on) next.add(id);
      else next.delete(id);
      return next;
    });
  };

  const handleSave = async () => {
    if (!proxy) return;
    setIsBusy(true);
    try {
      for (const id of attach) {
        await invoke("update_profile_proxy", {
          profileId: id,
          proxyId: proxy.id,
        });
      }
      for (const id of detach) {
        await invoke("update_profile_proxy", { profileId: id, proxyId: null });
      }
      showSuccessToast(t("proxies.assign.saved", { count: changes }));
      onClose();
    } catch (error: unknown) {
      showErrorToast(translateBackendError(t, error));
    } finally {
      setIsBusy(false);
    }
  };

  return (
    <Dialog
      open={proxy !== null}
      onOpenChange={(open) => {
        if (!open && !isBusy) onClose();
      }}
    >
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            {t("proxies.assign.title", { name: proxy?.name ?? "" })}
          </DialogTitle>
          <DialogDescription>
            {t("proxies.assign.description")}
          </DialogDescription>
        </DialogHeader>
        <Input
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder={t("proxies.assign.search")}
          aria-label={t("proxies.assign.search")}
        />
        <ul className="max-h-72 space-y-0.5 overflow-y-auto">
          {visible.length === 0 && (
            <li className="px-2 py-3 text-sm text-muted-foreground">
              {t("proxies.assign.empty")}
            </li>
          )}
          {visible.map((profile) => (
            <li key={profile.id}>
              <label
                htmlFor={`proxy-assign-${profile.id}`}
                className="flex cursor-pointer items-center gap-3 rounded-md px-2 py-1.5 text-sm hover:bg-foreground/5"
              >
                <Checkbox
                  id={`proxy-assign-${profile.id}`}
                  checked={checked.has(profile.id)}
                  onCheckedChange={(value) =>
                    toggle(profile.id, value === true)
                  }
                  disabled={isBusy}
                />
                <span className="min-w-0 truncate">{profile.name}</span>
              </label>
            </li>
          ))}
        </ul>
        <DialogFooter>
          <RippleButton variant="outline" onClick={onClose} disabled={isBusy}>
            {t("common.buttons.cancel")}
          </RippleButton>
          <LoadingButton
            isLoading={isBusy}
            disabled={changes === 0}
            onClick={() => void handleSave()}
          >
            {t("proxies.assign.save")}
          </LoadingButton>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
