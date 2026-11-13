"use client";

import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { LuTimer, LuTriangleAlert } from "react-icons/lu";
import { ProxyFormDialog } from "@/components/proxy-form-dialog";
import { ProxyPicker } from "@/components/proxy-picker";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import type { StoredProxy } from "@/types";

/** The proxy the last quick profile used, so the next one is two clicks. */
const LAST_PROXY_KEY = "owl.quickProfileProxy";

function readLastProxy(): string | null {
  try {
    return window.localStorage.getItem(LAST_PROXY_KEY);
  } catch {
    return null;
  }
}

function rememberProxy(proxyId: string | null) {
  try {
    if (proxyId) window.localStorage.setItem(LAST_PROXY_KEY, proxyId);
    else window.localStorage.removeItem(LAST_PROXY_KEY);
  } catch {
    // Remembering is a convenience; the launch does not depend on it.
  }
}

interface QuickProfileDialogProps {
  isOpen: boolean;
  onClose: () => void;
  proxies: StoredProxy[];
  starting: boolean;
  onStart: (proxyId: string | null) => Promise<void>;
}

/**
 * Picks the route of a quick profile before it is made and launched: the
 * last one used is preselected, "Add proxy" creates one in place, and a
 * launch without a route is refused up front when the settings block it.
 */
export function QuickProfileDialog({
  isOpen,
  onClose,
  proxies,
  starting,
  onStart,
}: QuickProfileDialogProps) {
  const { t } = useTranslation();
  const [proxyId, setProxyId] = useState<string | null>(null);
  const [routeRequired, setRouteRequired] = useState(false);
  const [isProxyFormOpen, setIsProxyFormOpen] = useState(false);

  const proxiesRef = useRef(proxies);
  proxiesRef.current = proxies;

  // Only on opening: a proxy made here arrives in `proxies` a moment later
  // and must not reset the choice.
  useEffect(() => {
    if (!isOpen) return;
    const last = readLastProxy();
    setProxyId(
      last && proxiesRef.current.some((proxy) => proxy.id === last)
        ? last
        : null,
    );
    void invoke<{ require_route_for_launch?: boolean }>("get_app_settings")
      .then((settings) => {
        setRouteRequired(Boolean(settings.require_route_for_launch));
      })
      .catch(() => {
        setRouteRequired(false);
      });
  }, [isOpen]);

  const blocked = routeRequired && proxyId === null;

  const start = async () => {
    if (blocked || starting) return;
    rememberProxy(proxyId);
    await onStart(proxyId);
    onClose();
  };

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <LuTimer className="size-4" aria-hidden="true" />
            {t("header.quickProfile")}
          </DialogTitle>
          <DialogDescription>{t("header.quickProfileHint")}</DialogDescription>
        </DialogHeader>
        <div className="grid gap-2">
          <Label>{t("profileInfo.fields.proxy")}</Label>
          <ProxyPicker
            proxies={proxies}
            value={proxyId}
            ariaLabel={t("profileInfo.fields.proxy")}
            onSelect={setProxyId}
            onAdd={() => {
              setIsProxyFormOpen(true);
            }}
          />
          {blocked && (
            <p
              role="status"
              className="flex items-start gap-1.5 text-xs text-warning-text"
            >
              <LuTriangleAlert
                className="mt-0.5 size-3.5 shrink-0"
                aria-hidden="true"
              />
              {t("quickProfile.routeRequired")}
            </p>
          )}
        </div>
        <DialogFooter>
          <Button type="button" variant="outline" onClick={onClose}>
            {t("common.buttons.cancel")}
          </Button>
          <Button
            type="button"
            disabled={blocked || starting}
            onClick={() => void start()}
          >
            {t("quickProfile.launch")}
          </Button>
        </DialogFooter>
      </DialogContent>
      <ProxyFormDialog
        isOpen={isProxyFormOpen}
        onClose={() => {
          setIsProxyFormOpen(false);
        }}
        onCreated={(proxy) => {
          setProxyId(proxy.id);
        }}
      />
    </Dialog>
  );
}
